//! The Kiln-free crates' traits over a [`Store`] and the file system
//! (`spec/edits.md` §5.7, §7.4, §10.4), and the round a stage command
//! works on: its request and the observers its map phase selected.

use std::collections::HashMap;
use std::fs;
use std::io::{self, Write as _};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use slip_exec::store::{Object, Store, StoreError};
use slip_model::transform::Transform;
use slip_model::{Digest, Phase, Plan, RunRecord, Shard};

use super::{CommandError, read_checked, store_error};

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

/// The sequence of a process's temporary journals.
static JOURNAL_SEQ: AtomicU64 = AtomicU64::new(0);

impl slip_apply::JournalStore for StoreJournal<'_> {
    /// Atomic, durable and exclusive (`spec/landed/edits/WP-063.md` §6):
    /// a temporary file in this process's scratch directory, named
    /// `journal.<t>.<n>.tmp` with [`slip_exec::store::process_token`], written and
    /// synced, hard-linked to the journal's name (which fails with
    /// `AlreadyExists` if the name exists), the temporary removed and the
    /// store's directory synced.
    fn publish(&self, bytes: &[u8]) -> io::Result<()> {
        let scratch = self.0.scratch_dir().map_err(to_io)?;
        let n = JOURNAL_SEQ.fetch_add(1, Ordering::SeqCst);
        // Never made again by a later process with this pid (whose scratch
        // directory has the same name) while the journal, the temporary's
        // old file, is alive (`spec/harness.md` §11).
        let token = slip_exec::store::process_token();
        let tmp = scratch.join(format!("journal.{token}.{n}.tmp"));
        let written = (|| {
            let mut f = fs::OpenOptions::new().write(true).create_new(true).open(&tmp)?;
            f.write_all(bytes)?;
            f.sync_all()?;
            fs::hard_link(&tmp, self.0.journal_path())
        })();
        // The temporary is in the scratch directory, which the store
        // removes when it is dropped: a failure to remove it here leaves
        // nothing behind, and must not report a journal that is published
        // as not published.
        let _ = fs::remove_file(&tmp);
        written?;
        sync_dir(self.0.root())
    }

    fn read(&self) -> io::Result<Option<Vec<u8>>> {
        match fs::read(self.0.journal_path()) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Removes the journal and syncs the store's directory.
    fn remove(&self) -> io::Result<()> {
        match fs::remove_file(self.0.journal_path()) {
            Ok(()) => sync_dir(self.0.root()),
            Err(e) => Err(e),
        }
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
}

fn to_io(e: StoreError) -> io::Error {
    match e {
        StoreError::Io { error, .. } => error,
        e => io::Error::other(e.to_string()),
    }
}

fn sync_dir(dir: &Path) -> io::Result<()> {
    fs::File::open(dir)?.sync_all()
}

/// The request a round's map shards name (`spec/edits.md` §10.2), read
/// from the store and checked against its id: the `transform` of the
/// first `ok` shard, in the order of `shards`, that has `edits`. `Input`
/// error if the round is not a transform's (`Store::is_transform_round`),
/// if a shard read cannot be decoded, if no shard has `edits`, or if the
/// request is missing or is not the file its id names. Whether every
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
    let transform =
        read_checked(store, Object::Transform, &id, "the request", Transform::from_bytes)?;
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

    // The journal: a temporary in the scratch directory, linked to
    // `write.journal`, unlinked. Beside the names an earlier process with
    // this pid made there (`journal.<n>.tmp`), the publish succeeds and
    // leaves them alone; the journal is then its file's one name, and the
    // second publish is refused while it exists. Kills "a temporary name
    // reused".
    #[test]
    fn the_journal_temporary_is_never_made_again() {
        let d = std::env::temp_dir().join(format!("slip-cli-journal-unit-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        let store = Store::open(&d, Access::Exclusive).unwrap();
        let scratch = store.scratch_dir().unwrap();
        let next = JOURNAL_SEQ.load(Ordering::SeqCst);
        for n in next..next + 8 {
            fs::write(scratch.join(format!("journal.{n}.tmp")), "earlier\n").unwrap();
        }
        let j = StoreJournal(&store);
        j.publish(b"journal one").unwrap();
        assert_eq!(j.read().unwrap().as_deref(), Some(&b"journal one"[..]));
        assert_eq!(fs::metadata(store.journal_path()).unwrap().nlink(), 1);
        let err = j.publish(b"journal two").unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(j.read().unwrap().as_deref(), Some(&b"journal one"[..]));
        for n in next..next + 8 {
            let p = scratch.join(format!("journal.{n}.tmp"));
            assert_eq!(fs::read_to_string(&p).unwrap(), "earlier\n");
        }
        j.remove().unwrap();
        drop(store);
        let _ = fs::remove_dir_all(&d);
    }
}
