//! The Kiln-free crates' traits over a [`Store`] and the file system
//! (`spec/edits.md` §5.7, §7.4, §10.4), and the round a stage command
//! works on: its request and the observers its map phase selected.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, PoisonError};

use slip_exec::store::{Store, StoreError};
use slip_model::transform::Transform;
use slip_model::{Digest, Observer, Phase, Plan, RunRecord, Shard};

use super::{CommandError, read_request, store_error};

/// The map shards of one round, over the observers its map phase selected
/// (`selected_of_round`), in plan order: position `i` is the `i`-th
/// selected observer. An observer the phase did not select is not one of
/// the run's, so it is neither read nor counted as unparsed.
pub struct StoreShards<'a> {
    store: &'a Store,
    plan: Digest,
    round: u64,
    observers: Vec<Digest>,
}

impl<'a> StoreShards<'a> {
    /// The shards of `round` of `plan` (id `plan_id`) for the observers at
    /// `selected`, indices of `plan.observers`, ascending.
    ///
    /// # Panics
    ///
    /// If an index of `selected` is not one of `plan.observers` (the
    /// caller's invariant: `selected_of_round` makes them).
    pub fn new(
        store: &'a Store,
        plan_id: Digest,
        plan: &Plan,
        round: u64,
        selected: &[usize],
    ) -> StoreShards<'a> {
        let observers = selected.iter().map(|&i| plan.observers[i].id).collect();
        StoreShards { store, plan: plan_id, round, observers }
    }

    /// The path of the shard at position `index`.
    pub fn path(&self, index: usize) -> std::path::PathBuf {
        self.store.shard_path(&self.plan, Phase::Map, self.round, &self.observers[index])
    }
}

impl slip_reduce::Shards for StoreShards<'_> {
    fn open(&self, index: usize) -> Result<Option<Vec<u8>>, String> {
        self.store
            .read_shard(&self.plan, Phase::Map, self.round, &self.observers[index])
            .map_err(|e| e.to_string())
    }

    fn len(&self) -> usize {
        self.observers.len()
    }

    fn observer(&self, index: usize) -> Digest {
        self.observers[index]
    }
}

/// Blobs in the store's `blobs/` (`spec/edits.md` §10.4).
pub struct StoreBlobs<'a>(pub &'a Store);

impl slip_apply::Blobs for StoreBlobs<'_> {
    /// `NotFound` for a blob the store does not have, `InvalidData` for
    /// one that does not hash to its name.
    fn get(&self, id: Digest) -> io::Result<Vec<u8>> {
        match self.0.read_blob(&id) {
            Ok(Some(bytes)) => Ok(bytes),
            Ok(None) => Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("{}: no such blob", self.0.blob_path(&id).display()),
            )),
            Err(StoreError::Io { error, .. }) => Err(error),
            Err(e) => Err(io::Error::new(io::ErrorKind::InvalidData, e.to_string())),
        }
    }

    /// Publishes the bytes, then syncs the blob and `blobs/` (durable when
    /// it returns, `spec/landed/edits/WP-063.md` §6).
    fn put(&self, bytes: &[u8]) -> io::Result<Digest> {
        let id = self.0.publish_blob(bytes).map_err(to_io)?;
        let path = self.0.blob_path(&id);
        fs::File::open(&path)?.sync_all()?;
        if let Some(dir) = path.parent() {
            sync_dir(dir)?;
        }
        Ok(id)
    }
}

/// The store's one journal, `<store>/write.journal` (§7.3).
pub struct StoreJournal<'a>(pub &'a Store);

impl std::fmt::Debug for StoreShards<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoreShards")
            .field("store", &self.store.root())
            .field("plan", &self.plan)
            .field("round", &self.round)
            .field("observers", &self.observers)
            .finish()
    }
}

impl std::fmt::Debug for StoreBlobs<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("StoreBlobs").field(&self.0.root()).finish()
    }
}

impl std::fmt::Debug for StoreJournal<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("StoreJournal").field(&self.0.root()).finish()
    }
}

impl slip_apply::JournalStore for StoreJournal<'_> {
    /// Atomic for its reader (`spec/edits.md` §7.3 step 2: a journal cut
    /// short is told from a whole one), durable and exclusive: created in
    /// place by [`Store::publish_journal`], which fails with
    /// `AlreadyExists` if a journal exists.
    fn publish(&self, bytes: &[u8]) -> io::Result<()> {
        self.0.publish_journal(bytes).map_err(to_io)
    }

    fn read(&self) -> io::Result<Option<Vec<u8>>> {
        self.0.read_journal().map_err(to_io)
    }

    /// Removes the journal and syncs the store's directory.
    fn remove(&self) -> io::Result<()> {
        self.0.remove_journal().map_err(to_io)
    }
}

/// The pre-image: the file system.
#[derive(Debug, Clone, Copy, Default)]
pub struct DiskFiles;

impl slip_apply::Files for DiskFiles {
    fn read(&self, path: &[u8]) -> io::Result<Vec<u8>> {
        use std::os::unix::ffi::OsStrExt;
        fs::read(std::ffi::OsStr::from_bytes(path))
    }

    /// `stat`'s device and inode; `None` when nothing is there.
    fn identity(&self, path: &[u8]) -> io::Result<Option<(u64, u64)>> {
        use std::os::unix::ffi::OsStrExt;
        use std::os::unix::fs::MetadataExt;
        match fs::metadata(std::ffi::OsStr::from_bytes(path)) {
            Ok(m) => Ok(Some((m.dev(), m.ino()))),
            Err(e)
                if matches!(e.kind(), io::ErrorKind::NotFound | io::ErrorKind::NotADirectory) =>
            {
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }
}

fn sync_dir(dir: &Path) -> io::Result<()> {
    fs::File::open(dir)?.sync_all()
}

fn to_io(e: StoreError) -> io::Error {
    match e {
        StoreError::Io { error, .. } => error,
        e => io::Error::other(e.to_string()),
    }
}

/// The request a round's map shards name (`spec/edits.md` §10.2), read
/// from the store and checked against its id: the `transform` of the
/// first `ok` shard, in the order of `shards`, that has `edits`. `Input`
/// error if the round is not a transform's (`Store::is_transform_round`),
/// if a shard read cannot be decoded, if no shard has `edits`, if the
/// request is missing or is not the file its id names, or if an entry of
/// its ownership does not exist or is not spelled on disk
/// ([`read_request`]). Whether every
/// shard names this request is reduce's to check (§5.7).
pub fn request_of_round(
    store: &Store,
    plan_id: Digest,
    round: u64,
    shards: &StoreShards<'_>,
) -> Result<(Digest, Transform), CommandError> {
    use slip_reduce::Shards as _;
    if !store.is_transform_round(&plan_id, round).map_err(|e| store_error(store, &e))? {
        return Err(CommandError::Input(format!(
            "plan {plan_id} round {round}: not a transform's round (it has no transform run \
             record; round 0 is `parse`'s)"
        )));
    }
    let mut request = None;
    for i in 0..shards.len() {
        let Some(bytes) = shards.open(i).map_err(CommandError::Input)? else { continue };
        let shard = Shard::from_bytes(&bytes)
            .map_err(|e| CommandError::Input(format!("{}: {e}", shards.path(i).display())))?;
        if let Some(edits) = &shard.edits {
            request = Some(edits.transform);
            break;
        }
    }
    let Some(id) = request else {
        return Err(CommandError::Input(format!(
            "plan {plan_id} round {round}: no map shard of the round has an `edits` section"
        )));
    };
    let transform = read_request(store, &id)?;
    Ok((id, transform))
}

/// The observers the round's map phase selected (`spec/exec.md` §2.2):
/// the observers of the jobs of its latest map run record, as indices of
/// `plan.observers`, ascending, each once. `Input` error if the round has
/// no map run record, if the record cannot be decoded or is not of this
/// plan, phase and round, or if it names an observer the plan does not
/// have.
pub fn selected_of_round(
    store: &Store,
    plan_id: Digest,
    plan: &Plan,
    round: u64,
) -> Result<Vec<usize>, CommandError> {
    let bytes = store
        .latest_run_of_round(&plan_id, Phase::Map, round)
        .map_err(|e| store_error(store, &e))?
        .ok_or_else(|| {
            CommandError::Input(format!(
                "plan {plan_id} round {round}: the store {} has no map run record of the round",
                store.root().display()
            ))
        })?;
    let what = format!("plan {plan_id} round {round}: the map run record");
    let record =
        RunRecord::from_bytes(&bytes).map_err(|e| CommandError::Input(format!("{what}: {e}")))?;
    let r = &record.run;
    if r.plan != plan_id || r.phase != Phase::Map || r.round != round {
        return Err(CommandError::Input(format!(
            "{what}: it is of plan {}, round {}, not of this plan and round",
            r.plan, r.round
        )));
    }
    let index: HashMap<Digest, usize> =
        plan.observers.iter().enumerate().map(|(i, o)| (o.id, i)).collect();
    let mut selected = Vec::with_capacity(record.jobs.len());
    for job in &record.jobs {
        match index.get(&job.observer) {
            Some(&i) => selected.push(i),
            None => {
                return Err(CommandError::Input(format!(
                    "{what}: observer {} is not one of the plan's",
                    job.observer
                )));
            }
        }
    }
    selected.sort_unstable();
    selected.dedup();
    Ok(selected)
}

/// `spec/edits.md` §8.2 (WP-076 decision 2, WP-157): the observer's map
/// shard of the round, which every observer the round's map phase
/// selected has, `ok` or not. None, or bytes that are not its shard
/// (`verify_cmd::read_map_shard`), means that the phase did not finish
/// for it (an interrupted `map`: its job cancelled or never run); it is
/// not an unparsed observer, and the error is an input error naming its
/// main file, whatever `--allow-unparsed` says. `check` and `reduce` both
/// refuse the round with it.
pub(crate) fn map_shard_of_round(
    store: &Store,
    plan_id: &Digest,
    round: u64,
    observer: &Observer,
) -> Result<Shard, CommandError> {
    let read = crate::verify_cmd::read_map_shard(store, plan_id, round, &observer.id)
        .map_err(super::command_error)?;
    match read {
        Some(crate::verify_cmd::MapRead::Shard(s, _)) => Ok(*s),
        found => Err(CommandError::Input(format!(
            "{}: {} map shard of round {round}: the round's map phase did not finish for this \
             observer (interrupted?); run `slip map` again",
            lossy(&observer.main.path),
            if found.is_none() { "no" } else { "an unusable" },
        ))),
    }
}

/// WP-157: the round's map phase finished, which is what an edit set
/// promises (`spec/edits.md` §10.2): [`map_shard_of_round`] for every
/// observer at `selected` (indices of `plan.observers`, ascending, from
/// `selected_of_round`), on up to `threads` threads. The error is that of
/// the least selected position that has one, whatever the threads.
pub fn map_phase_finished(
    store: &Store,
    plan_id: Digest,
    plan: &Plan,
    round: u64,
    selected: &[usize],
    threads: usize,
) -> Result<(), CommandError> {
    let next = AtomicUsize::new(0);
    // The least position found with an error, and its error.
    let least: Mutex<Option<(usize, CommandError)>> = Mutex::new(None);
    let work = || {
        loop {
            let k = next.fetch_add(1, Ordering::Relaxed);
            let Some(&i) = selected.get(k) else { return };
            let found = least.lock().unwrap_or_else(PoisonError::into_inner).as_ref().map(|l| l.0);
            if found.is_some_and(|l| l < k) {
                return;
            }
            if let Err(e) = map_shard_of_round(store, &plan_id, round, &plan.observers[i]) {
                let mut l = least.lock().unwrap_or_else(PoisonError::into_inner);
                if l.as_ref().is_none_or(|l| k < l.0) {
                    *l = Some((k, e));
                }
                return;
            }
        }
    };
    std::thread::scope(|s| {
        for _ in 1..threads.clamp(1, selected.len().max(1)) {
            s.spawn(work);
        }
        work();
    });
    match least.into_inner().unwrap_or_else(PoisonError::into_inner) {
        Some((_, e)) => Err(e),
        None => Ok(()),
    }
}

/// The path of `path` (bytes) for a message.
pub(crate) fn lossy(path: &[u8]) -> String {
    String::from_utf8_lossy(path).into_owned()
}

/// `spec/landed/harness/WP-159.md` item 4, the journal's sequence.
#[cfg(test)]
mod tests {
    use super::*;
    use slip_apply::JournalStore as _;
    use slip_exec::store::Access;
    use std::os::unix::fs::MetadataExt as _;

    // The journal is created in place (spec/exec.md §2.1 "The journal"):
    // no temporary in the scratch directory or `tmp/`, one name, the
    // second publish refused while it exists, and the removal of none is
    // `NotFound`.
    #[test]
    fn the_journal_is_created_in_place() {
        let d = std::env::temp_dir().join(format!("slip-cli-journal-unit-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        let store = Store::open(&d, Access::Exclusive).unwrap();
        let j = StoreJournal(&store);
        assert_eq!(j.read().unwrap(), None);
        j.publish(b"journal one").unwrap();
        assert_eq!(j.read().unwrap().as_deref(), Some(&b"journal one"[..]));
        assert_eq!(fs::metadata(store.journal_path()).unwrap().nlink(), 1);
        assert_eq!(fs::read_dir(d.join("tmp")).unwrap().count(), 0, "no temporary");
        let err = j.publish(b"journal two").unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(j.read().unwrap().as_deref(), Some(&b"journal one"[..]));
        j.remove().unwrap();
        assert_eq!(j.remove().unwrap_err().kind(), io::ErrorKind::NotFound);
        drop(store);
        let _ = fs::remove_dir_all(&d);
    }
}
