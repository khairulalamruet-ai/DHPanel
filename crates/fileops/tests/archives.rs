//! Packing and unpacking on a real directory, with hostile archives.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::{
    io::Write as _,
    os::unix::fs::{MetadataExt as _, symlink},
    path::PathBuf,
};

use dhpanel_fileops::handle;
use dhpanel_node_protocol::files::{ArchiveFormat, FileErrorCode, FileRequest as R, FileResult};
use dhpanel_safe_fs::Root;

struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("dh-arch-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("tenant")).unwrap();
        Self(dir)
    }
    fn root(&self) -> Root {
        Root::open(&self.0.join("tenant")).unwrap()
    }
    fn path(&self, rel: &str) -> PathBuf {
        self.0.join("tenant").join(rel)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn code(r: FileResult) -> FileErrorCode {
    match r {
        FileResult::Error { code, .. } => code,
        other => panic!("expected an error, got {other:?}"),
    }
}

fn done(r: FileResult) -> u32 {
    match r {
        FileResult::Done { count } => count,
        other => panic!("expected done, got {other:?}"),
    }
}

fn extract(root: &Root, path: &str, dest: &str) -> FileResult {
    handle(
        root,
        R::Extract {
            path: path.into(),
            dest: dest.into(),
            max_bytes: 1 << 20,
            max_entries: 100,
        },
    )
}

fn zip_with(entries: &[(&str, &[u8], Option<u32>)]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, data, mode) in entries {
        let mut options = zip::write::SimpleFileOptions::default();
        if let Some(m) = mode {
            options = options.unix_permissions(*m);
        }
        zip.start_file(*name, options).unwrap();
        zip.write_all(data).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

/// A tar.gz with entries written byte by byte, so names the library would refuse
/// can be tried.
fn tar_gz_with(entries: &[(&str, tar::EntryType, &[u8], u32, &str)]) -> Vec<u8> {
    let mut raw = Vec::new();
    for (name, kind, data, mode, link) in entries {
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(*kind);
        header.set_size(data.len() as u64);
        header.set_mode(*mode);
        {
            let old = header.as_old_mut();
            old.name[..name.len()].copy_from_slice(name.as_bytes());
            old.linkname[..link.len()].copy_from_slice(link.as_bytes());
        }
        header.set_cksum();
        raw.extend_from_slice(header.as_bytes());
        raw.extend_from_slice(data);
        raw.resize(raw.len().div_ceil(512) * 512, 0);
    }
    raw.extend_from_slice(&[0u8; 1024]);
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    gz.write_all(&raw).unwrap();
    gz.finish().unwrap()
}

#[test]
fn what_is_packed_comes_out_the_same_in_both_formats() {
    for (format, ext) in [
        (ArchiveFormat::Zip, "zip"),
        (ArchiveFormat::TarGz, "tar.gz"),
    ] {
        let s = Scratch::new(ext);
        let root = s.root();
        std::fs::create_dir_all(s.path("site/sub")).unwrap();
        std::fs::write(s.path("site/a.txt"), "alpha").unwrap();
        std::fs::write(s.path("site/sub/b.txt"), "beta").unwrap();
        std::fs::write(s.path("single.txt"), "one").unwrap();
        symlink("/etc/passwd", s.path("site/link")).unwrap();
        let pack = |dest: &str, max_bytes| {
            handle(
                &root,
                R::Archive {
                    paths: vec!["site".into(), "single.txt".into()],
                    dest: dest.into(),
                    format,
                    max_bytes,
                },
            )
        };
        assert_eq!(done(pack(&format!("out.{ext}"), 1 << 20)), 5, "{ext}");
        assert_eq!(
            code(pack(&format!("out.{ext}"), 1 << 20)),
            FileErrorCode::Exists
        );
        assert_eq!(
            code(pack(&format!("small.{ext}"), 3)),
            FileErrorCode::TooLarge
        );
        assert!(
            !s.path(&format!("small.{ext}")).exists(),
            "no half archive is left"
        );
        assert_eq!(
            done(extract(&root, &format!("out.{ext}"), "back")),
            5,
            "{ext}"
        );
        assert_eq!(
            std::fs::read_to_string(s.path("back/site/a.txt")).unwrap(),
            "alpha"
        );
        assert_eq!(
            std::fs::read_to_string(s.path("back/site/sub/b.txt")).unwrap(),
            "beta"
        );
        assert_eq!(
            std::fs::read_to_string(s.path("back/single.txt")).unwrap(),
            "one"
        );
        assert!(!s.path("back/site/link").exists(), "links are not packed");
        // Nothing is replaced: unpacking again says so.
        assert_eq!(
            code(extract(&root, &format!("out.{ext}"), "back")),
            FileErrorCode::Exists
        );
    }
}

#[test]
fn an_archive_that_would_leave_the_folder_is_refused_before_anything_is_written() {
    let s = Scratch::new("slip");
    let root = s.root();
    std::fs::write(s.0.join("victim"), "keep").unwrap();
    for (name, bytes) in [
        (
            "evil1.zip",
            zip_with(&[("fine.txt", b"x", None), ("../../etc/passwd", b"pwn", None)]),
        ),
        ("evil2.zip", zip_with(&[("/etc/passwd", b"pwn", None)])),
        ("evil3.zip", zip_with(&[("a/../../victim", b"pwn", None)])),
        ("evil4.zip", zip_with(&[("a\\..\\b", b"pwn", None)])),
        (
            "evil5.tar.gz",
            tar_gz_with(&[
                ("fine.txt", tar::EntryType::Regular, b"x", 0o644, ""),
                ("../victim", tar::EntryType::Regular, b"pwn", 0o644, ""),
            ]),
        ),
        (
            "evil6.tar.gz",
            tar_gz_with(&[("/etc/cron.d/x", tar::EntryType::Regular, b"pwn", 0o644, "")]),
        ),
    ] {
        std::fs::write(s.path(name), bytes).unwrap();
        let dest = format!("into-{name}");
        assert_eq!(
            code(extract(&root, name, &dest)),
            FileErrorCode::BadArchive,
            "{name}"
        );
        assert!(
            !s.path(&dest).exists(),
            "{name}: not even the folder was made"
        );
        assert!(!s.path("fine.txt").exists() && !s.path(&format!("{dest}/fine.txt")).exists());
    }
    assert_eq!(std::fs::read_to_string(s.0.join("victim")).unwrap(), "keep");
    // Not an archive at all, or damaged.
    std::fs::write(s.path("text.zip"), "just text").unwrap();
    assert_eq!(
        code(extract(&root, "text.zip", "t")),
        FileErrorCode::BadArchive
    );
    let mut cut = zip_with(&[("a.txt", &[7u8; 5000], None)]);
    cut.truncate(cut.len() / 2);
    std::fs::write(s.path("cut.zip"), cut).unwrap();
    assert_eq!(
        code(extract(&root, "cut.zip", "c")),
        FileErrorCode::BadArchive
    );
}

#[test]
fn links_are_skipped_modes_are_cut_and_limits_hold() {
    let s = Scratch::new("limits");
    let root = s.root();
    let tgz = tar_gz_with(&[
        ("dir/", tar::EntryType::Directory, b"", 0o000, ""),
        (
            "dir/run.sh",
            tar::EntryType::Regular,
            b"#!/bin/sh",
            0o4755,
            "",
        ),
        ("dir/secret", tar::EntryType::Regular, b"s", 0o000, ""),
        (
            "dir/link",
            tar::EntryType::Symlink,
            b"",
            0o777,
            "/etc/passwd",
        ),
        ("dir/hard", tar::EntryType::Link, b"", 0o777, "dir/run.sh"),
    ]);
    std::fs::write(s.path("a.tgz"), tgz).unwrap();
    assert_eq!(
        done(extract(&root, "a.tgz", "out")),
        3,
        "two files and a folder; the links are skipped"
    );
    assert!(!s.path("out/dir/link").exists() && !s.path("out/dir/hard").exists());
    let mode = |p: &str| std::fs::metadata(s.path(p)).unwrap().mode() & 0o7777;
    assert_eq!(mode("out/dir/run.sh"), 0o755, "no setuid bit");
    assert_eq!(
        mode("out/dir/secret") & 0o600,
        0o600,
        "the owner can always use it"
    );
    assert_eq!(mode("out/dir") & 0o700, 0o700);
    // A zip that is bigger than the limit, or has too many entries.
    std::fs::write(
        s.path("big.zip"),
        zip_with(&[("a", &vec![0u8; 5000], None)]),
    )
    .unwrap();
    let r = handle(
        &root,
        R::Extract {
            path: "big.zip".into(),
            dest: "b".into(),
            max_bytes: 4000,
            max_entries: 10,
        },
    );
    assert_eq!(code(r), FileErrorCode::TooLarge);
    std::fs::write(
        s.path("many.zip"),
        zip_with(&[("a", b"1", None), ("b", b"2", None), ("c", b"3", None)]),
    )
    .unwrap();
    let r = handle(
        &root,
        R::Extract {
            path: "many.zip".into(),
            dest: "m".into(),
            max_bytes: 100,
            max_entries: 2,
        },
    );
    assert_eq!(code(r), FileErrorCode::TooMany);
    assert!(!s.path("b").exists() && !s.path("m").exists());
    // Into a link, or out of the root: refused.
    symlink("/tmp", s.path("tmp-link")).unwrap();
    assert!(matches!(
        code(extract(&root, "a.tgz", "tmp-link/x")),
        FileErrorCode::Link | FileErrorCode::NotDir
    ));
    assert_eq!(
        code(extract(&root, "a.tgz", "../x")),
        FileErrorCode::InvalidPath
    );
    assert_eq!(
        code(extract(&root, "../a.tgz", "x")),
        FileErrorCode::InvalidPath
    );
    assert_eq!(
        code(extract(&root, "missing.zip", "x")),
        FileErrorCode::NotFound
    );
}
