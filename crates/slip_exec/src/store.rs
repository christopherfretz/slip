//! The store (`spec/exec.md` §2). Work package WP-008.
//!
//! A directory holding plans, shards, run records and worker logs:
//!
//! ```text
//! <store>/
//!   VERSION                                   the text "1\n"
//!   lock                                      "<pid>\n" while a phase runs (exclusive open)
//!   plans/<plan id>.plan
//!   declared/<declared id>.decl               DECL containers (spec/verify.md §2)
//!   shards/<plan id>/<phase>.<round>/<observer id>.shard
//!   runs/<plan id>/<phase>.<round>.<started_unix_ns>.run
//!   logs/<plan id>/<phase>.<round>/<observer id>.<attempt>.log
//!   tmp/<pid>.<t>.<n>.tmp                     files being written by process <pid>
//!   tmp/<pid>.<t>.<n>.claim                   process <pid> taking the lock
//!   tmp/<pid>.d/                              scratch directory of process <pid>
//! ```
//!
//! A transform's run (M2, `spec/edits.md` §10.4) adds `transforms/`,
//! `edits/`, `overlays/`, `results/` ([`Object`]), `blobs/`, the transform
//! run records `runs/<plan id>/transform.<round>.<started_unix_ns>.run` and
//! `write.journal`. Their directories are made by the first publish into
//! each, so a store initialised before them is a valid store. Objects and
//! blobs are named by their content: a file is left untouched when it
//! already holds that content, and replaced otherwise. An exclusive [`Store::open`] of a store that
//! holds `write.journal` is refused ([`StoreError::UnfinishedWrite`]);
//! [`Store::open_for_recovery`] is the one opener that accepts it.
//!
//! Every file is published by writing it under `tmp/` and renaming it to its
//! final name, so a reader never sees a partial file; the lock and the
//! journal alone are created in place (`create_new`), and the store makes
//! no hard link. An exclusive [`Store`] holds `lock`, taken through claims
//! (§2.1 "The lock"); a shared one only reads and publishes. A `Store` removes
//! its own `tmp/` entries and, if it holds it, the lock, when dropped, and
//! [`cleanup_on_signal`] does the same from the signal path (§7).
//! [`Store::remove_entries_of`] removes the entries of a dead child (the
//! pool's workers). The rules in detail are `spec/exec.md` §2.1, the
//! interface §2.2.

use std::collections::HashSet;
use std::fmt;
use std::fs;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use slip_model::container::{Reader, TRAILER_LEN};
use slip_model::declared::Declared;
use slip_model::hash::content_digest;
use slip_model::{
    Digest, EditSet, Overlay, Phase, Plan, RunRecord, Shard, Transform, TransformResult, formats,
};

/// The text of `VERSION`.
pub const VERSION_TEXT: &str = "1\n";

/// How a store is opened (§2 "Locking").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    /// Holds `<store>/lock` for the life of the [`Store`]; cleans stale
    /// `tmp/` entries when opened. For a command that runs a phase.
    Exclusive,
    /// No lock, no cleaning: reads and publishes only.
    Shared,
}

/// Why a store operation failed. All of these are `Input` errors of the
/// command (`spec/exec.md` §8.5, status 3).
#[derive(Debug)]
pub enum StoreError {
    /// The directory is not empty and has no `VERSION` (or is not a
    /// directory): Slip does not write into it.
    Unrecognised(PathBuf),
    /// `VERSION` holds something other than `"1\n"` (shown lossily).
    Version(String),
    /// The lock is held by a live process.
    Busy { pid: u32, lock: PathBuf },
    /// Bytes given to a `publish_*` method are not a valid file of the
    /// expected kind, or do not belong at the place named.
    Invalid(String),
    /// A file system operation failed.
    Io { path: PathBuf, error: io::Error },
    /// An exclusive open found `write.journal` (this path): the store has
    /// an unfinished write of the working tree (`spec/edits.md` §7.3).
    UnfinishedWrite(PathBuf),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::Unrecognised(p) => write!(
                f,
                "{}: not a Slip store (no VERSION) and not empty; refusing to write into it",
                p.display()
            ),
            StoreError::Version(v) => {
                write!(f, "unsupported store version {v:?} (this Slip reads version 1)")
            }
            StoreError::Busy { pid, lock } => {
                write!(f, "the store is busy: process {pid} holds {}", lock.display())
            }
            StoreError::Invalid(what) => write!(f, "refusing to publish: {what}"),
            StoreError::Io { path, error } => write!(f, "{}: {error}", path.display()),
            StoreError::UnfinishedWrite(journal) => write!(
                f,
                "the store has an unfinished write of the working tree ({}); \
                 run `slip apply --recover`",
                journal.display()
            ),
        }
    }
}

impl std::error::Error for StoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            StoreError::Io { error, .. } => Some(error),
            _ => None,
        }
    }
}

type Result<T> = std::result::Result<T, StoreError>;

fn io_err(path: &Path) -> impl FnOnce(io::Error) -> StoreError + '_ {
    move |error| StoreError::Io { path: path.to_path_buf(), error }
}

/// `<phase>` in a path: `map` or `verify`.
pub fn phase_name(phase: Phase) -> &'static str {
    match phase {
        Phase::Map => "map",
        Phase::Verify => "verify",
    }
}

/// Process-wide counter for the names of `tmp/` files.
static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

/// The name of this process's `n`-th `tmp/` file, `<pid>.<t>.<n>.tmp`:
/// `<t>` is [`process_token`], so that no later process with the same
/// pid makes the name again (§2.1 "`tmp/` names"; on a file system that
/// confuses the names of a hard-linked file, a name made again while
/// another name of its old file is alive can make that name read as the
/// new file, `spec/harness.md` §11).
fn tmp_name(n: u64) -> String {
    format!("{}.{}.{n}.tmp", std::process::id(), process_token())
}

/// This process's token: the time of its first call, in nanoseconds since
/// the Unix epoch (0 if the clock is before it). Two processes with one
/// pid are never alive at once, and the later one's first call comes
/// after the earlier one's.
pub fn process_token() -> u64 {
    static TOKEN: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    *TOKEN.get_or_init(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| u64::try_from(d.as_nanos()).unwrap_or(u64::MAX))
    })
}

/// The name of this process's `n`-th claim (§2.1 "The lock"),
/// `<pid>.<t>.<n>.claim`: never made again, as a temporary's name.
fn claim_name(n: u64) -> String {
    format!("{}.{}.{n}.claim", std::process::id(), process_token())
}

/// The locks this process holds now, by [`lock_key`], under the mutex that
/// makes this process's takes one at a time (§2.1 "The lock", steps 2
/// and 3).
static HELD: Mutex<Option<HashSet<PathBuf>>> = Mutex::new(None);

/// What [`HELD`] knows a store's lock by: the lock of the store's
/// canonical path (two spellings of one store are one lock), or of the
/// path given if it has none.
fn lock_key(root: &Path) -> PathBuf {
    fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf()).join("lock")
}

/// The lock a store holds.
#[derive(Debug)]
struct Held {
    path: PathBuf,
    key: PathBuf,
    /// The close ran: the lock is released (or left) and unregistered.
    released: AtomicBool,
}

/// What a `Store` and its signal cleanup share.
#[derive(Debug)]
struct Inner {
    root: PathBuf,
    /// The lock, when this store holds it.
    lock: Option<Held>,
    /// This store's `tmp/` files that exist now, and whether it is closed
    /// (cleaned up: no more files may be created).
    tmp: Mutex<(HashSet<PathBuf>, bool)>,
    /// [`Store::scratch_dir`] was called on this store.
    scratch: AtomicBool,
}

impl Inner {
    fn tmp_dir(&self) -> PathBuf {
        self.root.join("tmp")
    }

    fn scratch_path(&self) -> PathBuf {
        self.tmp_dir().join(format!("{}.d", std::process::id()))
    }

    /// Removes this store's `tmp/` files, its scratch directory and the
    /// lock if held. Idempotent; never panics; errors are ignored (best
    /// effort, on exit paths).
    fn clean(&self) {
        let files = {
            let mut g = self.tmp.lock().unwrap_or_else(PoisonError::into_inner);
            g.1 = true;
            std::mem::take(&mut g.0)
        };
        for f in files {
            let _ = fs::remove_file(f);
        }
        if self.scratch.swap(false, Ordering::SeqCst) {
            let _ = fs::remove_dir_all(self.scratch_path());
        }
        if let Some(held) = &self.lock {
            // Under the mutex of the takes: no take of this process sees
            // the lock removed and still registered, or registered again
            // and then removed by this close.
            let mut registry = HELD.lock().unwrap_or_else(PoisonError::into_inner);
            if !held.released.swap(true, Ordering::SeqCst) {
                release(&RealFs, &held.path, std::process::id());
                if let Some(set) = registry.as_mut() {
                    set.remove(&held.key);
                }
            }
        }
    }

    /// Writes `bytes` to a new `tmp/` file of this store and returns its
    /// path; the file is registered so that it is removed on every exit path.
    fn write_tmp(&self, bytes: &[u8]) -> Result<PathBuf> {
        let dir = self.tmp_dir();
        let n = TMP_SEQ.fetch_add(1, Ordering::SeqCst);
        let path = dir.join(tmp_name(n));
        {
            // Created under the lock, so a concurrent `clean` either sees
            // the file or makes us refuse to create it.
            let mut g = self.tmp.lock().unwrap_or_else(PoisonError::into_inner);
            if g.1 {
                return Err(StoreError::Io {
                    path,
                    error: io::Error::new(io::ErrorKind::Interrupted, "the store is closed"),
                });
            }
            let created = fs::OpenOptions::new().write(true).create_new(true).open(&path);
            let file = match created {
                Ok(f) => f,
                Err(e) if e.kind() == io::ErrorKind::NotFound => {
                    fs::create_dir_all(&dir).map_err(io_err(&dir))?;
                    fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&path)
                        .map_err(io_err(&path))?
                }
                Err(e) => return Err(StoreError::Io { path, error: e }),
            };
            g.0.insert(path.clone());
            drop(g);
            let mut file = file;
            if let Err(e) = file.write_all(bytes) {
                self.forget_tmp(&path, true);
                return Err(StoreError::Io { path, error: e });
            }
        }
        Ok(path)
    }

    /// Drops `path` from the registered files, removing it if `remove`.
    fn forget_tmp(&self, path: &Path, remove: bool) {
        let mut g = self.tmp.lock().unwrap_or_else(PoisonError::into_inner);
        g.0.remove(path);
        if remove {
            let _ = fs::remove_file(path);
        }
    }

    /// Publishes `bytes` at `dest` (§2 "Publishing").
    fn publish(&self, dest: &Path, bytes: &[u8]) -> Result<()> {
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(io_err(parent))?;
        }
        let tmp = self.write_tmp(bytes)?;
        match fs::rename(&tmp, dest) {
            Ok(()) => {
                self.forget_tmp(&tmp, false);
                Ok(())
            }
            Err(e) => {
                self.forget_tmp(&tmp, true);
                Err(StoreError::Io { path: dest.to_path_buf(), error: e })
            }
        }
    }
}

/// The contents of a lock held by process `pid`.
fn lock_text(pid: u32) -> String {
    format!("{pid}\n")
}

// -- the lock (§2.1 "The lock") ----------------------------------------------

/// How many rounds a take tries before it is `Busy` (§2.1 "The lock").
const ROUNDS: u32 = 8;

/// What a creation of a name that must not exist did.
#[derive(Debug)]
enum Made {
    /// Nothing was created (`AlreadyExists` among others).
    Not(io::Error),
    /// The name was created, then writing or syncing it failed.
    Partly(io::Error),
}

/// The operations of the lock on names, so that its tests can run it
/// over a file system in memory: [`RealFs`] is the store's.
trait LockFs {
    /// Creates `path`, which must not exist, empty.
    fn create(&self, path: &Path) -> io::Result<()>;
    /// Creates `path`, which must not exist, then writes `bytes` to it and
    /// syncs it.
    fn create_written(&self, path: &Path, bytes: &[u8]) -> std::result::Result<(), Made>;
    /// The bytes of `path`; `None` when there is no such file.
    fn read(&self, path: &Path) -> io::Result<Option<Vec<u8>>>;
    /// Removes the file `path` (`NotFound` if there is none).
    fn remove(&self, path: &Path) -> io::Result<()>;
    /// The UTF-8 names in directory `dir`; none if it does not exist.
    fn list(&self, dir: &Path) -> io::Result<Vec<String>>;
    /// Whether process `pid` exists ([`alive`]).
    fn alive(&self, pid: u32) -> bool;
    /// Waits before round `round` + 1 after meeting a contender.
    fn pause(&self, round: u32);
}

/// The file system.
struct RealFs;

impl LockFs for RealFs {
    fn create(&self, path: &Path) -> io::Result<()> {
        fs::OpenOptions::new().write(true).create_new(true).open(path).map(drop)
    }

    fn create_written(&self, path: &Path, bytes: &[u8]) -> std::result::Result<(), Made> {
        let mut f =
            fs::OpenOptions::new().write(true).create_new(true).open(path).map_err(Made::Not)?;
        f.write_all(bytes).and_then(|()| f.sync_all()).map_err(Made::Partly)
    }

    fn read(&self, path: &Path) -> io::Result<Option<Vec<u8>>> {
        match fs::read(path) {
            Ok(b) => Ok(Some(b)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn remove(&self, path: &Path) -> io::Result<()> {
        fs::remove_file(path)
    }

    fn list(&self, dir: &Path) -> io::Result<Vec<String>> {
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e),
        };
        let mut out = Vec::new();
        for entry in entries {
            if let Ok(name) = entry?.file_name().into_string() {
                out.push(name);
            }
        }
        Ok(out)
    }

    fn alive(&self, pid: u32) -> bool {
        alive(pid)
    }

    /// 1 ms and a random part below 4 ms, the part's bound doubled each
    /// round up to the sixth: at most about 0.5 s over the 8 rounds. Random,
    /// not a function of the pid and the round: two openers whose waits are
    /// alike meet again every round (a soak of four openers, two of them
    /// with equal pids modulo 5, slowed to a crawl so).
    fn pause(&self, round: u32) {
        use std::hash::{BuildHasher as _, Hasher as _};
        // `RandomState` draws its keys at random per process and changes
        // them with every `new`.
        let mut h = std::hash::RandomState::new().build_hasher();
        h.write_u32(round);
        h.write_u128(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos()),
        );
        let span_us = 4_000u64 << round.min(5);
        std::thread::sleep(std::time::Duration::from_micros(1_000 + h.finish() % span_us));
    }
}

/// What a lock's bytes say (§2.1 "The lock", step 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LockText {
    /// `<pid>\n`, the pid canonical decimal.
    Whole(u32),
    /// Empty, or one to ten decimal digits without the newline: what a
    /// creator leaves between the creation and the write.
    Partial,
    /// Anything else: not a lock Slip made.
    Other,
}

fn lock_state(text: &[u8]) -> LockText {
    if let Some(line) = text.strip_suffix(b"\n") {
        return std::str::from_utf8(line)
            .ok()
            .and_then(decimal)
            .and_then(|p| u32::try_from(p).ok())
            .map_or(LockText::Other, LockText::Whole);
    }
    if text.len() <= 10 && text.iter().all(u8::is_ascii_digit) {
        LockText::Partial
    } else {
        LockText::Other
    }
}

/// What a round inside ended with.
enum Inside {
    /// Another live process's claim (its pid), or a lock made by an opener
    /// that does not follow §2.1 (none): the next round.
    Again(Option<u32>),
    /// The lock is taken, or the take failed.
    Done(Result<()>),
}

/// §2.1 "The lock": takes `lock` for process `me`, its claims made in
/// `tmp` under the names `next` gives. `held_here`: this process holds
/// `lock` now (the caller's registry, under the mutex of the takes).
fn take(
    fs: &dyn LockFs,
    lock: &Path,
    tmp: &Path,
    me: u32,
    held_here: bool,
    next: &mut dyn FnMut() -> String,
) -> Result<()> {
    let mut contender = None;
    for round in 0..ROUNDS {
        // 1. Claim.
        let claim = tmp.join(next());
        fs.create(&claim).map_err(io_err(&claim))?;
        let inside = inside(fs, lock, tmp, me, held_here);
        // 5. Leave (and every other way out of the round).
        let left = fs.remove(&claim);
        match inside {
            Inside::Done(Ok(())) => {
                if let Err(e) = left {
                    // A claim of this live process left behind would make
                    // every other opener wait until it ends: give the lock
                    // back rather than hold it so.
                    let _ = fs.remove(lock);
                    return Err(StoreError::Io { path: claim, error: e });
                }
                return Ok(());
            }
            Inside::Done(Err(e)) => return Err(e),
            Inside::Again(c) => {
                contender = c.or(contender);
                if round + 1 < ROUNDS {
                    fs.pause(round);
                }
            }
        }
    }
    // The lock's live owner if it reads one, else the last contender.
    let owner = match fs.read(lock) {
        Ok(Some(text)) => match lock_state(&text) {
            LockText::Whole(q) if fs.alive(q) => Some(q),
            _ => None,
        },
        _ => None,
    };
    Err(StoreError::Busy { pid: owner.or(contender).unwrap_or(0), lock: lock.to_path_buf() })
}

/// Steps 2 to 4 of a round, `me`'s claim made.
fn inside(fs: &dyn LockFs, lock: &Path, tmp: &Path, me: u32, held_here: bool) -> Inside {
    // 2. Look for others.
    let names = match fs.list(tmp) {
        Ok(n) => n,
        Err(e) => return Inside::Done(Err(StoreError::Io { path: tmp.to_path_buf(), error: e })),
    };
    for name in &names {
        let Some(pid) = name.strip_suffix(".claim").and_then(pid_prefix) else { continue };
        if pid != me && fs.alive(pid) {
            return Inside::Again(Some(pid));
        }
    }
    // 3. Read the lock: only one live process at a time gets here.
    let busy = |pid| Inside::Done(Err(StoreError::Busy { pid, lock: lock.to_path_buf() }));
    match fs.read(lock) {
        Ok(None) => {}
        Ok(Some(text)) => match lock_state(&text) {
            LockText::Whole(q) if q == me && held_here => return busy(q),
            LockText::Whole(q) if q != me && fs.alive(q) => return busy(q),
            // A dead owner's, this process's own left behind, or one whose
            // creator died before its write: no process holds it.
            LockText::Whole(_) | LockText::Partial => match fs.remove(lock) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => {
                    return Inside::Done(Err(StoreError::Io {
                        path: lock.to_path_buf(),
                        error: e,
                    }));
                }
            },
            LockText::Other => return Inside::Done(Err(not_a_lock(lock))),
        },
        Err(e) => return Inside::Done(Err(StoreError::Io { path: lock.to_path_buf(), error: e })),
    }
    // 4. Create.
    match fs.create_written(lock, lock_text(me).as_bytes()) {
        Ok(()) => Inside::Done(Ok(())),
        Err(Made::Not(e)) if e.kind() == io::ErrorKind::AlreadyExists => Inside::Again(None),
        Err(Made::Not(e)) => {
            Inside::Done(Err(StoreError::Io { path: lock.to_path_buf(), error: e }))
        }
        Err(Made::Partly(e)) => {
            let _ = fs.remove(lock);
            Inside::Done(Err(StoreError::Io { path: lock.to_path_buf(), error: e }))
        }
    }
}

/// The close's part of the lock (§2.1 "Closing"): removes `lock` if it
/// holds `me`'s line. Returns whether it did.
fn release(fs: &dyn LockFs, lock: &Path, me: u32) -> bool {
    if fs.read(lock).ok().flatten().as_deref() == Some(lock_text(me).as_bytes()) {
        return fs.remove(lock).is_ok();
    }
    false
}

fn not_a_lock(lock: &Path) -> StoreError {
    StoreError::Io {
        path: lock.to_path_buf(),
        error: io::Error::new(
            io::ErrorKind::InvalidData,
            "the lock file does not hold a process id; remove it if no Slip runs here",
        ),
    }
}

/// Whether process `pid` exists (§2 "Locking"): `/proc/<pid>` exists, or
/// there is no `/proc`.
fn alive(pid: u32) -> bool {
    if !Path::new("/proc/self").exists() {
        return true;
    }
    Path::new("/proc").join(pid.to_string()).exists()
}

/// A canonical decimal number: digits, no leading zero (except "0").
fn decimal(text: &str) -> Option<u64> {
    let v: u64 = text.parse().ok()?;
    (v.to_string() == text).then_some(v)
}

/// The process id a `tmp/` entry name or lock file starts with.
fn pid_prefix(name: &str) -> Option<u32> {
    let (pid, _) = name.split_once('.')?;
    let pid = decimal(pid)?;
    u32::try_from(pid).ok()
}

/// An open store. Dropping it removes the `tmp/` files it made, its scratch
/// directory, and the lock if it holds it.
#[derive(Debug)]
pub struct Store {
    inner: Arc<Inner>,
}

impl Drop for Store {
    fn drop(&mut self) {
        self.inner.clean();
    }
}

impl Store {
    /// Opens (initialising if absent or empty) the store at `dir` (§2
    /// "Opening", "Locking", "Cleaning"). The root is made absolute against
    /// the current directory.
    ///
    /// An exclusive open of a store that holds `write.journal` is
    /// [`StoreError::UnfinishedWrite`] (after the lock was taken, before
    /// stale entries are cleaned; the lock is released). A shared open is
    /// not refused.
    pub fn open(dir: &Path, access: Access) -> Result<Store> {
        Store::open_inner(dir, access, false)
    }

    /// Opens the store at `dir` exclusively, as [`Store::open`], but
    /// accepting a store that holds `write.journal`: the one opener for
    /// `slip apply --recover` (`spec/edits.md` §7.3).
    pub fn open_for_recovery(dir: &Path) -> Result<Store> {
        Store::open_inner(dir, Access::Exclusive, true)
    }

    fn open_inner(dir: &Path, access: Access, recovery: bool) -> Result<Store> {
        let root = std::path::absolute(dir).map_err(io_err(dir))?;
        let version = root.join("VERSION");
        match fs::read(&version) {
            Ok(text) => {
                if text != VERSION_TEXT.as_bytes() {
                    return Err(StoreError::Version(String::from_utf8_lossy(&text).into_owned()));
                }
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => initialise(&root)?,
            Err(e) if e.kind() == io::ErrorKind::NotADirectory => {
                return Err(StoreError::Unrecognised(root));
            }
            Err(e) => return Err(StoreError::Io { path: version, error: e }),
        }
        let mut store = Store {
            inner: Arc::new(Inner {
                root,
                lock: None,
                tmp: Mutex::new((HashSet::new(), false)),
                scratch: AtomicBool::new(false),
            }),
        };
        if access == Access::Exclusive {
            let lock = store.take_lock()?;
            // The store is not shared yet: set the lock in place.
            let Some(inner) = Arc::get_mut(&mut store.inner) else {
                unreachable!("a store being opened has no other reference")
            };
            inner.lock = Some(lock);
            if !recovery {
                let journal = store.journal_path();
                match fs::symlink_metadata(&journal) {
                    Ok(_) => return Err(StoreError::UnfinishedWrite(journal)),
                    Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                    Err(e) => return Err(StoreError::Io { path: journal, error: e }),
                }
            }
            store.clean_stale()?;
        }
        Ok(store)
    }

    /// The store's directory, absolute.
    pub fn root(&self) -> &Path {
        &self.inner.root
    }

    /// Takes `<root>/lock` for this process (§2.1 "The lock"), or fails
    /// with `Busy`, and registers it as held.
    fn take_lock(&self) -> Result<Held> {
        let lock = self.root().join("lock");
        let key = lock_key(self.root());
        let tmp = self.inner.tmp_dir();
        // The mutex of the takes: this process's takes one at a time.
        let mut registry = HELD.lock().unwrap_or_else(PoisonError::into_inner);
        let set = registry.get_or_insert_with(HashSet::new);
        fs::create_dir_all(&tmp).map_err(io_err(&tmp))?;
        let me = std::process::id();
        let mut next = || claim_name(TMP_SEQ.fetch_add(1, Ordering::SeqCst));
        take(&RealFs, &lock, &tmp, me, set.contains(&key), &mut next)?;
        set.insert(key.clone());
        Ok(Held { path: lock, key, released: AtomicBool::new(false) })
    }

    /// Removes `tmp/` entries whose process is gone (§2 "Cleaning").
    fn clean_stale(&self) -> Result<()> {
        let dir = self.inner.tmp_dir();
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(StoreError::Io { path: dir, error: e }),
        };
        for entry in entries {
            let entry = entry.map_err(io_err(&dir))?;
            let name = entry.file_name();
            let Some(pid) = name.to_str().and_then(pid_prefix) else { continue };
            if alive(pid) {
                continue;
            }
            let path = entry.path();
            let is_dir = entry.file_type().map_err(io_err(&path))?.is_dir();
            let removed = if is_dir { fs::remove_dir_all(&path) } else { fs::remove_file(&path) };
            match removed {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(StoreError::Io { path, error: e }),
            }
        }
        Ok(())
    }

    /// `<root>/tmp/<this process id>.d`, created if absent: the process's
    /// scratch directory, removed (recursively) when this store is dropped
    /// or cleaned up on a signal.
    pub fn scratch_dir(&self) -> Result<PathBuf> {
        let path = self.inner.scratch_path();
        {
            let g = self.inner.tmp.lock().unwrap_or_else(PoisonError::into_inner);
            if g.1 {
                return Err(StoreError::Io {
                    path,
                    error: io::Error::new(io::ErrorKind::Interrupted, "the store is closed"),
                });
            }
            self.inner.scratch.store(true, Ordering::SeqCst);
        }
        fs::create_dir_all(&path).map_err(io_err(&path))?;
        Ok(path)
    }

    /// Removes every `tmp/` entry of process `pid` (named `<pid>.…`),
    /// directories included: what a child of this process that is known to
    /// be dead left behind (§2 "Cleaning"; the pool calls it for the workers
    /// it reaps). The caller vouches that `pid` is gone; unlike the stale
    /// cleaning of an exclusive open, `/proc` is not consulted. This
    /// process's own id is never removed (a no-op). An entry that vanishes
    /// meanwhile is not an error; the first other failure is returned, after
    /// the remaining entries have been tried.
    pub fn remove_entries_of(&self, pid: u32) -> Result<()> {
        remove_entries_of(&self.inner.tmp_dir(), pid)
    }

    // -- paths -------------------------------------------------------------

    /// `plans/<id>.plan`.
    pub fn plan_path(&self, id: &Digest) -> PathBuf {
        self.root().join("plans").join(format!("{id}.plan"))
    }

    /// `shards/<plan>/<phase>.<round>/<observer>.shard`.
    pub fn shard_path(
        &self,
        plan: &Digest,
        phase: Phase,
        round: u64,
        observer: &Digest,
    ) -> PathBuf {
        self.root()
            .join("shards")
            .join(plan.to_string())
            .join(format!("{}.{round}", phase_name(phase)))
            .join(format!("{observer}.shard"))
    }

    /// `runs/<plan>/<phase>.<round>.<started_unix_ns>.run`.
    pub fn run_path(
        &self,
        plan: &Digest,
        phase: Phase,
        round: u64,
        started_unix_ns: u64,
    ) -> PathBuf {
        self.root()
            .join("runs")
            .join(plan.to_string())
            .join(format!("{}.{round}.{started_unix_ns}.run", phase_name(phase)))
    }

    fn log_file(
        &self,
        plan: &Digest,
        phase: Phase,
        round: u64,
        observer: &Digest,
        attempt: u64,
    ) -> PathBuf {
        self.root()
            .join("logs")
            .join(plan.to_string())
            .join(format!("{}.{round}", phase_name(phase)))
            .join(format!("{observer}.{attempt}.log"))
    }

    // -- plans ---------------------------------------------------------------

    /// Publishes a plan under its content id, which is returned. The bytes
    /// must be a valid `PLAN` file.
    pub fn publish_plan(&self, bytes: &[u8]) -> Result<Digest> {
        if bytes.get(8..12) != Some(&Plan::KIND[..]) {
            return Err(StoreError::Invalid("not a PLAN file".into()));
        }
        let id = formats::content_id(bytes)
            .map_err(|e| StoreError::Invalid(format!("not a valid plan: {e}")))?;
        self.inner.publish(&self.plan_path(&id), bytes)?;
        Ok(id)
    }

    /// The bytes of plan `id`, if the store has it (not validated).
    pub fn read_plan(&self, id: &Digest) -> Result<Option<Vec<u8>>> {
        read_optional(&self.plan_path(id))
    }

    /// Every plan whose id starts with `hex_prefix`, ascending. A prefix
    /// that is not lower-case hexadecimal of at most 64 digits matches
    /// nothing; the empty prefix matches every plan.
    pub fn find_plan(&self, hex_prefix: &str) -> Result<Vec<Digest>> {
        find_ids(&self.root().join("plans"), "plan", hex_prefix)
    }

    // -- declared changes (spec/verify.md §2) ---------------------------------

    /// `declared/<id>.decl`.
    pub fn declared_path(&self, id: &Digest) -> PathBuf {
        self.root().join("declared").join(format!("{id}.decl"))
    }

    /// Every declared changes' id that starts with `hex_prefix`, ascending;
    /// as [`Store::find_plan`] (`slip check --declared <id>`,
    /// `spec/edits.md` §10).
    pub fn find_declared(&self, hex_prefix: &str) -> Result<Vec<Digest>> {
        find_ids(&self.root().join("declared"), "decl", hex_prefix)
    }

    /// Publishes declared changes under their content id, which is
    /// returned. The bytes must be a valid `DECL` file (kind `DECL`, decoded
    /// and checked in full); anything else is `Invalid` and nothing is
    /// written. The directory `declared/` is made if the store was
    /// initialised before it existed.
    pub fn publish_declared(&self, bytes: &[u8]) -> Result<Digest> {
        if bytes.get(8..12) != Some(&Declared::KIND[..]) {
            return Err(StoreError::Invalid("not a DECL file".into()));
        }
        let id = formats::content_id(bytes)
            .map_err(|e| StoreError::Invalid(format!("not a valid DECL file: {e}")))?;
        self.inner.publish(&self.declared_path(&id), bytes)?;
        Ok(id)
    }

    /// The bytes of declared changes `id`, if the store has them (not
    /// validated: a reader checks the kind and the content id).
    pub fn read_declared(&self, id: &Digest) -> Result<Option<Vec<u8>>> {
        read_optional(&self.declared_path(id))
    }

    // -- shards --------------------------------------------------------------

    /// Publishes one observer's shard and returns its content id. The bytes
    /// must be a valid `SHRD` file whose header names `observer`, `phase`
    /// and `round`.
    pub fn publish_shard(
        &self,
        plan: &Digest,
        phase: Phase,
        round: u64,
        observer: &Digest,
        bytes: &[u8],
    ) -> Result<Digest> {
        let shard = Shard::from_bytes(bytes)
            .map_err(|e| StoreError::Invalid(format!("not a valid shard: {e}")))?;
        if shard.observer != *observer || shard.phase != phase || shard.round != round {
            return Err(StoreError::Invalid(format!(
                "the shard is for observer {}, {} round {}, not observer {observer}, {} round {round}",
                shard.observer,
                phase_name(shard.phase),
                shard.round,
                phase_name(phase),
            )));
        }
        let id = trailer(bytes);
        self.inner.publish(&self.shard_path(plan, phase, round, observer), bytes)?;
        Ok(id)
    }

    /// The bytes of a shard, if the store has it (not validated).
    pub fn read_shard(
        &self,
        plan: &Digest,
        phase: Phase,
        round: u64,
        observer: &Digest,
    ) -> Result<Option<Vec<u8>>> {
        read_optional(&self.shard_path(plan, phase, round, observer))
    }

    // -- run records ---------------------------------------------------------

    /// Publishes a run record and returns its path. The bytes must be a
    /// valid `RUNR` file whose `plan`, `phase`, `round` and
    /// `started_unix_ns` are the ones given.
    pub fn publish_run(
        &self,
        plan: &Digest,
        phase: Phase,
        round: u64,
        started_unix_ns: u64,
        bytes: &[u8],
    ) -> Result<PathBuf> {
        let record = RunRecord::from_bytes(bytes)
            .map_err(|e| StoreError::Invalid(format!("not a valid run record: {e}")))?;
        let r = &record.run;
        if r.plan != *plan
            || r.phase != phase
            || r.round != round
            || r.started_unix_ns != started_unix_ns
        {
            return Err(StoreError::Invalid(format!(
                "the run record is for plan {}, {} round {}, started {}, not plan {plan}, {} \
                 round {round}, started {started_unix_ns}",
                r.plan,
                phase_name(r.phase),
                r.round,
                r.started_unix_ns,
                phase_name(phase),
            )));
        }
        let path = self.run_path(plan, phase, round, started_unix_ns);
        self.inner.publish(&path, bytes)?;
        Ok(path)
    }

    /// The bytes of the run record of `plan` and `phase`, any round, with
    /// the greatest `started_unix_ns` (then the greatest round); `None` if
    /// there is none. Not validated.
    pub fn latest_run(&self, plan: &Digest, phase: Phase) -> Result<Option<Vec<u8>>> {
        let dir = self.root().join("runs").join(plan.to_string());
        let mut best: Option<(u64, u64, String)> = None;
        for name in list_names(&dir)? {
            let Some(key) = parse_run_name(&name, phase) else { continue };
            if best.as_ref().is_none_or(|b| (key.1, key.0) > (b.1, b.0)) {
                best = Some((key.0, key.1, name));
            }
        }
        match best {
            None => Ok(None),
            Some((_, _, name)) => read_optional(&dir.join(name)),
        }
    }

    /// `runs/<plan>/transform.<round>.<started_unix_ns>.run`.
    fn transform_run_path(&self, plan: &Digest, round: u64, started_unix_ns: u64) -> PathBuf {
        self.root()
            .join("runs")
            .join(plan.to_string())
            .join(format!("{TRANSFORM}.{round}.{started_unix_ns}.run"))
    }

    /// The bytes of the run record of `plan`, `phase` and `round` with the
    /// greatest `started_unix_ns`; `None` if there is none. Not validated.
    /// A transform run record is never taken for a phase's.
    pub fn latest_run_of_round(
        &self,
        plan: &Digest,
        phase: Phase,
        round: u64,
    ) -> Result<Option<Vec<u8>>> {
        let dir = self.root().join("runs").join(plan.to_string());
        let mut best: Option<(u64, String)> = None;
        for name in list_names(&dir)? {
            let Some((r, ns)) = parse_run_name(&name, phase) else { continue };
            if r == round && best.as_ref().is_none_or(|b| ns > b.0) {
                best = Some((ns, name));
            }
        }
        match best {
            None => Ok(None),
            Some((_, name)) => read_optional(&dir.join(name)),
        }
    }

    // -- a transform's run (spec/edits.md §10.4) -----------------------------

    /// `<dir>/<id>.<extension>` of `object`.
    pub fn object_path(&self, object: Object, id: &Digest) -> PathBuf {
        self.root().join(object.dir()).join(format!("{id}.{}", object.extension()))
    }

    /// Publishes a file of `object`'s kind under its content id, which is
    /// returned. `Invalid` (nothing written) if `bytes` is not a container
    /// of that kind (`spec/model.md` §5: magic, kind, the kind's version
    /// as [`Object::version`] gives it, trailer,
    /// section framing) or not a valid file of it, decoded and checked in
    /// full by `slip_model::formats::content_id`. The file is left
    /// untouched when it already holds that content.
    pub fn publish_object(&self, object: Object, bytes: &[u8]) -> Result<Digest> {
        let id = container_id(bytes, object.kind(), object.version())?;
        formats::content_id(bytes).map_err(|e| {
            let name = String::from_utf8_lossy(&object.kind()).into_owned();
            StoreError::Invalid(format!("not a valid {name} file: {e}"))
        })?;
        self.publish_once(&self.object_path(object, &id), bytes)?;
        Ok(id)
    }

    /// The bytes of `object` `id`, if the store has it (not validated).
    pub fn read_object(&self, object: Object, id: &Digest) -> Result<Option<Vec<u8>>> {
        read_optional(&self.object_path(object, id))
    }

    /// Every `object` whose id starts with `hex_prefix`, ascending; as
    /// [`Store::find_plan`].
    pub fn find_object(&self, object: Object, hex_prefix: &str) -> Result<Vec<Digest>> {
        find_ids(&self.root().join(object.dir()), object.extension(), hex_prefix)
    }

    /// `blobs/<content digest>`: a file's bytes, no container.
    pub fn blob_path(&self, id: &Digest) -> PathBuf {
        self.root().join("blobs").join(id.to_string())
    }

    /// Publishes a file's bytes under their content digest
    /// (`spec/model.md` §6), which is returned. The file is left untouched
    /// when it already holds that content.
    pub fn publish_blob(&self, bytes: &[u8]) -> Result<Digest> {
        let id = content_digest(bytes);
        self.publish_once(&self.blob_path(&id), bytes)?;
        Ok(id)
    }

    /// The bytes of blob `id`, if the store has it; `Invalid` if they do
    /// not hash to `id` (`spec/edits.md` §10.4).
    pub fn read_blob(&self, id: &Digest) -> Result<Option<Vec<u8>>> {
        let path = self.blob_path(id);
        let Some(bytes) = read_optional(&path)? else { return Ok(None) };
        let actual = content_digest(&bytes);
        if actual != *id {
            return Err(StoreError::Invalid(format!(
                "the blob {} does not hash to its name (its content digest is {actual})",
                path.display()
            )));
        }
        Ok(Some(bytes))
    }

    /// The round a transform's run of `plan` takes (`spec/edits.md` §8,
    /// `spec/exec.md` §2): one more than the greatest round the plan has
    /// taken, and at least 1. A round is taken by a shard directory
    /// `map.<r>` or `verify.<r>` (a directory: a file of that name takes
    /// nothing) or a transform run record `transform.<r>.<ns>.run`, with
    /// `r` and `ns` canonical decimal. `Invalid` if the greatest round
    /// taken is `u64::MAX`.
    pub fn next_round(&self, plan: &Digest) -> Result<u64> {
        let mut greatest: Option<u64> = None;
        let mut take = |r: u64| greatest = Some(greatest.map_or(r, |g| g.max(r)));
        let shards = self.root().join("shards").join(plan.to_string());
        for (name, is_dir) in list_entries(&shards)? {
            let round = [Phase::Map, Phase::Verify].into_iter().find_map(|phase| {
                name.strip_prefix(phase_name(phase))?.strip_prefix('.').and_then(decimal)
            });
            if let (Some(r), true) = (round, is_dir) {
                take(r);
            }
        }
        let runs = self.root().join("runs").join(plan.to_string());
        for name in list_names(&runs)? {
            if let Some((r, _)) = parse_transform_run_name(&name) {
                take(r);
            }
        }
        match greatest {
            None => Ok(1),
            Some(g) => g.checked_add(1).map(|r| r.max(1)).ok_or_else(|| {
                StoreError::Invalid(format!("plan {plan} has taken the last round, {g}"))
            }),
        }
    }

    /// Whether a transform's run took `round` of `plan`: it has a transform
    /// run record (`transform.<round>.<ns>.run`).
    pub fn is_transform_round(&self, plan: &Digest, round: u64) -> Result<bool> {
        let runs = self.root().join("runs").join(plan.to_string());
        Ok(list_names(&runs)?
            .iter()
            .any(|name| parse_transform_run_name(name).is_some_and(|(r, _)| r == round)))
    }

    /// Publishes a transform run record (`RUNT`, `spec/edits.md` §10.6) and
    /// returns its path, `runs/<plan>/transform.<round>.<started_unix_ns>.run`.
    /// The bytes must be a valid `RUNT` file, the whole record checked as
    /// `slip_model::formats::transform_run_names` checks it, whose section
    /// `run` names `plan`, `round` and `started_unix_ns`. Anything else is
    /// `Invalid`.
    pub fn publish_transform_run(
        &self,
        plan: &Digest,
        round: u64,
        started_unix_ns: u64,
        bytes: &[u8],
    ) -> Result<PathBuf> {
        let (p, r, ns) = formats::transform_run_names(bytes)
            .map_err(|e| StoreError::Invalid(format!("not a valid transform run record: {e}")))?;
        if p != *plan || r != round || ns != started_unix_ns {
            return Err(StoreError::Invalid(format!(
                "the transform run record is for plan {p}, round {r}, started {ns}, not plan \
                 {plan}, round {round}, started {started_unix_ns}"
            )));
        }
        let path = self.transform_run_path(plan, round, started_unix_ns);
        self.inner.publish(&path, bytes)?;
        Ok(path)
    }

    /// `<store>/write.journal` (`spec/edits.md` §7.3): present only during a
    /// write of the working tree.
    pub fn journal_path(&self) -> PathBuf {
        self.root().join("write.journal")
    }

    /// Creates `write.journal` holding `bytes` (§2.1 "The journal"): created
    /// exclusively under its name, written, synced, then the store's
    /// directory synced. An existing journal is `Io` `AlreadyExists` and is
    /// left as it was. If the write or a sync fails, the journal is removed
    /// (if it can be) and the error returned.
    pub fn publish_journal(&self, bytes: &[u8]) -> Result<()> {
        let path = self.journal_path();
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(io_err(&path))?;
        let written = f.write_all(bytes).and_then(|()| f.sync_all()).and_then(|()| {
            drop(f);
            sync_dir(self.root())
        });
        if let Err(e) = written {
            let _ = fs::remove_file(&path);
            return Err(StoreError::Io { path, error: e });
        }
        Ok(())
    }

    /// The bytes of `write.journal`; `None` if there is none.
    pub fn read_journal(&self) -> Result<Option<Vec<u8>>> {
        read_optional(&self.journal_path())
    }

    /// Removes `write.journal` and syncs the store's directory. No journal
    /// is `Io` `NotFound`.
    pub fn remove_journal(&self) -> Result<()> {
        let path = self.journal_path();
        fs::remove_file(&path).map_err(io_err(&path))?;
        sync_dir(self.root()).map_err(io_err(self.root()))
    }

    /// Publishes content-addressed `bytes` at `dest`, leaving `dest`
    /// untouched when it already holds that content: a regular file (not a
    /// symbolic link) whose bytes are `bytes`. Anything else at the name (other
    /// bytes, a symbolic link, dangling or not) is replaced by the publish of
    /// §2.1; a directory there makes the rename fail (`Io`).
    ///
    /// Comparing the bytes is the check of the content's name: for a blob,
    /// the file hashes to the id; for an object, it passes the frame check
    /// and its content id is the id (two files with one valid trailer
    /// differ only by a collision of the digest).
    fn publish_once(&self, dest: &Path, bytes: &[u8]) -> Result<()> {
        match fs::symlink_metadata(dest) {
            Ok(m) if m.is_file() => match fs::read(dest) {
                Ok(have) if have == bytes => Ok(()),
                Ok(_) => self.inner.publish(dest, bytes),
                Err(e) if e.kind() == io::ErrorKind::NotFound => self.inner.publish(dest, bytes),
                Err(e) => Err(StoreError::Io { path: dest.to_path_buf(), error: e }),
            },
            Ok(_) => self.inner.publish(dest, bytes),
            Err(e) if e.kind() == io::ErrorKind::NotFound => self.inner.publish(dest, bytes),
            Err(e) => Err(StoreError::Io { path: dest.to_path_buf(), error: e }),
        }
    }

    // -- logs ----------------------------------------------------------------

    /// The log file of one attempt, `logs/<plan>/<phase>.<round>/<observer>.<attempt>.log`.
    /// Its parent directories are created; the file itself is not.
    pub fn log_path(
        &self,
        plan: &Digest,
        phase: Phase,
        round: u64,
        observer: &Digest,
        attempt: u64,
    ) -> Result<PathBuf> {
        let path = self.log_file(plan, phase, round, observer, attempt);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(io_err(parent))?;
        }
        Ok(path)
    }

    /// Removes the log of one attempt (logs are kept only for failed
    /// attempts). A log that does not exist is not an error.
    pub fn remove_log(
        &self,
        plan: &Digest,
        phase: Phase,
        round: u64,
        observer: &Digest,
        attempt: u64,
    ) -> Result<()> {
        let path = self.log_file(plan, phase, round, observer, attempt);
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(StoreError::Io { path, error: e }),
        }
    }
}

/// What [`crate::signals::install`] needs to remove this store's `tmp/`
/// files, its scratch directory and, if it holds it, the lock (§7). Once it
/// has run, the store refuses to create `tmp/` files. Never panics.
pub fn cleanup_on_signal(store: &Store) -> impl FnOnce() + Send + 'static {
    let inner = Arc::clone(&store.inner);
    move || inner.clean()
}

/// What the signal path needs to call [`Store::remove_entries_of`] without
/// the store: the store's `tmp/` directory.
pub(crate) fn tmp_dir_of(store: &Store) -> PathBuf {
    store.inner.tmp_dir()
}

/// [`Store::remove_entries_of`] on the `tmp/` directory `dir`.
pub(crate) fn remove_entries_of(dir: &Path, pid: u32) -> Result<()> {
    if pid == std::process::id() {
        return Ok(());
    }
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(StoreError::Io { path: dir.to_path_buf(), error: e }),
    };
    let mut first = None;
    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                first.get_or_insert(StoreError::Io { path: dir.to_path_buf(), error: e });
                continue;
            }
        };
        let name = entry.file_name();
        if name.to_str().and_then(pid_prefix) != Some(pid) {
            continue;
        }
        let path = entry.path();
        let removed = match entry.file_type() {
            Ok(t) if t.is_dir() => fs::remove_dir_all(&path),
            Ok(_) => fs::remove_file(&path),
            Err(e) => Err(e),
        };
        match removed {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => {
                first.get_or_insert(StoreError::Io { path, error: e });
            }
        }
    }
    first.map_or(Ok(()), Err)
}

/// The ids of the `<64 hex>.<extension>` files of `dir` that start with
/// `hex_prefix`, ascending (§2.1 `find_plan`).
fn find_ids(dir: &Path, extension: &str, hex_prefix: &str) -> Result<Vec<Digest>> {
    if hex_prefix.len() > 64 || !hex_prefix.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f'))
    {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for name in list_names(dir)? {
        let Some(hex) = name.strip_suffix(extension).and_then(|n| n.strip_suffix('.')) else {
            continue;
        };
        let Some(id) = Digest::from_hex(hex) else { continue };
        if hex.starts_with(hex_prefix) {
            out.push(id);
        }
    }
    out.sort();
    Ok(out)
}

/// A file of a transform's run (`spec/edits.md` §10.4), published under its
/// content id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Object {
    /// `transforms/<request id>.trfm`, kind `TRFM`.
    Transform,
    /// `edits/<edit set id>.edts`, kind `EDTS`.
    EditSet,
    /// `overlays/<overlay id>.ovly`, kind `OVLY`.
    Overlay,
    /// `results/<result id>.rslt`, kind `RSLT`.
    Result,
}

impl Object {
    /// The directory under the store's root.
    pub fn dir(self) -> &'static str {
        match self {
            Object::Transform => "transforms",
            Object::EditSet => "edits",
            Object::Overlay => "overlays",
            Object::Result => "results",
        }
    }

    /// The file name's extension.
    pub fn extension(self) -> &'static str {
        match self {
            Object::Transform => "trfm",
            Object::EditSet => "edts",
            Object::Overlay => "ovly",
            Object::Result => "rslt",
        }
    }

    /// The container kind (`spec/model.md` §5.1).
    pub fn kind(self) -> [u8; 4] {
        match self {
            Object::Transform => *b"TRFM",
            Object::EditSet => *b"EDTS",
            Object::Overlay => *b"OVLY",
            Object::Result => *b"RSLT",
        }
    }

    /// The highest container version of the kind this Slip reads: the
    /// model's own (`spec/edits.md` §3, §6, §7.1, §8.5, §10.6; RSLT is at
    /// version 2 since WP-111, whose decoder refuses version 1).
    pub fn version(self) -> u32 {
        match self {
            Object::Transform => Transform::VERSION,
            Object::EditSet => EditSet::VERSION,
            Object::Overlay => Overlay::VERSION,
            Object::Result => TransformResult::VERSION,
        }
    }
}

/// The first component of a transform run record's name.
const TRANSFORM: &str = "transform";

/// The content id of a container of `kind` at most at `version`, checked as
/// `spec/model.md` §5 rule 5 checks the frame (the sections are not
/// decoded); `Invalid` otherwise.
fn container_id(bytes: &[u8], kind: [u8; 4], version: u32) -> Result<Digest> {
    let name = String::from_utf8_lossy(&kind).into_owned();
    Reader::open(bytes, kind, version)
        .map(|r| r.content_id())
        .map_err(|e| StoreError::Invalid(format!("not a valid {name} container: {e}")))
}

/// `transform.<round>.<started_unix_ns>.run`: (round, started).
fn parse_transform_run_name(name: &str) -> Option<(u64, u64)> {
    let rest = name.strip_suffix(".run")?.strip_prefix(TRANSFORM)?.strip_prefix('.')?;
    let (round, started) = rest.split_once('.')?;
    Some((decimal(round)?, decimal(started)?))
}

/// The content id of a file already validated: its trailer.
fn trailer(bytes: &[u8]) -> Digest {
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes[bytes.len() - TRAILER_LEN..]);
    Digest(out)
}

/// `<phase>.<round>.<started_unix_ns>.run` for `phase`: (round, started).
fn parse_run_name(name: &str, phase: Phase) -> Option<(u64, u64)> {
    let rest = name.strip_suffix(".run")?.strip_prefix(phase_name(phase))?.strip_prefix('.')?;
    let (round, started) = rest.split_once('.')?;
    Some((decimal(round)?, decimal(started)?))
}

/// The UTF-8 names in `dir`, each with whether it is a directory (not
/// following a symbolic link); none if `dir` does not exist.
fn list_entries(dir: &Path) -> Result<Vec<(String, bool)>> {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(StoreError::Io { path: dir.to_path_buf(), error: e }),
    };
    let mut out = Vec::new();
    for entry in entries {
        let entry = entry.map_err(io_err(dir))?;
        let is_dir = entry.file_type().map_err(io_err(&entry.path()))?.is_dir();
        if let Ok(name) = entry.file_name().into_string() {
            out.push((name, is_dir));
        }
    }
    Ok(out)
}

/// The UTF-8 names in `dir`; none if it does not exist.
fn list_names(dir: &Path) -> Result<Vec<String>> {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(StoreError::Io { path: dir.to_path_buf(), error: e }),
    };
    let mut out = Vec::new();
    for entry in entries {
        let entry = entry.map_err(io_err(dir))?;
        if let Ok(name) = entry.file_name().into_string() {
            out.push(name);
        }
    }
    Ok(out)
}

/// Syncs directory `dir`, so that the names created or removed in it last.
fn sync_dir(dir: &Path) -> io::Result<()> {
    fs::File::open(dir)?.sync_all()
}

fn read_optional(path: &Path) -> Result<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(b) => Ok(Some(b)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(StoreError::Io { path: path.to_path_buf(), error: e }),
    }
}

/// Initialises the store at `root`, which has no `VERSION`: only if it is
/// absent or empty (§2 "Opening").
fn initialise(root: &Path) -> Result<()> {
    match fs::read_dir(root) {
        Ok(mut entries) => {
            if entries.next().is_some() {
                return Err(StoreError::Unrecognised(root.to_path_buf()));
            }
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            fs::create_dir_all(root).map_err(io_err(root))?;
        }
        Err(e) if e.kind() == io::ErrorKind::NotADirectory => {
            return Err(StoreError::Unrecognised(root.to_path_buf()));
        }
        Err(e) => return Err(StoreError::Io { path: root.to_path_buf(), error: e }),
    }
    for sub in ["tmp", "plans", "declared", "shards", "runs", "logs"] {
        let d = root.join(sub);
        fs::create_dir_all(&d).map_err(io_err(&d))?;
    }
    // VERSION last and atomically: a store with VERSION is complete.
    let staging = Inner {
        root: root.to_path_buf(),
        lock: None,
        tmp: Mutex::new((HashSet::new(), false)),
        scratch: AtomicBool::new(false),
    };
    let result = staging.publish(&root.join("VERSION"), VERSION_TEXT.as_bytes());
    staging.clean();
    result
}

/// The sequences of `spec/landed/harness/WP-159.md` item 4 that need this
/// module's counter: a `tmp/` name is never one an earlier process with
/// this pid could have made.
#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let d =
            std::env::temp_dir().join(format!("slip-exec-store-unit-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    // spec/exec.md §2.1 "`tmp/` names": `<pid>.<t>.<n>.tmp`, the token the
    // same for every name of the process, and the pid what cleaning reads.
    #[test]
    fn tmp_names_carry_the_process_token() {
        let pid = std::process::id();
        let t = process_token();
        assert!(t > 0);
        assert_eq!(process_token(), t);
        assert_eq!(tmp_name(7), format!("{pid}.{t}.7.tmp"));
        assert_eq!(pid_prefix(&tmp_name(7)), Some(pid));
        assert_ne!(tmp_name(7), format!("{pid}.7.tmp"));
    }

    // spec/exec.md §2.1: what an earlier process with this pid left in
    // `tmp/` under the names it made (`<pid>.<n>.tmp` for the counters
    // this process is about to use) is not made again: the publishes
    // and the lock succeed beside it, and leave it as it was. Kills "a
    // temporary name reused".
    #[test]
    fn an_earlier_process_names_are_not_made_again() {
        let d = scratch("earlier");
        let store = Store::open(&d, Access::Shared).unwrap();
        let pid = std::process::id();
        let next = TMP_SEQ.load(Ordering::SeqCst);
        let old: Vec<PathBuf> = (next..next + 64)
            .flat_map(|n| {
                [
                    format!("{pid}.{n}.tmp"),
                    tmp_name(n).replace(&format!(".{}", process_token()), ".1"),
                ]
            })
            .map(|name| d.join("tmp").join(name))
            .collect();
        for p in &old {
            fs::write(p, "earlier\n").unwrap();
        }
        let blob = store.publish_blob(b"now\n").unwrap();
        assert_eq!(store.read_blob(&blob).unwrap().as_deref(), Some(&b"now\n"[..]));
        drop(store);
        let store = Store::open(&d, Access::Exclusive);
        // The planted names start with this live pid: stale cleaning
        // keeps them, and the lock was taken beside them.
        let store = store.unwrap();
        assert_eq!(fs::read_to_string(d.join("lock")).unwrap(), format!("{pid}\n"));
        for p in &old {
            assert_eq!(fs::read_to_string(p).unwrap(), "earlier\n", "{}", p.display());
        }
        drop(store);
        let _ = fs::remove_dir_all(&d);
    }

    // spec/exec.md §2.1 "The lock": created in place, so `lock` is its
    // file's one name, and the claim is gone once the lock is held.
    #[test]
    fn the_lock_is_created_in_place() {
        use std::os::unix::fs::MetadataExt as _;
        let d = scratch("lock");
        let store = Store::open(&d, Access::Exclusive).unwrap();
        let pid = std::process::id();
        assert_eq!(fs::read_to_string(d.join("lock")).unwrap(), format!("{pid}\n"));
        assert_eq!(fs::metadata(d.join("lock")).unwrap().nlink(), 1);
        let mine = format!("{pid}.");
        let left: Vec<String> = fs::read_dir(d.join("tmp"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with(&mine))
            .collect();
        assert_eq!(left, Vec::<String>::new());
        drop(store);
        assert!(!d.join("lock").exists());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn lock_texts() {
        assert_eq!(lock_state(b"12\n"), LockText::Whole(12));
        assert_eq!(lock_state(b"0\n"), LockText::Whole(0));
        assert_eq!(lock_state(b""), LockText::Partial);
        assert_eq!(lock_state(b"12"), LockText::Partial);
        assert_eq!(lock_state(b"4294967296"), LockText::Partial);
        assert_eq!(lock_state(b"012\n"), LockText::Other);
        assert_eq!(lock_state(b"4294967296\n"), LockText::Other);
        assert_eq!(lock_state(b"12345678901"), LockText::Other);
        assert_eq!(lock_state(b"x"), LockText::Other);
        assert_eq!(lock_state(b"\n"), LockText::Other);
    }

    /// A file system in memory for [`take`] and [`release`], seen by
    /// processes ([`Proc`]): every operation of a process is numbered, and
    /// a process can crash before, fail at, or let another run before, any
    /// one of them.
    mod world {
        use super::*;
        use std::cell::{Cell, RefCell};
        use std::collections::{BTreeMap, HashMap};

        pub const LOCK: &str = "/s/lock";
        pub const TMP: &str = "/s/tmp";

        #[derive(Default)]
        pub struct World {
            /// Name to (inode, bytes).
            pub files: RefCell<BTreeMap<PathBuf, (u64, Vec<u8>)>>,
            pub dead: RefCell<HashSet<u32>>,
            /// A name created from now on reads as absent its next `lie`
            /// reads (the hazard of `spec/harness.md` §11 for a new name).
            pub lie: Cell<u32>,
            lies: RefCell<HashMap<PathBuf, u32>>,
            ino: Cell<u64>,
        }

        impl World {
            pub fn put(&self, path: &str, bytes: &[u8]) {
                let ino = self.ino.get() + 1;
                self.ino.set(ino);
                self.files.borrow_mut().insert(PathBuf::from(path), (ino, bytes.to_vec()));
            }

            pub fn lock(&self) -> Option<Vec<u8>> {
                self.files.borrow().get(Path::new(LOCK)).map(|f| f.1.clone())
            }

            pub fn kill(&self, pid: u32) {
                self.dead.borrow_mut().insert(pid);
            }
        }

        type Hook<'w> = Box<dyn FnOnce() + 'w>;

        pub struct Proc<'w> {
            pub w: &'w World,
            pub pid: u32,
            pub n: Cell<u64>,
            pub crash_at: Option<u64>,
            pub fail_at: Option<u64>,
            pub hook: RefCell<Option<(u64, Hook<'w>)>>,
            seq: Cell<u64>,
        }

        impl<'w> Proc<'w> {
            pub fn new(w: &'w World, pid: u32) -> Proc<'w> {
                Proc {
                    w,
                    pid,
                    n: Cell::new(0),
                    crash_at: None,
                    fail_at: None,
                    hook: RefCell::new(None),
                    seq: Cell::new(0),
                }
            }

            pub fn take(&self, held_here: bool) -> Result<()> {
                let mut next = || {
                    let n = self.seq.get();
                    self.seq.set(n + 1);
                    format!("{}.1.{n}.claim", self.pid)
                };
                take(self, Path::new(LOCK), Path::new(TMP), self.pid, held_here, &mut next)
            }

            pub fn release(&self) -> bool {
                release(self, Path::new(LOCK), self.pid)
            }

            /// One operation: the crash, the hook, the failure.
            fn op(&self) -> io::Result<()> {
                let k = self.n.get();
                self.n.set(k + 1);
                if self.crash_at == Some(k) {
                    self.w.kill(self.pid);
                }
                if self.w.dead.borrow().contains(&self.pid) {
                    // A dead process does nothing more.
                    return Err(io::Error::other("dead"));
                }
                let hook = {
                    let mut h = self.hook.borrow_mut();
                    if h.as_ref().is_some_and(|(at, _)| *at == k) { h.take() } else { None }
                };
                if let Some((_, f)) = hook {
                    f();
                }
                if self.fail_at == Some(k) {
                    return Err(io::Error::other("injected"));
                }
                Ok(())
            }

            fn make(&self, path: &Path) -> io::Result<u64> {
                let mut files = self.w.files.borrow_mut();
                if files.contains_key(path) {
                    return Err(io::ErrorKind::AlreadyExists.into());
                }
                let ino = self.w.ino.get() + 1;
                self.w.ino.set(ino);
                files.insert(path.to_path_buf(), (ino, Vec::new()));
                self.w.lies.borrow_mut().insert(path.to_path_buf(), self.w.lie.get());
                Ok(ino)
            }

            /// Writes to the file `ino`, if it still has the name `path`.
            fn write(&self, path: &Path, ino: u64, bytes: &[u8]) {
                if let Some(f) = self.w.files.borrow_mut().get_mut(path)
                    && f.0 == ino
                {
                    f.1 = bytes.to_vec();
                }
            }
        }

        impl LockFs for Proc<'_> {
            fn create(&self, path: &Path) -> io::Result<()> {
                self.op()?;
                self.make(path).map(drop)
            }

            fn create_written(&self, path: &Path, bytes: &[u8]) -> std::result::Result<(), Made> {
                self.op().map_err(Made::Not)?;
                let ino = self.make(path).map_err(Made::Not)?;
                // The line in two writes, then the sync: a crash between
                // leaves the empty file, or the digits without the newline.
                self.op().map_err(Made::Partly)?;
                self.write(path, ino, &bytes[..bytes.len() - 1]);
                self.op().map_err(Made::Partly)?;
                self.write(path, ino, bytes);
                self.op().map_err(Made::Partly)
            }

            fn read(&self, path: &Path) -> io::Result<Option<Vec<u8>>> {
                self.op()?;
                if let Some(n) = self.w.lies.borrow_mut().get_mut(path)
                    && *n > 0
                {
                    *n -= 1;
                    return Ok(None);
                }
                Ok(self.w.files.borrow().get(path).map(|f| f.1.clone()))
            }

            fn remove(&self, path: &Path) -> io::Result<()> {
                self.op()?;
                self.w.lies.borrow_mut().remove(path);
                match self.w.files.borrow_mut().remove(path) {
                    Some(_) => Ok(()),
                    None => Err(io::ErrorKind::NotFound.into()),
                }
            }

            fn list(&self, dir: &Path) -> io::Result<Vec<String>> {
                self.op()?;
                Ok(self
                    .w
                    .files
                    .borrow()
                    .keys()
                    .filter(|p| p.parent() == Some(dir))
                    .filter_map(|p| p.file_name()?.to_str().map(str::to_owned))
                    .collect())
            }

            fn alive(&self, pid: u32) -> bool {
                let _ = self.op();
                !self.w.dead.borrow().contains(&pid)
            }

            fn pause(&self, _round: u32) {}
        }
    }

    use world::{Proc, World};

    const A: u32 = 100;
    const B: u32 = 200;
    const C: u32 = 300;
    const DEAD: u32 = 7;

    /// The stores an opener can find: (name, files, whether the lock is
    /// `A`'s own left by a close of this process).
    fn starts() -> Vec<(&'static str, World)> {
        let mut out = Vec::new();
        for (name, lock, claim) in [
            ("absent", None, false),
            ("a dead pid's", Some(format!("{DEAD}\n")), false),
            ("empty", Some(String::new()), false),
            ("partial", Some("12".to_owned()), false),
            ("ten digits", Some("4294967295".to_owned()), false),
            ("a dead claim", None, true),
            ("a dead claim and lock", Some(format!("{DEAD}\n")), true),
            ("A's own, left", Some(format!("{A}\n")), false),
        ] {
            let w = World::default();
            w.kill(DEAD);
            if let Some(text) = lock {
                w.put(world::LOCK, text.as_bytes());
            }
            if claim {
                w.put(&format!("{}/{DEAD}.1.0.claim", world::TMP), b"");
            }
            out.push((name, w));
        }
        out
    }

    fn holds(w: &World, pid: u32) -> bool {
        w.lock().as_deref() == Some(lock_text(pid).as_bytes())
    }

    /// The operations of `A`'s take when alone, from each start.
    fn solo_ops() -> u64 {
        starts()
            .iter()
            .map(|(_, w)| {
                let a = Proc::new(w, A);
                a.take(false).unwrap();
                a.n.get()
            })
            .max()
            .unwrap()
    }

    fn is_busy(r: &Result<()>, pid: u32) -> bool {
        matches!(r, Err(StoreError::Busy { pid: p, .. }) if *p == pid)
    }

    // §2.1 "After a crash at any one step": `A` dies before each of the
    // operations of its take or of its close; then `B` alone takes the
    // lock. Kills "an empty or partial lock is not a lock to take over"
    // and "a dead process's claim is a contender".
    #[test]
    fn a_crash_at_any_step_never_wedges_the_lock() {
        let n = solo_ops();
        for k in 0..=n {
            for (name, w) in starts() {
                let mut a = Proc::new(&w, A);
                a.crash_at = Some(k);
                let r = a.take(false);
                if r.is_ok() {
                    assert!(holds(&w, A), "{name}, crash at {k}");
                    w.kill(A);
                }
                let b = Proc::new(&w, B);
                let rb = b.take(false);
                assert!(rb.is_ok(), "{name}, crash at {k}: {rb:?}");
                assert!(holds(&w, B), "{name}, crash at {k}");
            }
        }
        // The close: `A` holds, and dies before each step of its release.
        for k in 0..4 {
            let w = World::default();
            Proc::new(&w, A).take(false).unwrap();
            let mut a = Proc::new(&w, A);
            a.crash_at = Some(k);
            a.release();
            let rb = Proc::new(&w, B).take(false);
            assert!(rb.is_ok(), "close crash at {k}: {rb:?}");
        }
    }

    // A take that fails at any one operation leaves no lock of its own
    // behind, and the lock is free again once `A` ends (a claim whose
    // removal failed is a contender while `A` lives).
    #[test]
    fn a_failure_at_any_step_leaves_the_lock_takeable() {
        let n = solo_ops();
        for k in 0..n {
            for (name, w) in starts() {
                let left = holds(&w, A);
                let mut a = Proc::new(&w, A);
                a.fail_at = Some(k);
                let r = a.take(false);
                if r.is_ok() {
                    assert!(holds(&w, A), "{name}, fail at {k}");
                    assert!(Proc::new(&w, A).release());
                } else if !left {
                    assert!(!holds(&w, A), "{name}, fail at {k}: {r:?}");
                }
                let b = Proc::new(&w, B);
                if b.take(false).is_err() {
                    w.kill(A);
                    let rb = Proc::new(&w, B).take(false);
                    assert!(rb.is_ok(), "{name}, fail at {k}: {rb:?}");
                }
                assert!(holds(&w, B), "{name}, fail at {k}");
            }
        }
    }

    // §2.1 "Why two openers never both hold the lock": `B` takes (to the
    // end) between any two operations of `A`'s take, from every start,
    // with created names read as absent 0, 1 or 3 times. Never both; with
    // honest reads, exactly one, and the lock is its. Kills "the reader
    // takes an empty lock as free without its claim" (step 2 skipped) and
    // "a claim made after the listing".
    #[test]
    fn two_openers_never_both_hold_the_lock() {
        let n = solo_ops();
        for lie in [0, 1, 3] {
            for k in 0..n {
                for (name, w) in starts() {
                    w.lie.set(lie);
                    let rb = std::rc::Rc::new(std::cell::RefCell::new(None));
                    let a = Proc::new(&w, A);
                    let (wr, rbr) = (&w, rb.clone());
                    *a.hook.borrow_mut() = Some((
                        k,
                        Box::new(move || {
                            let r = Proc::new(wr, B).take(false);
                            *rbr.borrow_mut() = Some(r);
                        }),
                    ));
                    let ra = a.take(false);
                    // A take shorter than `k` operations: `B` after it.
                    let ran = rb.borrow_mut().take();
                    let rb = ran.unwrap_or_else(|| Proc::new(&w, B).take(false));
                    let at = format!("{name}, lie {lie}, B before A's op {k}: {ra:?} {rb:?}");
                    assert!(!(ra.is_ok() && rb.is_ok()), "{at}");
                    if lie == 0 {
                        assert!(ra.is_ok() != rb.is_ok(), "{at}");
                        assert!(holds(&w, if ra.is_ok() { A } else { B }), "{at}");
                    }
                }
            }
        }
    }

    // Three: `C` takes between two operations of `B`'s, itself between two
    // of `A`'s.
    #[test]
    fn three_openers_never_two_hold_the_lock() {
        let n = solo_ops();
        for k in 0..n {
            for j in 0..n + 8 {
                for (name, w) in starts() {
                    let res = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
                    let a = Proc::new(&w, A);
                    let (wr, r1) = (&w, res.clone());
                    *a.hook.borrow_mut() = Some((
                        k,
                        Box::new(move || {
                            let b = Proc::new(wr, B);
                            let r2 = r1.clone();
                            *b.hook.borrow_mut() = Some((
                                j,
                                Box::new(move || {
                                    let ok = Proc::new(wr, C).take(false).is_ok();
                                    r2.borrow_mut().push(ok);
                                }),
                            ));
                            let ok = b.take(false).is_ok();
                            r1.borrow_mut().push(ok);
                        }),
                    ));
                    let ra = a.take(false).is_ok();
                    res.borrow_mut().push(ra);
                    let oks = res.borrow().iter().filter(|ok| **ok).count();
                    assert_eq!(oks, 1, "{name}, B before A's op {k}, C before B's op {j}");
                }
            }
        }
    }

    // §2.1 "Closing", with the hazard: the close reads its lock as absent
    // and leaves it. Others are then `Busy` naming this process; its own
    // next exclusive open takes the lock; while it holds it again, a third
    // open of this process is `Busy`.
    #[test]
    fn a_close_that_reads_its_lock_as_absent() {
        let w = World::default();
        w.lie.set(1);
        let a = Proc::new(&w, A);
        a.take(false).unwrap();
        assert!(!a.release(), "the close read the lock as absent");
        assert!(holds(&w, A), "the lock is left");
        assert!(is_busy(&Proc::new(&w, B).take(false), A));
        // The re-open of this process (no longer registered as holding it).
        w.lie.set(0);
        let again = Proc::new(&w, A);
        assert!(again.take(false).is_ok());
        assert!(holds(&w, A));
        // Registered again: a second open of this process is `Busy`.
        assert!(is_busy(&Proc::new(&w, A).take(true), A));
        assert!(is_busy(&Proc::new(&w, B).take(false), A));
        assert!(again.release());
        assert!(Proc::new(&w, B).take(false).is_ok());
    }

    // A live pid's lock, or a live contender's claim, is `Busy`, and a lock
    // that is not Slip's is an error left alone.
    #[test]
    fn busy_and_foreign_locks() {
        let w = World::default();
        w.put(world::LOCK, format!("{B}\n").as_bytes());
        assert!(is_busy(&Proc::new(&w, A).take(false), B));
        let w = World::default();
        w.put(&format!("{}/{C}.1.0.claim", world::TMP), b"");
        assert!(is_busy(&Proc::new(&w, A).take(false), C));
        assert_eq!(w.lock(), None);
        let w = World::default();
        w.put(world::LOCK, b"not a lock");
        let r = Proc::new(&w, A).take(false);
        assert!(
            matches!(&r, Err(StoreError::Io { error, .. })
            if error.kind() == io::ErrorKind::InvalidData),
            "{r:?}"
        );
        assert_eq!(w.lock().as_deref(), Some(&b"not a lock"[..]));
        // No claim of A is left in any of these.
        assert!(w.files.borrow().keys().all(|p| !p.to_string_lossy().ends_with(".claim")));
    }
}
