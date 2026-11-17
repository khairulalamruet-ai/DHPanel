//! Taking parts out of a backup (T15.04): one file (a database dump) or a folder's files,
//! packed again for the account's own files.
//!
//! Both read the archive once from the start and write what they take to a [`Write`], so
//! nothing is held in memory and nothing is written to the disk by this crate. The paths are
//! those of [`crate::Archive`]: relative to the archive's root folder, which is told apart from
//! a top-level folder the same way a reader does (an entry is looked for both as it is and
//! without its first folder).

use std::io::{BufRead as _, Read, Write};

use crate::ImportError;

fn err(e: impl std::fmt::Display) -> ImportError {
    ImportError::Archive(e.to_string())
}

/// A tar reader over a gzip stream or a plain tar.
fn tar_of<'a, R: Read + 'a>(reader: R) -> Result<tar::Archive<Box<dyn Read + 'a>>, ImportError> {
    let mut reader = std::io::BufReader::new(reader);
    let gz = reader.fill_buf().map_err(err)?.starts_with(&[0x1f, 0x8b]);
    let boxed: Box<dyn Read + 'a> = if gz {
        Box::new(flate2::read::GzDecoder::new(reader))
    } else {
        Box::new(reader)
    };
    Ok(tar::Archive::new(boxed))
}

/// An entry's path as the model writes it: cleaned, and safe (no `..`, nothing absolute).
fn clean(path: &str) -> Result<Option<String>, ImportError> {
    let path = path.trim_end_matches('/').trim_start_matches("./");
    if path.is_empty() || path == "." {
        return Ok(None);
    }
    if path.starts_with('/')
        || path.contains('\\')
        || path.contains('\0')
        || path.split('/').any(|p| p == ".." || p.is_empty())
    {
        return Err(ImportError::UnsafePath(path.chars().take(200).collect()));
    }
    Ok(Some(path.to_owned()))
}

/// The path with its first folder taken off, when there is one.
fn below_root(path: &str) -> Option<&str> {
    path.split_once('/').map(|(_, rest)| rest)
}

/// Copies the content of one file of the archive to `out`. Returns its size.
///
/// # Errors
/// [`ImportError`]: a broken archive, an unsafe path, or the file is not there.
pub fn copy_file<R: Read, W: Write>(
    reader: R,
    wanted: &str,
    out: &mut W,
) -> Result<u64, ImportError> {
    let mut tar = tar_of(reader)?;
    for entry in tar.entries().map_err(err)? {
        let mut entry = entry.map_err(err)?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let path = entry.path().map_err(err)?.to_string_lossy().into_owned();
        let Some(path) = clean(&path)? else { continue };
        if path == wanted || below_root(&path) == Some(wanted) {
            return std::io::copy(&mut entry, out).map_err(err);
        }
    }
    Err(ImportError::Invalid(format!(
        "`{wanted}` is not in the archive"
    )))
}

/// What [`repack`] did.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Repacked {
    /// Files and folders written.
    pub entries: u64,
    /// Bytes of file content written.
    pub bytes: u64,
    /// Links and special files that were left out.
    pub skipped: u64,
}

/// Writes a `.tar.gz` of the archive's entries that `map` wants. `map` gets an entry's path
/// (relative to the archive's root) and answers the path it gets in the new archive, or `None`
/// to leave it out. Only plain files and folders are copied; links, devices and the like are
/// left out (and counted), so unpacking the result cannot reach outside its folder.
///
/// # Errors
/// [`ImportError`]: a broken archive, an unsafe path, a path `map` makes unsafe, or `out` failing.
pub fn repack<R: Read, W: Write>(
    reader: R,
    map: impl Fn(&str) -> Option<String>,
    out: W,
) -> Result<Repacked, ImportError> {
    let mut tar = tar_of(reader)?;
    let gz = flate2::write::GzEncoder::new(out, flate2::Compression::fast());
    let mut builder = tar::Builder::new(gz);
    let mut done = Repacked::default();
    for entry in tar.entries().map_err(err)? {
        let mut entry = entry.map_err(err)?;
        let kind = entry.header().entry_type();
        let path = entry.path().map_err(err)?.to_string_lossy().into_owned();
        let Some(path) = clean(&path)? else { continue };
        // The archive's root folder is not known here: try the path as it is, then without
        // its first folder.
        let Some(new) = map(&path).or_else(|| below_root(&path).and_then(&map)) else {
            continue;
        };
        let Some(new) = clean(&new)? else { continue };
        if !(kind.is_file() || kind.is_dir()) {
            done.skipped += 1;
            continue;
        }
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(kind);
        header.set_mode(entry.header().mode().unwrap_or(0o644) & 0o777);
        header.set_mtime(entry.header().mtime().unwrap_or(0));
        header.set_uid(0);
        header.set_gid(0);
        if kind.is_dir() {
            header.set_size(0);
            builder
                .append_data(&mut header, &new, std::io::empty())
                .map_err(err)?;
        } else {
            let size = entry.header().size().map_err(err)?;
            header.set_size(size);
            builder
                .append_data(&mut header, &new, &mut entry)
                .map_err(err)?;
            done.bytes += size;
        }
        done.entries += 1;
    }
    builder.into_inner().map_err(err)?.finish().map_err(err)?;
    Ok(done)
}
