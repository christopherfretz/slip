//! The store (`spec/exec.md` §2). Work package WP-008.
//!
//! A directory holding plans, shards, run records and worker logs:
//!
//! ```text
//! <store>/
//!   VERSION                                   the text "1\n"
//!   lock                                      "<pid>\n" while a phase runs (exclusive open)
//!   plans/<plan id>.plan
//!   shards/<plan id>/<phase>.<round>/<observer id>.shard
//!   runs/<plan id>/<phase>.<round>.<started_unix_ns>.run
//!   logs/<plan id>/<phase>.<round>/<observer id>.<attempt>.log
//!   tmp/<pid>.<n>.tmp                         files being written by process <pid>
//!   tmp/<pid>.d/                              scratch directory of process <pid>
//! ```
//!
//! A transform's run (M2, `spec/edits.md` §10.4) adds `transforms/`,
//! `edits/`, `overlays/`, `results/` ([`Object`]), `blobs/`, the transform
//! run records `runs/<plan id>/transform.<round>.<started_unix_ns>.run` and
//! `write.journal`. Their directories are made by the first publish into
//! each, so a store initialised before them is a valid store. Objects and
//! blobs are named by their content: publishing one that is already there
//! leaves the file untouched. An exclusive [`Store::open`] of a store that
//! holds `write.journal` is refused ([`StoreError::UnfinishedWrite`]);
//! [`Store::open_for_recovery`] is the one opener that accepts it.
//!
//! Every file is published by writing it under `tmp/` and renaming it to its
//! final name, so a reader never sees a partial file. An exclusive [`Store`]
//! holds `lock`; a shared one only reads and publishes. A `Store` removes
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
use slip_model::hash::content_digest;
use slip_model::wire::Decoder;
use slip_model::{Digest, Phase, Plan, RunRecord, Shard, formats};

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

/// What a `Store` and its signal cleanup share.
#[derive(Debug)]
struct Inner {
    root: PathBuf,
    /// The lock path, when this store holds it.
    lock: Option<PathBuf>,
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
        if let Some(lock) = &self.lock {
            // Only our own lock: never one another process has taken over.
            if fs::read(lock).ok().as_deref() == Some(lock_text().as_bytes()) {
                let _ = fs::remove_file(lock);
            }
        }
    }

    /// Writes `bytes` to a new `tmp/` file of this store and returns its
    /// path; the file is registered so that it is removed on every exit path.
    fn write_tmp(&self, bytes: &[u8]) -> Result<PathBuf> {
        let dir = self.tmp_dir();
        let n = TMP_SEQ.fetch_add(1, Ordering::SeqCst);
        let path = dir.join(format!("{}.{n}.tmp", std::process::id()));
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

/// The contents of a lock held by this process.
fn lock_text() -> String {
    format!("{}\n", std::process::id())
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

    /// Creates `<lock>` naming this process, or takes over a lock whose
    /// owner is gone.
    fn take_lock(&self) -> Result<PathBuf> {
        let lock = self.root().join("lock");
        // A few rounds: each lost race against another opener retries.
        for _ in 0..8 {
            // Written in full under tmp/ and linked into place, so the lock
            // never exists without its process id.
            let tmp = self.inner.write_tmp(lock_text().as_bytes())?;
            let linked = fs::hard_link(&tmp, &lock);
            self.inner.forget_tmp(&tmp, true);
            match linked {
                Ok(()) => return Ok(lock),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(StoreError::Io { path: lock, error: e }),
            }
            let owner = match fs::read(&lock) {
                Ok(text) => parse_lock(&lock, &text)?,
                Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
                Err(e) => return Err(StoreError::Io { path: lock, error: e }),
            };
            if alive(owner) {
                return Err(StoreError::Busy { pid: owner, lock });
            }
            // Take over: move the stale lock aside, then check that what we
            // moved is the stale lock and not one a racing opener just made.
            let n = TMP_SEQ.fetch_add(1, Ordering::SeqCst);
            let aside = self.inner.tmp_dir().join(format!("{}.{n}.tmp", std::process::id()));
            match fs::rename(&lock, &aside) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
                Err(e) => return Err(StoreError::Io { path: lock, error: e }),
            }
            let moved = fs::read(&aside);
            let moved_owner = moved.as_ref().ok().and_then(|t| parse_lock(&lock, t).ok());
            if moved_owner == Some(owner) {
                let _ = fs::remove_file(&aside);
                continue;
            }
            // Someone else's fresh lock: put it back and report it.
            let _ = fs::hard_link(&aside, &lock);
            let _ = fs::remove_file(&aside);
            return Err(StoreError::Busy { pid: moved_owner.unwrap_or(0), lock });
        }
        let pid = fs::read(&lock).ok().and_then(|t| parse_lock(&lock, &t).ok()).unwrap_or(0);
        Err(StoreError::Busy { pid, lock })
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

    /// Publishes a container of `object`'s kind under its content id, which
    /// is returned. `Invalid` (nothing written) if `bytes` is not a
    /// container of that kind (`spec/model.md` §5: magic, kind, version 1,
    /// trailer, section framing; the sections are not decoded). Content
    /// that is already there is left untouched.
    pub fn publish_object(&self, object: Object, bytes: &[u8]) -> Result<Digest> {
        let id = container_id(bytes, object.kind())?;
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
    /// (`spec/model.md` §6), which is returned. A blob that is already
    /// there is left untouched.
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

    /// The least round for which `plan` has neither `map.<r>` nor
    /// `verify.<r>` among its shard directories and no
    /// `transform.<r>.*.run` among its run records (`spec/edits.md` §8,
    /// `spec/exec.md` §2). Names whose round is not canonical decimal are
    /// not rounds.
    pub fn next_round(&self, plan: &Digest) -> Result<u64> {
        let mut taken = HashSet::new();
        let shards = self.root().join("shards").join(plan.to_string());
        for name in list_names(&shards)? {
            let round = [Phase::Map, Phase::Verify].into_iter().find_map(|phase| {
                name.strip_prefix(phase_name(phase))?.strip_prefix('.').and_then(decimal)
            });
            if let Some(r) = round {
                taken.insert(r);
            }
        }
        let runs = self.root().join("runs").join(plan.to_string());
        for name in list_names(&runs)? {
            if let Some((r, _)) = parse_transform_run_name(&name) {
                taken.insert(r);
            }
        }
        let mut round = 0;
        while taken.contains(&round) {
            round += 1;
        }
        Ok(round)
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
    /// The bytes must be a `RUNT` container whose section `run` names
    /// `plan`, `round` and `started_unix_ns`; only those leading fields of
    /// the record are read. Anything else is `Invalid`.
    pub fn publish_transform_run(
        &self,
        plan: &Digest,
        round: u64,
        started_unix_ns: u64,
        bytes: &[u8],
    ) -> Result<PathBuf> {
        let (p, r, ns) = transform_run_names(bytes)
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

    /// Publishes content-addressed `bytes` at `dest` unless a file is
    /// already there, which is then left untouched.
    fn publish_once(&self, dest: &Path, bytes: &[u8]) -> Result<()> {
        match fs::symlink_metadata(dest) {
            Ok(_) => Ok(()),
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
/// `hex_prefix`, ascending (§2.1 "find_plan").
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
}

/// The version of every container of a transform's run (`spec/edits.md`
/// §3, §6, §7.1, §8.5, §10.6).
const OBJECT_VERSION: u32 = 1;

/// The first component of a transform run record's name.
const TRANSFORM: &str = "transform";

/// The kind of a transform run record.
const RUNT: [u8; 4] = *b"RUNT";

/// The content id of a container of `kind`, version 1, checked as
/// `spec/model.md` §5 rule 5 checks the frame (the sections are not
/// decoded); `Invalid` otherwise.
fn container_id(bytes: &[u8], kind: [u8; 4]) -> Result<Digest> {
    let name = String::from_utf8_lossy(&kind).into_owned();
    Reader::open(bytes, kind, OBJECT_VERSION)
        .map(|r| r.content_id())
        .map_err(|e| StoreError::Invalid(format!("not a valid {name} container: {e}")))
}

/// `plan`, `round` and `started_unix_ns` of a `RUNT` container
/// (`spec/edits.md` §10.6): the frame is checked and section `run` (id 1)
/// decoded as far as `started_unix_ns`.
fn transform_run_names(
    bytes: &[u8],
) -> std::result::Result<(Digest, u64, u64), slip_model::wire::Error> {
    use slip_model::wire::{Error, ErrorKind};
    let reader = Reader::open(bytes, RUNT, OBJECT_VERSION)?;
    let Some(mut d) = reader.decoder(1) else {
        return Err(Error::new(ErrorKind::Malformed, 0, "no section run"));
    };
    d.str()?; // tool_slip
    d.str()?; // tool_kiln
    let plan = d.digest()?;
    let round = d.uv()?;
    d.digest()?; // transform
    for _ in 0..4 {
        // edits, declared, overlay, result: opt<digest>
        skip_opt_digest(&mut d)?;
    }
    let started = d.uv()?;
    Ok((plan, round, started))
}

fn skip_opt_digest(d: &mut Decoder<'_>) -> std::result::Result<(), slip_model::wire::Error> {
    use slip_model::wire::{Error, ErrorKind};
    let at = d.offset();
    match d.u8()? {
        0 => Ok(()),
        1 => d.digest().map(|_| ()),
        _ => Err(Error::new(ErrorKind::Malformed, at, "opt tag is not 0 or 1")),
    }
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

fn parse_lock(lock: &Path, text: &[u8]) -> Result<u32> {
    std::str::from_utf8(text)
        .ok()
        .and_then(|t| t.strip_suffix('\n'))
        .and_then(decimal)
        .and_then(|p| u32::try_from(p).ok())
        .ok_or_else(|| StoreError::Io {
            path: lock.to_path_buf(),
            error: io::Error::new(
                io::ErrorKind::InvalidData,
                "the lock file does not hold a process id; remove it if no Slip runs here",
            ),
        })
}

/// `<phase>.<round>.<started_unix_ns>.run` for `phase`: (round, started).
fn parse_run_name(name: &str, phase: Phase) -> Option<(u64, u64)> {
    let rest = name.strip_suffix(".run")?.strip_prefix(phase_name(phase))?.strip_prefix('.')?;
    let (round, started) = rest.split_once('.')?;
    Some((decimal(round)?, decimal(started)?))
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
    for sub in ["tmp", "plans", "shards", "runs", "logs"] {
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
