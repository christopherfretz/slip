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
//!
//! [`folding_root_or`] is the other root, for the tests that are ABOUT
//! spelling on a volume that folds case (`spec/edits.md` §2.1 "One
//! spelling"; `docs/wp/WP-151.md`): `$SLIP_TEST_ROOT` is the container's
//! own disk, which does not fold, and a test there skips. Its directory is
//! under the caller's `CARGO_TARGET_TMPDIR`, the target directory, which
//! on this project's host is on the workspace's mount (it folds). The
//! mount's two hazards apply there (`spec/landed/harness/WP-159.md`,
//! `WP-162.md`): a hard link can read another name's content, and a fresh
//! file can change its inode number in bursts. So a test takes it only
//! when it needs a volume that folds, and makes no hard link in it.
//! [`folding_guard`] fails a run in which the source tree's volume folds
//! and this root's does not: the folding tests would all skip unseen.

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

/// The directory, under the target directory, of [`folding_root_or`].
pub const FOLDING: &str = "folding";

/// The root of this process's work directories for a test about spelling
/// on a volume that folds (the module's documentation):
/// `<target_tmp>/folding/<package>/<test binary>.<pid>.<token>`, made at
/// the first call. With `$SLIP_TEST_ROOT` set its name is the one of the
/// process's [`work_root_or`] directory, made first, whose lock (on the
/// container's disk, where a lock excludes) says whether the process
/// still runs: the first call of every later process removes each folding
/// sibling whose twin is gone or free and that is older than [`STALE`]
/// ([`sweep_folding`]). Without it the directory holds its own lock and
/// the sweep is [`sweep`]'s (on the mount: nothing).
///
/// # Panics
///
/// When the directory cannot be made or locked.
pub fn folding_root_or(target_tmp: &str) -> PathBuf {
    /// The directory and, without a twin, its lock.
    static MINE: OnceLock<(PathBuf, Option<File>)> = OnceLock::new();
    MINE.get_or_init(|| {
        let root = Path::new(target_tmp).join(FOLDING);
        if std::env::var_os(VAR).is_some_and(|v| !v.is_empty()) {
            let twin = work_root_or(target_tmp);
            let name = twin.file_name().expect("a work directory's name");
            let dir = root.join(env!("CARGO_PKG_NAME")).join(name);
            fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
            let parent = twin.parent().expect("the package's directory").to_owned();
            sweep_folding(
                dir.parent().expect("the folding package directory"),
                &dir,
                &parent,
                STALE,
            );
            (dir, None)
        } else {
            let (dir, lock) = make(&root).unwrap_or_else(|e| panic!("{}: {e}", root.display()));
            let parent = dir.parent().expect("the package's directory").to_owned();
            sweep(&parent, &dir, STALE);
            (dir, Some(lock))
        }
    })
    .0
    .clone()
}

/// Removes every directory of `parent` (the folding directories of a
/// package) but `keep` whose twin of the same name in `twins` (the
/// package's [`work_root_or`] directories) is gone or holds a free
/// [`LOCK`], and that is older than `stale`. Removes nothing unless a lock
/// excludes another in `twins` (checked through `keep`'s twin, held by
/// this process). Returns the directories removed.
pub fn sweep_folding(parent: &Path, keep: &Path, twins: &Path, stale: Duration) -> Vec<PathBuf> {
    let mut removed = Vec::new();
    let Some(name) = keep.file_name() else { return removed };
    let Ok(probe) = File::open(twins.join(name).join(LOCK)) else { return removed };
    if probe.try_lock().is_ok() {
        return removed;
    }
    drop(probe);
    let Ok(entries) = fs::read_dir(parent) else { return removed };
    for e in entries.flatten() {
        let dir = e.path();
        if dir == keep || !e.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        let old = e
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age >= stale);
        if !old {
            continue;
        }
        let ended = match File::open(twins.join(e.file_name()).join(LOCK)) {
            Ok(lock) => lock.try_lock().is_ok(),
            Err(err) => err.kind() == io::ErrorKind::NotFound,
        };
        if ended && fs::remove_dir_all(&dir).is_ok() {
            removed.push(dir);
        }
    }
    removed
}

/// Whether the directory `dir` folds case: a file made there is found by
/// its name in another case, as the same file. The probe file is removed.
///
/// # Panics
///
/// When the probe cannot be made.
pub fn folds(dir: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    let name = format!("fold-probe.{}.{:?}", std::process::id(), std::thread::current().id())
        .replace(|c: char| !c.is_ascii_alphanumeric() && c != '.' && c != '-', "");
    let probe = dir.join(&name);
    fs::write(&probe, b"").unwrap_or_else(|e| panic!("{}: {e}", probe.display()));
    let made = fs::metadata(&probe).expect("the probe");
    let other = fs::metadata(dir.join(name.to_ascii_uppercase()));
    let _ = fs::remove_file(&probe);
    other.is_ok_and(|m| m.dev() == made.dev() && m.ino() == made.ino())
}

/// Whether the volume of the source tree folds case: its own manifest
/// found by the name in upper case, as the same file. Read only.
pub fn source_folds() -> bool {
    use std::os::unix::fs::MetadataExt;
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let (Ok(a), Ok(b)) =
        (fs::metadata(dir.join("Cargo.toml")), fs::metadata(dir.join("CARGO.TOML")))
    else {
        return false;
    };
    a.dev() == b.dev() && a.ino() == b.ino()
}

/// The one way a test that needs a volume that folds gets it: the folding
/// root ([`folding_root_or`]) and whether it folds (probed once per
/// process). PANICS when it does not fold and the source tree's volume
/// does ([`source_folds`]: this project's host, where the gate runs), so
/// that such a test cannot skip there. Elsewhere, when no volume folds,
/// prints one line `<test>: skipped: <why>` and the test leaves out what
/// needs folding (or all of it: [`folding_volume_or`]).
///
/// # Panics
///
/// As said, and when the folding root cannot be made.
pub fn folding_checked_or(target_tmp: &str, test: &str) -> (PathBuf, bool) {
    static FOLDS: OnceLock<bool> = OnceLock::new();
    let root = folding_root_or(target_tmp);
    let f = *FOLDS.get_or_init(|| folds(&root));
    if !f {
        assert!(
            !source_folds(),
            "{test}: the folding root {} does not fold, but the source tree's volume does (the \
             gate's host): a test about a volume that folds must run here, not skip",
            root.display()
        );
        eprintln!(
            "{test}: skipped: no volume that folds here (the folding root {} and the source tree \
             are case-sensitive)",
            root.display()
        );
    }
    (root, f)
}

/// [`folding_checked_or`]'s root when it folds; `None` (the line said)
/// when no volume folds here.
pub fn folding_volume_or(target_tmp: &str, test: &str) -> Option<PathBuf> {
    let (root, f) = folding_checked_or(target_tmp, test);
    f.then_some(root)
}

/// The guard of the folding tests, run by one `#[test]` of each crate that
/// has them: says which kind of volume [`work_root_or`] and
/// [`folding_root_or`] are on, and FAILS when the source tree's volume
/// folds (this project's host, where the gate runs) and the folding root's
/// does not, so that "every folding test skipped" cannot pass unseen.
/// Also the folding root's own rule: under `target_tmp`, one directory per
/// process, named as the work directory when `$SLIP_TEST_ROOT` is set.
pub fn folding_guard(target_tmp: &str) {
    let kind = |f: bool| if f { "folds" } else { "does not fold" };
    let work = work_root_or(target_tmp);
    let folding = folding_root_or(target_tmp);
    let (w, f, s) = (folds(&work), folds(&folding), source_folds());
    eprintln!("work root {}: {}", work.display(), kind(w));
    eprintln!("folding root {}: {}", folding.display(), kind(f));
    eprintln!("source tree {}: {}", env!("CARGO_MANIFEST_DIR"), kind(s));
    assert!(
        f || !s,
        "the source tree's volume folds and the folding root's does not: every test about a \
         volume that folds would skip (folding root {})",
        folding.display()
    );
    let base = Path::new(target_tmp).join(FOLDING).join(env!("CARGO_PKG_NAME"));
    assert_eq!(folding.parent(), Some(base.as_path()), "{}", folding.display());
    assert_eq!(folding_root_or(target_tmp), folding, "one directory per process");
    if std::env::var_os(VAR).is_some_and(|v| !v.is_empty()) {
        assert_eq!(folding.file_name(), work.file_name(), "named as the work directory");
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
    assert_eq!(sweep(&at, &keep, Duration::from_hours(1)), Vec::<PathBuf>::new(), "too young");
    let removed = sweep(&at, &keep, Duration::ZERO);
    if excludes {
        assert_eq!(removed, std::slice::from_ref(&ended));
    } else {
        eprintln!("{}: a lock does not exclude another here: nothing swept", at.display());
        assert!(removed.is_empty());
    }
    assert!(keep.is_dir() && running.is_dir() && other.is_dir());
    drop((held, busy));
    fs::remove_dir_all(&at).unwrap();
}
