//! Reading the table of contents of a backup archive (tar, plain or gzip).
//!
//! The archive is read once, front to back. File contents are kept only when they are
//! small (and not part of the home directory): they are the panel's own metadata. What the
//! account holds (its files, mail, dumps) is not read here, only counted or named; the
//! executor streams it out later. Nothing is ever extracted to disk by this module.

use std::collections::BTreeMap;
use std::io::Read;

use crate::ImportError;

/// What is allowed to be read.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Most entries in the archive.
    pub max_entries: u64,
    /// Largest file whose content is kept.
    pub max_small_file: u64,
    /// Most bytes of content kept in all.
    pub max_small_total: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_entries: 20_000_000,
            max_small_file: 4 * 1024 * 1024,
            max_small_total: 256 * 1024 * 1024,
        }
    }
}

/// The directory (below the archive's root) whose entries are counted, not listed.
pub const HOME: &str = "homedir";

/// What the archive says about itself.
#[derive(Debug, Default)]
pub struct Archive {
    /// The one top-level folder every entry sits in (without the trailing `/`), if any.
    pub root: Option<String>,
    /// Entries other than those under the home directory: path (relative to the root) to size.
    pub files: BTreeMap<String, u64>,
    /// Directories other than those under the home directory.
    pub dirs: Vec<String>,
    /// Content of the small files.
    content: BTreeMap<String, Vec<u8>>,
    /// Entries under the home directory.
    pub home_entries: u64,
    /// Bytes of files under the home directory.
    pub home_bytes: u64,
    /// Paths under the home directory that matter to a reader (see [`Archive::scan`]).
    pub home_paths: BTreeMap<String, u64>,
    /// `homedir/mail/<domain>/<mailbox>` folders: the mail of one mailbox each.
    pub mail_dirs: std::collections::BTreeSet<String>,
}

impl Archive {
    /// Reads the table of contents. `reader` may be a gzip stream or a plain tar.
    /// Paths under the home directory are listed in [`Archive::home_paths`] only when
    /// `keep_home` says so (for example the mail directories' top level).
    ///
    /// # Errors
    /// [`ImportError::Archive`] for a broken archive, [`ImportError::UnsafePath`] for a
    /// path that escapes the archive, [`ImportError::TooBig`] past the limits.
    pub fn scan<R: Read>(
        reader: R,
        limits: Limits,
        keep_home: impl Fn(&str) -> bool,
    ) -> Result<Self, ImportError> {
        let mut head = [0u8; 2];
        let mut reader = std::io::BufReader::new(reader);
        {
            use std::io::BufRead;
            let buf = reader.fill_buf().map_err(archive_error)?;
            head[..buf.len().min(2)].copy_from_slice(&buf[..buf.len().min(2)]);
        }
        if head == [0x1f, 0x8b] {
            let gz = flate2::read::GzDecoder::new(reader);
            Self::scan_tar(gz, limits, &keep_home)
        } else {
            Self::scan_tar(reader, limits, &keep_home)
        }
    }

    fn scan_tar<R: Read>(
        reader: R,
        limits: Limits,
        keep_home: &impl Fn(&str) -> bool,
    ) -> Result<Self, ImportError> {
        let mut tar = tar::Archive::new(reader);
        let mut raw: Vec<(String, u64, bool, Option<Vec<u8>>)> = Vec::new();
        let mut entries = 0u64;
        let mut kept = 0u64;
        let mut roots = std::collections::BTreeSet::new();
        let mut any_top_file = false;
        for entry in tar.entries().map_err(archive_error)? {
            let mut entry = entry.map_err(archive_error)?;
            entries += 1;
            if entries > limits.max_entries {
                return Err(ImportError::TooBig("too many entries".into()));
            }
            let path = entry
                .path()
                .map_err(archive_error)?
                .to_string_lossy()
                .into_owned();
            let path = path
                .trim_end_matches('/')
                .trim_start_matches("./")
                .to_owned();
            if path.is_empty() || path == "." {
                continue;
            }
            if path.starts_with('/')
                || path.contains('\\')
                || path.contains('\0')
                || path.split('/').any(|p| p == ".." || p.is_empty())
            {
                return Err(ImportError::UnsafePath(path.chars().take(200).collect()));
            }
            let kind = entry.header().entry_type();
            let is_dir = kind.is_dir();
            let is_file = kind.is_file();
            let size = entry.header().size().unwrap_or(0);
            match path.split_once('/') {
                Some((top, _)) => {
                    roots.insert(top.to_owned());
                }
                None if is_dir => {
                    roots.insert(path.clone());
                }
                None => any_top_file = true,
            }
            let small =
                is_file && size <= limits.max_small_file && kept + size <= limits.max_small_total;
            // Whether the path is under the home directory is decided after the root is
            // known, so small files are read now and dropped later if they turn out to be
            // part of the home directory. Large and non-file entries carry no content.
            let wanted = home_rel(&path).is_none_or(keep_home);
            let content = if small && wanted {
                let mut buf = Vec::with_capacity(usize::try_from(size).unwrap_or(0));
                entry.read_to_end(&mut buf).map_err(archive_error)?;
                kept += size;
                Some(buf)
            } else {
                None
            };
            raw.push((path, size, is_dir, content));
        }
        let root = if roots.len() == 1 && !any_top_file {
            roots.into_iter().next()
        } else {
            None
        };
        let mut a = Archive {
            root: root.clone(),
            ..Archive::default()
        };
        for (path, size, is_dir, content) in raw {
            let rel = match &root {
                Some(r) => match path.strip_prefix(r.as_str()) {
                    Some("") => continue,
                    Some(rest) => rest.trim_start_matches('/').to_owned(),
                    None => path,
                },
                None => path,
            };
            if rel.is_empty() {
                continue;
            }
            if rel == HOME || rel.starts_with("homedir/") {
                a.home_entries += 1;
                if !is_dir {
                    a.home_bytes += size;
                }
                if let Some(rest) = rel.strip_prefix("homedir/mail/") {
                    let mut parts = rest.split('/');
                    if let (Some(d), Some(m)) = (parts.next(), parts.next()) {
                        a.mail_dirs.insert(format!("homedir/mail/{d}/{m}"));
                    }
                }
                if keep_home(&rel) {
                    if let Some(c) = content {
                        a.content.insert(rel.clone(), c);
                    }
                    a.home_paths.insert(rel, size);
                }
                continue;
            }
            if is_dir {
                a.dirs.push(rel);
            } else {
                if let Some(c) = content {
                    a.content.insert(rel.clone(), c);
                }
                a.files.insert(rel, size);
            }
        }
        Ok(a)
    }

    /// Whether a file (outside the home directory) exists.
    #[must_use]
    pub fn has(&self, path: &str) -> bool {
        self.files.contains_key(path)
    }

    /// Whether a directory exists, or any file is below it.
    #[must_use]
    pub fn has_dir(&self, path: &str) -> bool {
        let prefix = format!("{path}/");
        self.dirs
            .iter()
            .any(|d| d == path || d.starts_with(&prefix))
            || self.files.keys().any(|f| f.starts_with(&prefix))
            || self
                .home_paths
                .keys()
                .any(|f| f == path || f.starts_with(&prefix))
            || self.mail_dirs.contains(path)
    }

    /// Text of a small file, with invalid UTF-8 replaced.
    #[must_use]
    pub fn text(&self, path: &str) -> Option<String> {
        self.content
            .get(path)
            .map(|b| String::from_utf8_lossy(b).into_owned())
    }

    /// The files directly below `dir` (outside the home directory), with their sizes.
    pub fn files_in<'a>(&'a self, dir: &str) -> impl Iterator<Item = (&'a str, u64)> + 'a {
        let prefix = format!("{dir}/");
        self.files.iter().filter_map(move |(p, s)| {
            let rest = p.strip_prefix(&prefix)?;
            (!rest.contains('/')).then_some((rest, *s))
        })
    }
}

/// The part of `path` that starts at the home directory, whether or not the archive has a
/// root folder (the root is unknown while scanning).
fn home_rel(path: &str) -> Option<&str> {
    if path.starts_with("homedir/") {
        return Some(path);
    }
    let (_, rest) = path.split_once('/')?;
    (rest.starts_with("homedir/")).then_some(rest)
}

fn archive_error(err: std::io::Error) -> ImportError {
    ImportError::Archive(err.to_string())
}
