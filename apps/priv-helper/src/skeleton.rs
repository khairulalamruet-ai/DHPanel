//! The default files a new tenant starts with (T04.14).
//!
//! They are written while the tenant's `www` directory is still the helper's own
//! (before it is handed over), so no privileged code ever touches a directory a
//! tenant controls. Text only: the request is one line of JSON of at most
//! [`crate::protocol::MAX_REQUEST_BYTES`].
//!
//! Every path is checked here, whatever the agent checked: relative, plain
//! components, no `.` or `..`, limited depth and size, no duplicates, and no file
//! that is also a directory of another file.

use std::{
    fs, io,
    os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _},
    path::Path,
};

use crate::protocol::{ErrorCode, SkeletonFile};

/// Most files in one request.
pub const MAX_FILES: usize = 20;
/// Largest single file, in bytes.
pub const MAX_FILE_BYTES: usize = 16 * 1024;
/// Most bytes in all files together.
pub const MAX_TOTAL_BYTES: usize = 24 * 1024;
/// Deepest nesting (a file directly in `www` is depth 1).
pub const MAX_DEPTH: usize = 6;
/// Longest single name.
pub const MAX_NAME: usize = 64;

type Failure = (ErrorCode, &'static str);

fn refuse(message: &'static str) -> Failure {
    (ErrorCode::InvalidArguments, message)
}

/// The path split into names, or why it is not acceptable.
fn components(path: &str) -> Result<Vec<&str>, Failure> {
    if path.is_empty() || path.len() > MAX_DEPTH * (MAX_NAME + 1) {
        return Err(refuse("a skeleton file path has a bad length"));
    }
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() > MAX_DEPTH {
        return Err(refuse("a skeleton file path is nested too deeply"));
    }
    for part in &parts {
        let plain = !part.is_empty()
            && part.len() <= MAX_NAME
            && *part != "."
            && *part != ".."
            && part
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
        if !plain {
            return Err(refuse(
                "skeleton file names may hold letters, digits, dot, dash and underscore only",
            ));
        }
    }
    Ok(parts)
}

/// Checks the whole list.
///
/// # Errors
/// [`ErrorCode::InvalidArguments`] naming the first problem.
pub fn check(files: &[SkeletonFile]) -> Result<(), Failure> {
    if files.len() > MAX_FILES {
        return Err(refuse("too many skeleton files"));
    }
    let mut total = 0usize;
    let mut seen: Vec<&str> = Vec::new();
    for file in files {
        components(&file.path)?;
        if file.content.len() > MAX_FILE_BYTES {
            return Err(refuse("a skeleton file is too large"));
        }
        if file
            .content
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
        {
            return Err(refuse("a skeleton file holds control characters"));
        }
        total += file.content.len();
        if total > MAX_TOTAL_BYTES {
            return Err(refuse("the skeleton files are too large together"));
        }
        if seen.contains(&file.path.as_str()) {
            return Err(refuse("a skeleton file is listed twice"));
        }
        seen.push(&file.path);
    }
    // A name cannot be a file and a directory at once.
    for a in &seen {
        if seen
            .iter()
            .any(|b| b.len() > a.len() && b.starts_with(a) && b.as_bytes()[a.len()] == b'/')
        {
            return Err(refuse("a skeleton file is also a directory of another"));
        }
    }
    Ok(())
}

/// Writes the files below `www` (empty or half-written by an earlier attempt)
/// and hands everything created to `owner:group`.
///
/// Existing files are left as they are: this only ever runs before the
/// directory is handed over, so whatever is there is ours from a stopped attempt.
///
/// # Errors
/// Filesystem errors.
pub fn write(www: &Path, files: &[SkeletonFile], owner: u32, group: u32) -> io::Result<()> {
    let mut created: Vec<std::path::PathBuf> = Vec::new();
    for file in files {
        let parts = components(&file.path).map_err(|(_, m)| io::Error::other(m))?;
        let mut dir = www.to_path_buf();
        for part in &parts[..parts.len() - 1] {
            dir.push(part);
            match fs::create_dir(&dir) {
                Ok(()) => {
                    fs::set_permissions(&dir, fs::Permissions::from_mode(0o755))?;
                    created.push(dir.clone());
                }
                Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
                    // Never go through a link.
                    if !fs::symlink_metadata(&dir)?.file_type().is_dir() {
                        return Err(io::Error::other("a skeleton directory is not a directory"));
                    }
                }
                Err(err) => return Err(err),
            }
        }
        let path = dir.join(parts[parts.len() - 1]);
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o644)
            .open(&path)
        {
            Ok(mut out) => {
                io::Write::write_all(&mut out, file.content.as_bytes())?;
                // The service's umask is strict; the file's mode is not left to it.
                out.set_permissions(fs::Permissions::from_mode(0o644))?;
                created.push(path);
            }
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {}
            Err(err) => return Err(err),
        }
    }
    for path in created {
        std::os::unix::fs::lchown(&path, Some(owner), Some(group))?;
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    fn file(path: &str, content: &str) -> SkeletonFile {
        SkeletonFile {
            path: path.into(),
            content: content.into(),
        }
    }

    #[test]
    fn plain_relative_paths_pass_everything_else_fails() {
        assert!(
            check(&[
                file("index.html", "hi"),
                file("css/site.css", "a{}"),
                file(".htaccess", "")
            ])
            .is_ok()
        );
        for bad in [
            "",
            "/etc/passwd",
            "../x",
            "a/../x",
            "a//b",
            "a/./b",
            ".",
            "..",
            "a/",
            "/a",
            "a b",
            "a\\b",
            "näme",
            "a\nb",
            "a/b/c/d/e/f/g.txt",
        ] {
            assert!(check(&[file(bad, "x")]).is_err(), "{bad:?}");
        }
        assert!(check(&[file(&"a".repeat(65), "x")]).is_err());
    }

    #[test]
    fn counts_sizes_duplicates_and_clashes_are_limited() {
        assert!(
            check(&[file("a", "1"), file("a", "2")]).is_err(),
            "duplicate"
        );
        assert!(
            check(&[file("a", "1"), file("a/b", "2")]).is_err(),
            "file and directory"
        );
        assert!(
            check(&[file("a", "1"), file("ab/c", "2")]).is_ok(),
            "a prefix is not a parent"
        );
        assert!(check(&[file("a", &"x".repeat(MAX_FILE_BYTES + 1))]).is_err());
        assert!(check(&[file("a", "x\0y")]).is_err());
        let many: Vec<_> = (0..=MAX_FILES)
            .map(|i| file(&format!("f{i}"), ""))
            .collect();
        assert!(check(&many).is_err());
        let heavy: Vec<_> = (0..3)
            .map(|i| file(&format!("f{i}"), &"x".repeat(MAX_FILE_BYTES)))
            .collect();
        assert!(check(&heavy).is_err(), "together over the total");
    }

    #[test]
    fn files_are_written_below_the_directory_and_never_over_existing_ones() {
        let www = std::env::temp_dir().join(format!("dhph-skel-{}", std::process::id()));
        let _ = fs::remove_dir_all(&www);
        fs::create_dir_all(&www).unwrap();
        let meta = fs::metadata(&www).unwrap();
        use std::os::unix::fs::MetadataExt as _;
        let (uid, gid) = (meta.uid(), meta.gid());
        let files = [
            file("index.html", "<h1>hi</h1>"),
            file("css/site.css", "a{}"),
        ];
        write(&www, &files, uid, gid).unwrap();
        assert_eq!(
            fs::read_to_string(www.join("index.html")).unwrap(),
            "<h1>hi</h1>"
        );
        assert_eq!(fs::read_to_string(www.join("css/site.css")).unwrap(), "a{}");
        assert_eq!(
            fs::metadata(www.join("css")).unwrap().permissions().mode() & 0o777,
            0o755
        );
        assert_eq!(
            fs::metadata(www.join("index.html"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o644
        );
        // Running again leaves what is there alone.
        fs::write(www.join("index.html"), "changed").unwrap();
        write(&www, &files, uid, gid).unwrap();
        assert_eq!(
            fs::read_to_string(www.join("index.html")).unwrap(),
            "changed"
        );
        // A link in the way is never followed.
        let outside = www.with_extension("outside");
        fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, www.join("link")).unwrap();
        assert!(write(&www, &[file("link/x.txt", "no")], uid, gid).is_err());
        assert!(fs::read_dir(&outside).unwrap().next().is_none());
        let _ = fs::remove_dir_all(&www);
        let _ = fs::remove_dir_all(&outside);
    }
}
