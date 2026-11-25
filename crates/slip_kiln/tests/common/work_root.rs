//! Where a test makes its work directories, the trees it writes and then
//! has a front end, `slip` or `slip_apply` read (`spec/harness.md` §11.5).
//!
//! One file, included by the `tests/common` module of every crate whose
//! tests write such trees (`#[path]`), so that the rule is the same in
//! each: with `SLIP_TEST_ROOT` set and not empty, a directory of this
//! test binary and process,
//! `$SLIP_TEST_ROOT/<package>/<test binary>.<pid>.<token>`, made at the
//! first call; unset or empty, the caller's fallback (Cargo's
//! `CARGO_TARGET_TMPDIR`), as before WP-162.
//!
//! No test ends a process, so nothing removes the directory when the
//! binary exits: it holds a lock on its file `.lock` for the life of the
//! process, and the first call of every later process of the package
//! removes each sibling whose lock is free (its process has ended) and
//! older than [`STALE`]. The root so holds the directories of running and
//! of the last ended processes, never more. On a file system where a lock
//! does not exclude another (the workspace's mount: a second `flock` of a
//! held file succeeds) nothing is removed.

use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// The variable.
pub const VAR: &str = "SLIP_TEST_ROOT";

/// The file of a directory that holds its process's lock.
pub const LOCK: &str = ".lock";

/// How old an ended process's directory must be before it is removed: a
/// directory younger than this may be one whose process has made it and
/// not yet taken its lock.
pub const STALE: Duration = Duration::from_secs(10);

/// The root of this process's work directories: under `$SLIP_TEST_ROOT`
/// when it is set and not empty ([`under`]), else `fallback`.
///
/// # Panics
///
/// When the variable is not an absolute path, or its directory cannot be
/// made or locked: a test cannot run without its work directory.
pub fn work_root_or(fallback: &str) -> PathBuf {
    match std::env::var_os(VAR).filter(|v| !v.is_empty()) {
        None => PathBuf::from(fallback),
        Some(v) => mine(Path::new(&v)).clone(),
    }
}

/// `<root>/<package>/<test binary>.<pid>.<token>`.
pub fn under(root: &Path, pid: u32, token: u64) -> PathBuf {
    root.join(env!("CARGO_PKG_NAME")).join(format!("{}.{pid}.{token:x}", env!("CARGO_CRATE_NAME")))
}

/// This process's directory under `root`, made, locked and its stale
/// siblings removed at the first call.
fn mine(root: &Path) -> &'static PathBuf {
    /// The directory and its lock, held until the process ends.
    static MINE: OnceLock<(PathBuf, File)> = OnceLock::new();
    &MINE
        .get_or_init(|| {
            assert!(root.is_absolute(), "{VAR}={}: not an absolute path", root.display());
            let made = make(root).unwrap_or_else(|e| panic!("{VAR}={}: {e}", root.display()));
            let parent = made.0.parent().expect("the package's directory").to_owned();
            sweep(&parent, &made.0, STALE);
            made
        })
        .0
}

/// Makes a new directory [`under`] `root` and takes its lock.
fn make(root: &Path) -> io::Result<(PathBuf, File)> {
    let pid = std::process::id();
    let mut token = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| {
        d.as_secs().wrapping_mul(1_000_000_000).wrapping_add(u64::from(d.subsec_nanos()))
    });
    let dir = loop {
        let dir = under(root, pid, token);
        fs::create_dir_all(dir.parent().expect("the package's directory"))?;
        match fs::create_dir(&dir) {
            Ok(()) => break dir,
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => token = token.wrapping_add(1),
            Err(e) => return Err(e),
        }
    };
    let lock = File::create(dir.join(LOCK))?;
    lock.lock()?;
    Ok((dir, lock))
}

/// Removes every directory of `parent` but `keep` that has a [`LOCK`]
/// file, free and older than `stale`; first checks, through a second
/// handle on `keep`'s held lock, that a lock excludes another here, and
/// removes nothing if it does not. Returns the directories removed.
pub fn sweep(parent: &Path, keep: &Path, stale: Duration) -> Vec<PathBuf> {
    let mut removed = Vec::new();
    let Ok(probe) = File::open(keep.join(LOCK)) else { return removed };
    if probe.try_lock().is_ok() {
        // The file system does not exclude: nothing tells an ended
        // process from a running one.
        return removed;
    }
    drop(probe);
    let Ok(entries) = fs::read_dir(parent) else { return removed };
    for e in entries.flatten() {
        let dir = e.path();
        if dir == keep || !e.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        let Ok(lock) = File::open(dir.join(LOCK)) else { continue };
        let old = lock
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age >= stale);
        if old && lock.try_lock().is_ok() && fs::remove_dir_all(&dir).is_ok() {
            removed.push(dir);
        }
    }
    removed
}

/// The rule's test, run by one `#[test]` of each crate that includes this
/// file, with that crate's fallback.
pub fn self_test(fallback: &str) {
    let crate_dir = |root: &str| {
        Path::new(root)
            .join(env!("CARGO_PKG_NAME"))
            .join(format!("{}.42.ff", env!("CARGO_CRATE_NAME")))
    };
    assert_eq!(under(Path::new("/r"), 42, 255), crate_dir("/r"));
    let got = work_root_or(fallback);
    match std::env::var_os(VAR).filter(|v| !v.is_empty()) {
        None => assert_eq!(got, Path::new(fallback)),
        Some(v) => {
            let base = Path::new(&v).join(env!("CARGO_PKG_NAME"));
            assert_eq!(got.parent(), Some(base.as_path()), "{}", got.display());
            let name = got.file_name().unwrap().to_string_lossy().into_owned();
            let pid = format!("{}.{}.", env!("CARGO_CRATE_NAME"), std::process::id());
            assert!(name.starts_with(&pid), "{name}");
            assert!(got.join(LOCK).is_file());
            assert!(!got.starts_with(fallback), "{} under the fallback", got.display());
            assert_eq!(work_root_or(fallback), got, "one directory per process");
        }
    }
    // The sweep, in a directory of its own under the root: an ended
    // process's directory (its lock free) goes, a running one's (held)
    // and one with no lock stay. Where a lock does not exclude, all stay.
    let at = got.join(format!("work-root-sweep.{}", std::process::id()));
    let _ = fs::remove_dir_all(&at);
    let (keep, ended, running, other) =
        (at.join("keep"), at.join("ended"), at.join("running"), at.join("other"));
    for d in [&keep, &ended, &running, &other] {
        fs::create_dir_all(d).unwrap();
    }
    let held = File::create(keep.join(LOCK)).unwrap();
    held.lock().unwrap();
    File::create(ended.join(LOCK)).unwrap();
    let busy = File::create(running.join(LOCK)).unwrap();
    busy.lock().unwrap();
    let excludes = File::open(keep.join(LOCK)).unwrap().try_lock().is_err();
    assert_eq!(sweep(&at, &keep, Duration::from_secs(3600)), Vec::<PathBuf>::new(), "too young");
    let removed = sweep(&at, &keep, Duration::ZERO);
    if excludes {
        assert_eq!(removed, [ended.clone()]);
    } else {
        eprintln!("{}: a lock does not exclude another here: nothing swept", at.display());
        assert!(removed.is_empty());
    }
    assert!(keep.is_dir() && running.is_dir() && other.is_dir());
    drop((held, busy));
    fs::remove_dir_all(&at).unwrap();
}
