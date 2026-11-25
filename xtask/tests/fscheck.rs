//! `cargo xtask fscheck` (spec/harness.md §11.2) over a temporary
//! directory.
//!
//! The rows run with few rounds and the output has its form. The status is
//! 0 when every row relied on is clean: on a file system that passes.
//! These tests cannot make a row fail through the file system (the
//! workspace mount fails `link-unlink-create`, `link-rename-over` and
//! perhaps `tmp-again-after-link` only sometimes, and a local file system
//! never), so they plant a wrong read through the reader instead, and they
//! assert nothing about the rows not relied on.

mod common;

use common::Fixture;
use std::path::Path;
use xtask::fscheck::{self, DEFAULT_ROUNDS, ROWS, Request};

fn args(a: &[&str]) -> Vec<String> {
    a.iter().map(|s| (*s).to_owned()).collect()
}

/// The other process of `create-new-xproc`: this package's `xtask`.
fn reader(extra: &[&str]) -> Vec<std::ffi::OsString> {
    let mut v: Vec<std::ffi::OsString> =
        vec![env!("CARGO_BIN_EXE_xtask").into(), "fscheck-reader".into()];
    v.extend(extra.iter().map(Into::into));
    v
}

fn run(req: &Request, read: fscheck::Reader<'_>) -> (u8, Vec<String>) {
    let mut lines = Vec::new();
    let code = fscheck::run(req, read, &mut |l| lines.push(l.to_owned()));
    (code, lines)
}

// §11.2: the command line.
#[test]
fn s11_2_parse() {
    let r = fscheck::parse(&args(&["d"])).unwrap();
    assert_eq!((r.dir.as_path(), r.rounds, r.rows.len()), (Path::new("d"), DEFAULT_ROUNDS, 12));
    let r = fscheck::parse(&args(&[
        "--row",
        "unique-tmp-link",
        "d",
        "--rounds",
        "5",
        "--row",
        "link-read-both",
    ]))
    .unwrap();
    assert_eq!(r.rounds, 5);
    // In the order of the table, whatever the order of the options.
    let names: Vec<&str> = r.rows.iter().map(|r| r.name).collect();
    assert_eq!(names, ["link-read-both", "unique-tmp-link"]);
    for bad in [
        &[][..],
        &["d", "e"],
        &["d", "--rounds"],
        &["d", "--rounds", "0"],
        &["d", "--rounds", "+3"],
        &["d", "--rounds", "1", "--rounds", "2"],
        &["d", "--row", "nosuch"],
        &["d", "--fast"],
        &[""],
    ] {
        assert!(fscheck::parse(&args(bad)).is_err(), "{bad:?}");
    }
}

// §11.2: every row runs, prints its line, and leaves nothing behind; a
// file system on which every relied-on row is clean gives status 0.
#[test]
fn s11_2_rows_run() {
    let f = Fixture::empty("fscheck-rows");
    let req = Request {
        dir: f.root().to_owned(),
        rounds: 20,
        rows: ROWS.to_vec(),
        reader: Some(reader(&[])),
    };
    let (code, lines) = run(&req, &|p| std::fs::read(p));
    assert_eq!(lines[0], format!("fscheck: {}: 20 rounds per row", f.root().display()));
    let rows: Vec<&String> = lines.iter().filter(|l| l.contains(" rounds 20 wrong ")).collect();
    assert_eq!(rows.len(), ROWS.len(), "{lines:#?}");
    for (line, row) in rows.iter().zip(ROWS) {
        assert!(line.starts_with(row.name), "{line}");
        let relied = if row.relied { "  relied on: " } else { "  not relied on: " };
        assert!(line.contains(relied) && line.ends_with(row.what), "{line}");
    }
    // The relied-on rows are clean here (that is what the harness and the
    // store need of a file system); the others are not asserted.
    for row in ROWS.iter().filter(|r| r.relied) {
        let line = rows.iter().find(|l| l.starts_with(row.name)).unwrap();
        assert!(line.contains(" wrong 0 "), "{line}");
    }
    assert_eq!(code, 0, "{lines:#?}");
    assert_eq!(lines.last().unwrap(), "fscheck: ok");
    assert_eq!(std::fs::read_dir(f.root()).unwrap().count(), 0, "the rows' directories removed");
}

// §11.2: a wrong read of a relied-on row is counted, its first is shown,
// and the status is 1; one of a row not relied on is counted and the
// status stays 0.
#[test]
fn s11_2_a_wrong_read_fails_a_relied_row() {
    let f = Fixture::empty("fscheck-plant");
    // Every read of a tree name gives other bytes.
    let planted = |p: &Path| -> std::io::Result<Vec<u8>> {
        if p.parent().is_some_and(|d| d.ends_with("tree")) {
            Ok(b"planted\n".to_vec())
        } else {
            std::fs::read(p)
        }
    };
    let row = |name: &str| *ROWS.iter().find(|r| r.name == name).unwrap();
    let req = Request {
        dir: f.root().to_owned(),
        rounds: 3,
        rows: vec![row("link-read-both")],
        reader: None,
    };
    let (code, lines) = run(&req, &planted);
    assert_eq!(code, 1, "{lines:#?}");
    assert!(lines[1].starts_with("link-read-both") && lines[1].contains(" wrong 6 "), "{lines:#?}");
    assert!(
        lines[2].starts_with("  first: ")
            && lines[2].ends_with("read `planted`, not `pre 0 detail`")
    );
    assert_eq!(lines.last().unwrap(), "fscheck: FAILED (link-read-both)");

    let req = Request {
        dir: f.root().to_owned(),
        rounds: 3,
        rows: vec![row("link-unlink-create"), row("copy-unlink-create")],
        reader: None,
    };
    let (code, lines) = run(&req, &planted);
    assert_eq!(code, 1);
    assert!(lines[1].contains(" wrong 3 ") && lines[1].contains("not relied on"), "{lines:#?}");
    assert_eq!(lines.last().unwrap(), "fscheck: FAILED (copy-unlink-create)");

    let req = Request {
        dir: f.root().to_owned(),
        rounds: 2,
        rows: vec![row("link-rename-over")],
        reader: None,
    };
    let (code, lines) = run(&req, &planted);
    assert_eq!((code, lines.last().unwrap().as_str()), (0, "fscheck: ok"), "{lines:#?}");

    // WP-161: the store's lock and journal rely on `create-new-read`, and
    // no longer on `unique-tmp-link`.
    assert!(row("create-new-read").relied);
    assert!(!row("unique-tmp-link").relied);
    let req = Request {
        dir: f.root().to_owned(),
        rounds: 3,
        rows: vec![row("create-new-read")],
        reader: None,
    };
    let (code, lines) = run(&req, &planted);
    assert_eq!(code, 1, "{lines:#?}");
    assert!(
        lines[1].starts_with("create-new-read") && lines[1].contains(" wrong 6 "),
        "{lines:#?}"
    );
    assert!(lines[2].ends_with("read `planted`, not `pre 0 detail`"), "{lines:#?}");
    assert_eq!(lines.last().unwrap(), "fscheck: FAILED (create-new-read)");
    let req = Request {
        dir: f.root().to_owned(),
        rounds: 3,
        rows: vec![row("unique-tmp-link")],
        reader: None,
    };
    let (code, lines) = run(&req, &planted);
    assert_eq!((code, lines.last().unwrap().as_str()), (0, "fscheck: ok"), "{lines:#?}");
}

// §11.2, WP-161: `create-new-xproc` is relied on; a short read by the
// other process (planted: it drops the last byte of round 8's file, a
// 256 KiB one) is its wrong read, shown, and the status is 1.
#[test]
fn s11_2_create_new_xproc() {
    let f = Fixture::empty("fscheck-xproc");
    let row = *ROWS.iter().find(|r| r.name == "create-new-xproc").unwrap();
    assert!(row.relied);
    let req = Request {
        dir: f.root().to_owned(),
        rounds: 10,
        rows: vec![row],
        reader: Some(reader(&["--short-at", "8"])),
    };
    let (code, lines) = run(&req, &|p| std::fs::read(p));
    assert_eq!(code, 1, "{lines:#?}");
    assert!(
        lines[1].starts_with("create-new-xproc") && lines[1].contains(" wrong 1 "),
        "{lines:#?}"
    );
    assert!(
        lines[2].contains("round 8, after the creation: the other process: wrong: ")
            && lines[2].ends_with(&format!(
                "j8 read {} bytes, not {}",
                fscheck::XPROC_BIG - 1,
                fscheck::XPROC_BIG
            )),
        "{lines:#?}"
    );
    assert_eq!(lines.last().unwrap(), "fscheck: FAILED (create-new-xproc)");
    // A reader that cannot start is an error of the row.
    let req = Request {
        dir: f.root().to_owned(),
        rounds: 1,
        rows: vec![row],
        reader: Some(vec![f.path("no-such-program").into()]),
    };
    let (code, lines) = run(&req, &|p| std::fs::read(p));
    assert_eq!(code, 1, "{lines:#?}");
    assert!(lines[1].starts_with("create-new-xproc") && lines[1].contains("error: "), "{lines:#?}");
}

// §11.2: a directory that does not exist is status 1.
#[test]
fn s11_2_no_directory() {
    let f = Fixture::empty("fscheck-none");
    let req = Request { dir: f.path("none"), rounds: 1, rows: ROWS.to_vec(), reader: None };
    let (code, lines) = run(&req, &|p| std::fs::read(p));
    assert_eq!(code, 1);
    assert!(lines[0].starts_with(&format!("fscheck: {}: ", f.path("none").display())), "{lines:?}");
}

// §11.5: `inode-stable` reads the numbers of the files it creates, at once
// and after its wait; a file whose numbers changed is a wrong read of a
// relied-on row (planted here through the reader of numbers: a local file
// system never changes them), shown with its numbers, and the status is 1.
#[test]
fn s11_5_inode_stable() {
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    use std::time::Instant;
    let f = Fixture::empty("fscheck-inode");
    let row = *ROWS.iter().find(|r| r.name == "inode-stable").unwrap();
    assert!(row.relied);
    let req = Request { dir: f.root().to_owned(), rounds: 5, rows: vec![row], reader: None };
    // The real numbers: clean here, and the row waits.
    let t0 = Instant::now();
    let (code, lines) = run(&req, &|p| std::fs::read(p));
    assert!(t0.elapsed() >= fscheck::INODE_WAIT, "the row waited");
    assert_eq!(code, 0, "{lines:#?}");
    assert!(lines[1].starts_with("inode-stable") && lines[1].contains(" rounds 5 wrong 0 "));
    assert!(
        lines[1].ends_with(
            "relied on: a file created fresh keeps its device and inode numbers for 2 s"
        )
    );
    // Planted: the numbers of `f3.h` change after the first read.
    let seen: RefCell<BTreeMap<std::path::PathBuf, u64>> = RefCell::new(BTreeMap::new());
    let planted = |p: &Path| -> std::io::Result<(u64, u64)> {
        let (dev, ino) = fscheck::identity(p)?;
        let mut seen = seen.borrow_mut();
        let n = seen.entry(p.to_owned()).or_insert(0);
        *n += 1;
        Ok(if p.ends_with("f3.h") && *n > 1 { (dev, ino + 1) } else { (dev, ino) })
    };
    let mut lines = Vec::new();
    let code = fscheck::run_with(&req, &|p| std::fs::read(p), &planted, &mut |l| {
        lines.push(l.to_owned());
    });
    assert_eq!(code, 1, "{lines:#?}");
    assert!(lines[1].contains(" rounds 5 wrong 1 "), "{lines:#?}");
    assert!(
        lines[2].starts_with("  first: ")
            && lines[2].contains("f3.h was ")
            && lines[2].contains(", then "),
        "{lines:#?}"
    );
    assert_eq!(lines.last().unwrap(), "fscheck: FAILED (inode-stable)");
    // Every file was read when made, at once and after the wait (the one
    // changed is not read again); none is left.
    let want = |p: &Path| if p.ends_with("f3.h") { 2 } else { 3 };
    assert!(seen.borrow().iter().all(|(p, n)| *n == want(p)), "{:?}", seen.borrow());
    assert_eq!(seen.borrow().len(), 5);
    assert_eq!(std::fs::read_dir(f.root()).unwrap().count(), 0, "the row's directory removed");
}
