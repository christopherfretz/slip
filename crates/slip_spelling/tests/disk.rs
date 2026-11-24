//! `slip_spelling` on the disk (`spec/landed/edits/WP-147.md`, the
//! ruling's Q3): a case-sensitive volume whose lookups are trusted
//! (`/dev/shm`, tmpfs: nothing listed), one that is listed and does not
//! fold (`/tmp`: this container's is `overlay`), and the workspace's,
//! which folds (`CARGO_TARGET_TMPDIR`: a FUSE mount of a macOS volume).
//! Each test skips, saying why, where its location is not of its kind.
//!
//! Two of them must write outside the workspace: the workspace's own
//! volume folds, so a case-sensitive volume, trusted (`/dev/shm`) or listed
//! (`/tmp`), exists only outside it. Each makes a directory named for the
//! test, the process and a counter, and removes it by that path when the
//! test leaves by any way (a return, a skip, a failed assertion: `Drop`
//! runs on unwinding), and the directory is owned by the guard before
//! anything else can fail.

use std::cell::RefCell;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use slip_spelling::{Disk, Id, Lookup, Spelling, SpellingError, real_path, respelled};

/// The disk, counting its listings.
#[derive(Debug, Default)]
struct Counted {
    disk: Disk,
    listed: RefCell<Vec<Vec<u8>>>,
}

impl Lookup for Counted {
    fn lstat(&self, p: &[u8]) -> io::Result<Option<Id>> {
        self.disk.lstat(p)
    }
    fn names(&self, dir: &[u8]) -> io::Result<Option<Vec<Vec<u8>>>> {
        self.listed.borrow_mut().push(dir.to_vec());
        self.disk.names(dir)
    }
    fn consistent(&self, dir: &[u8], dev: u64) -> bool {
        self.disk.consistent(dir, dev)
    }
}

/// A directory of its own under `base`, removed on drop by its path
/// (retrying the workspace mount's occasional `ENOTEMPTY`).
struct Scratch(PathBuf);

impl Scratch {
    fn new(base: &Path, name: &str) -> Option<Scratch> {
        static N: AtomicU32 = AtomicU32::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let made = Scratch(base.join(format!("slip-spelling-{name}-{}-{n}", std::process::id())));
        std::fs::create_dir(&made.0).ok()?;
        let real = std::fs::canonicalize(&made.0).ok()?;
        // Removed by its real path from here on (the same directory).
        let mut made = made;
        made.0 = real;
        Some(made)
    }

    fn write(&self, rel: &str) -> PathBuf {
        let p = self.0.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, b"x\n").unwrap();
        p
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        for _ in 0..20 {
            match std::fs::remove_dir_all(&self.0) {
                Err(e) if e.kind() == io::ErrorKind::DirectoryNotEmpty => {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                _ => return,
            }
        }
    }
}

fn bytes(p: &Path) -> &[u8] {
    p.as_os_str().as_bytes()
}

fn dev(p: &Path) -> u64 {
    std::fs::symlink_metadata(p).unwrap().dev()
}

fn folds(p: &Path, other: &Path) -> bool {
    p.exists() && other.exists()
}

/// `/dev/shm` (tmpfs, case-sensitive, trusted): the path is its own
/// spelling and nothing under it is listed.
#[test]
fn trusted_case_sensitive_volume_lists_nothing() {
    let base = Path::new("/dev/shm");
    let s = Spelling::new(Counted::default());
    if !base.is_dir() || !s.lookup().disk.consistent(bytes(base), dev(base)) {
        eprintln!("skipped: /dev/shm is missing or not a file system whose lookups are trusted");
        return;
    }
    let Some(t) = Scratch::new(base, "shm") else {
        eprintln!("skipped: cannot make a directory in /dev/shm");
        return;
    };
    let f = t.write("Src/a.h");
    if folds(&f, &t.0.join("src/A.H")) {
        eprintln!("skipped: /dev/shm folds case here");
        return;
    }
    assert_eq!(s.on_disk(bytes(&f)).unwrap(), bytes(&f));
    assert_eq!(s.on_disk(bytes(&t.0.join("Src"))).unwrap(), bytes(&t.0.join("Src")));
    let under: Vec<_> =
        s.lookup().listed.borrow().iter().filter(|d| d.starts_with(b"/dev/shm")).cloned().collect();
    assert!(under.is_empty(), "listed: {under:?}");
    let got = s.on_disk(bytes(&t.0.join("src/a.h")));
    assert!(matches!(got, Err(SpellingError::Missing { .. })), "{got:?}");
}

/// `/tmp` (here `overlay`: listed, not folding): the path is its own
/// spelling, found in the listings; another case is missing.
#[test]
fn listed_volume_that_does_not_fold() {
    let base = Path::new("/tmp");
    let Some(t) = Scratch::new(base, "tmp") else {
        eprintln!("skipped: cannot make a directory in /tmp");
        return;
    };
    let f = t.write("Src/a.h");
    if folds(&f, &t.0.join("src/A.H")) {
        eprintln!("skipped: /tmp folds case here");
        return;
    }
    let s = Spelling::new(Counted::default());
    if s.lookup().disk.consistent(bytes(&t.0), dev(&t.0)) {
        eprintln!("skipped: /tmp is trusted here, so it is not listed");
        return;
    }
    assert_eq!(s.on_disk(bytes(&f)).unwrap(), bytes(&f));
    assert!(s.lookup().listed.borrow().iter().any(|d| d.as_slice() == bytes(&t.0)));
    let got = s.on_disk(bytes(&t.0.join("src/A.H")));
    assert!(matches!(got, Err(SpellingError::Missing { .. })), "{got:?}");
}

/// The workspace's volume (folds case and normalisation): `real_path`
/// gives the listed spelling.
#[test]
fn folding_volume_gives_the_listed_spelling() {
    let base = Path::new(env!("CARGO_TARGET_TMPDIR"));
    let Some(t) = Scratch::new(base, "fold") else {
        eprintln!("skipped: cannot make a directory in {}", base.display());
        return;
    };
    let f = t.write("Foo/bar.h");
    let asked = t.0.join("foo/BAR.h");
    if !folds(&f, &asked) {
        eprintln!("skipped: {} does not fold case", base.display());
        return;
    }
    assert_eq!(real_path(&asked).unwrap(), f);
    assert_eq!(real_path(&f).unwrap(), f);
    let nfc = t.write("caf\u{e9}.h");
    let nfd = t.0.join("cafe\u{301}.h");
    if nfd.exists() {
        assert_eq!(real_path(&nfd).unwrap(), nfc);
    } else {
        eprintln!("{}: does not fold normalisation", base.display());
    }
}

/// Review S5 (S18): `respelled` respells a path only when it is its own
/// real path; one through a link (here `lnk`, to `dir`) is kept as given,
/// another case and all, as the records that keep a path as given want.
#[test]
fn respelled_only_a_path_that_is_its_own_real_path() {
    let base = Path::new(env!("CARGO_TARGET_TMPDIR"));
    let Some(t) = Scratch::new(base, "respell") else {
        eprintln!("skipped: cannot make a directory in {}", base.display());
        return;
    };
    let f = t.write("dir/foo.h");
    std::os::unix::fs::symlink(t.0.join("dir"), t.0.join("lnk")).unwrap();
    let asked = t.0.join("dir/FOO.h");
    if !folds(&f, &asked) {
        eprintln!("skipped: {} does not fold case", base.display());
        return;
    }
    assert_eq!(respelled(bytes(&asked)), bytes(&f));
    let through = t.0.join("lnk/FOO.h");
    assert_eq!(respelled(bytes(&through)), bytes(&through));
}

/// What `Disk` says of each location: the device's types from mountinfo
/// (a wrong `major:minor` decode finds no line), the mount that holds it,
/// and whether it is trusted. `/` and `/dev/shm` are named by a mountinfo
/// line of their own here; the workspace's FUSE mount reports another
/// device than its line, so it is not trusted whatever its type.
#[test]
fn what_disk_says_of_each_volume() {
    let Ok(text) = std::fs::read_to_string("/proc/self/mountinfo") else {
        eprintln!("skipped: no /proc/self/mountinfo");
        return;
    };
    let disk = Disk::default();
    let line_type = |mount: &str| {
        text.lines()
            .filter_map(|l| {
                let f: Vec<&str> = l.split(' ').collect();
                let dash = f.iter().skip(6).position(|x| *x == "-")?;
                (f.get(4) == Some(&mount)).then(|| f[6 + dash + 1].to_owned())
            })
            .next_back()
    };
    for p in ["/", "/tmp", "/dev/shm", env!("CARGO_TARGET_TMPDIR")] {
        let path = Path::new(p);
        if !path.exists() {
            continue;
        }
        let d = dev(path);
        let types = disk.mounts().types(d);
        let holder = disk.mounts().holder(bytes(path));
        let trusted = disk.consistent(bytes(path), d);
        eprintln!("{p}: device {d}, types {types:?}, held by {holder:?}, trusted {trusted}");
        if let Some(h) = holder.filter(|h| h.dev != d) {
            assert!(!trusted, "{p}: its holder {h:?} names another device than {d}");
        }
        if let Some(t) = line_type(p).filter(|_| p == "/" || p == "/dev/shm") {
            assert!(types.contains(&t.as_str()), "{p}: its line is {t}; Disk finds {types:?}");
            assert_eq!(trusted, t == "tmpfs" || t.starts_with("ext"), "{p}");
        }
    }
    // The workspace: trusted only if the mount that holds it (the
    // longest mount point above it) is of a trusted type.
    let ws = env!("CARGO_TARGET_TMPDIR");
    let holder = text
        .lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split(' ').collect();
            let dash = f.iter().skip(6).position(|x| *x == "-")?;
            let at = *f.get(4)?;
            let under = at == "/" || ws == at || ws.starts_with(&format!("{at}/"));
            under.then(|| (at.len(), f[6 + dash + 1].to_owned()))
        })
        .max();
    if let Some((_, t)) = holder {
        let trusted = disk.consistent(ws.as_bytes(), dev(Path::new(ws)));
        assert!(!trusted || slip_spelling::CONSISTENT.contains(&t.as_str()), "{ws}: {t}");
    }
}
