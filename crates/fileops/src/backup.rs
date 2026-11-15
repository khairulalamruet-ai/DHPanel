//! The files of an account as one encrypted stream (T09.04): a tar, compressed with zstd,
//! cut into authenticated chunks (`dhpanel-backup`).
//!
//! The walk is streaming (nothing is listed first), opens every file from its folder's
//! descriptor without following links, skips links and special files and the trash, and
//! lets the site run meanwhile: a file that changes while it is read gives what it had
//! when it was opened (a file that shrank is padded with zeros, one that grew is cut), so
//! the archive is always whole.

use std::{
    io::{Read, Write},
    os::fd::OwnedFd,
};

use dhpanel_backup::{KEY_LEN, StreamWriter};
use dhpanel_node_protocol::files::{FileErrorCode, TRASH_DIR};
use dhpanel_safe_fs::Root;
use rustix::fs::{FileType, Mode, OFlags};

use super::{EntryKind, Out, fail, kind_of, names, stat_at, sys};

const MAX_DEPTH: usize = 128;

/// What was packed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Packed {
    /// Files and folders in the archive.
    pub entries: u64,
    /// Bytes of file contents (before compression).
    pub bytes: u64,
    /// Links and special files left out.
    pub skipped: u64,
}

/// Reads the real bytes of a file, then zeros, up to `size`: a file that shrank while it was
/// being packed still gives the length its header promised.
struct Fixed<R: Read> {
    inner: R,
    left: u64,
    ended: bool,
}

impl<R: Read> Read for Fixed<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.left == 0 {
            return Ok(0);
        }
        let want = usize::try_from(self.left)
            .unwrap_or(usize::MAX)
            .min(buf.len());
        let n = if self.ended {
            0
        } else {
            self.inner.read(&mut buf[..want])?
        };
        if n == 0 {
            self.ended = true;
            buf[..want].fill(0);
            self.left -= want as u64;
            return Ok(want);
        }
        self.left -= n as u64;
        Ok(n)
    }
}

/// Packs everything below `root` (the trash excluded) into `out`, encrypted with `key`.
///
/// # Errors
/// The disk or `out` failing (not a changing file: see the module documentation).
pub fn pack_files(root: &Root, key: &[u8; KEY_LEN], out: impl Write) -> Out<Packed> {
    let io = |e: std::io::Error| (FileErrorCode::Failed, format!("cannot pack: {}", e.kind()));
    let encrypted = StreamWriter::new(out, key).map_err(io)?;
    let compressed = zstd::stream::Encoder::new(encrypted, 3).map_err(io)?;
    let mut tar = tar::Builder::new(compressed);
    tar.mode(tar::HeaderMode::Deterministic);
    let top = sys(rustix::fs::openat(
        root.fd(),
        ".",
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    ))?;
    let mut packed = Packed::default();
    walk(&top, "", 0, &mut tar, &mut packed)?;
    let compressed = tar.into_inner().map_err(io)?;
    let encrypted = compressed.finish().map_err(io)?;
    encrypted.finish().map_err(io)?;
    Ok(packed)
}

fn walk<W: Write>(
    dir: &OwnedFd,
    path: &str,
    depth: usize,
    tar: &mut tar::Builder<W>,
    packed: &mut Packed,
) -> Out<()> {
    let io = |e: std::io::Error| (FileErrorCode::Failed, format!("cannot pack: {}", e.kind()));
    if depth > MAX_DEPTH {
        return fail(FileErrorCode::TooMany, "the files are nested too deep");
    }
    for name in names(dir)? {
        if depth == 0 && name == TRASH_DIR {
            continue;
        }
        // Gone since it was listed: not there any more, nothing to keep.
        let Ok(st) = stat_at(dir, &name) else {
            continue;
        };
        let child = if path.is_empty() {
            name.clone()
        } else {
            format!("{path}/{name}")
        };
        let mut header = tar::Header::new_gnu();
        header.set_mode(st.st_mode & 0o777);
        header.set_mtime(u64::try_from(st.st_mtime).unwrap_or(0));
        match kind_of(FileType::from_raw_mode(st.st_mode)) {
            EntryKind::Dir => {
                let fd = match sys(rustix::fs::openat(
                    dir,
                    name.as_str(),
                    OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                    Mode::empty(),
                )) {
                    Ok(fd) => fd,
                    Err(_) => continue,
                };
                header.set_entry_type(tar::EntryType::Directory);
                header.set_size(0);
                tar.append_data(&mut header, format!("{child}/"), std::io::empty())
                    .map_err(io)?;
                packed.entries += 1;
                walk(&fd, &child, depth + 1, tar, packed)?;
            }
            EntryKind::File => {
                let Ok(fd) = sys(rustix::fs::openat(
                    dir,
                    name.as_str(),
                    OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
                    Mode::empty(),
                )) else {
                    continue;
                };
                // The size of the file that is open, not of the one that was listed.
                let size = sys(rustix::fs::fstat(&fd))
                    .map(|s| u64::try_from(s.st_size).unwrap_or(0))
                    .unwrap_or(0);
                header.set_entry_type(tar::EntryType::Regular);
                header.set_size(size);
                let reader = Fixed {
                    inner: std::fs::File::from(fd),
                    left: size,
                    ended: false,
                };
                tar.append_data(&mut header, &child, reader).map_err(io)?;
                packed.entries += 1;
                packed.bytes += size;
            }
            EntryKind::Link | EntryKind::Other => packed.skipped += 1,
        }
    }
    Ok(())
}
