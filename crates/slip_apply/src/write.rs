//! Writing the tree (`spec/edits.md` §7.3): `write` and `recover`, over
//! traits for the tree, the blobs and the journal so that a failure at
//! every operation is tested. The sequence of operations and what an
//! interruption at each leaves are `spec/landed/edits/WP-063.md` §5.

use std::collections::BTreeSet;
use std::io;

use slip_model::Digest;
use slip_model::formats;
use slip_model::hash::content_digest;
use slip_model::overlay::{Content, Overlay};
use slip_model::transform::{Ownership, is_canonical};

use crate::journal::{Journal, JournalEntry, parent, staged_name};

/// What `write` needs to know of a path (`Tree::stat`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stat {
    /// A regular file, reached at this path without following a symbolic
    /// link: neither the path nor a directory on it is a link.
    pub regular: bool,
    /// Its number of hard links.
    pub links: u64,
    /// Its permission bits (`0o7777`).
    pub mode: u32,
    /// Its directory accepts new names.
    pub dir_writable: bool,
    /// Its owner.
    pub uid: u32,
    /// Its group.
    pub gid: u32,
    /// The device and inode: which file it is.
    pub dev: u64,
    pub ino: u64,
    /// Its size in bytes.
    pub size: u64,
    /// Its modification and change times, as seconds and nanoseconds.
    pub mtime: (i64, i64),
    pub ctime: (i64, i64),
}

impl Stat {
    /// What the re-check before a rename compares (§7.3 step 5): the
    /// same device, size, modification and change time, and the same
    /// inode unless the times have a nanosecond part. Some volumes
    /// (FUSE-backed host mounts) give a file a new inode number on a later
    /// lookup after it, or its directory, was renamed; with times that
    /// fine, a different file of the same size and both times is not a
    /// realistic replacement (the change time cannot be set), so the
    /// inode is then not needed to tell. With coarse times (a zero
    /// nanosecond part) it is the one field that does.
    fn same_file(&self, now: &Stat) -> bool {
        let fine = self.mtime.1 != 0 && self.ctime.1 != 0;
        (self.dev, self.size, self.mtime, self.ctime) == (now.dev, now.size, now.mtime, now.ctime)
            && (self.ino == now.ino || fine)
    }
}

/// The working tree.
pub trait Tree {
    /// `None`: no such file.
    fn stat(&self, path: &[u8]) -> io::Result<Option<Stat>>;
    fn read(&self, path: &[u8]) -> io::Result<Vec<u8>>;
    /// Creates `path`, which must not exist (`AlreadyExists` otherwise;
    /// nothing is followed or truncated), with these permission bits and
    /// bytes, written and synced before it returns.
    fn create(&self, path: &[u8], mode: u32, bytes: &[u8]) -> io::Result<()>;
    /// Renames atomically, replacing `to`.
    fn rename(&self, from: &[u8], to: &[u8]) -> io::Result<()>;
    /// Removes a file; `NotFound` if there is none.
    fn remove(&self, path: &[u8]) -> io::Result<()>;
    /// Makes the names in `dir` durable.
    fn sync_dir(&self, dir: &[u8]) -> io::Result<()>;
}

/// Blobs by content digest (`spec/edits.md` §10.4).
pub trait Blobs {
    fn get(&self, id: Digest) -> io::Result<Vec<u8>>;
    /// Publishes the bytes, durably, and returns their content digest.
    fn put(&self, bytes: &[u8]) -> io::Result<Digest>;
}

/// The store's one journal (`<store>/write.journal`).
pub trait JournalStore {
    /// Publishes atomically, durably and exclusively: fails with
    /// `AlreadyExists`, changing nothing, if a journal exists; after `Ok`,
    /// a reader sees exactly these bytes, also after a crash.
    fn publish(&self, bytes: &[u8]) -> io::Result<()>;
    fn read(&self) -> io::Result<Option<Vec<u8>>>;
    /// Removes it, durably; `Ok` or `NotFound` when there is none.
    fn remove(&self) -> io::Result<()>;
}

/// Why `write` refused before replacing anything (§7.3 steps 1, 4 and the
/// re-check before each rename), or `recover` refused to start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refused {
    /// The file is not owned (§2.1).
    NotOwned(Vec<u8>),
    /// It has more than one hard link.
    HardLinked(Vec<u8>),
    /// Its directory is not writable.
    NotWritable(Vec<u8>),
    /// It is missing, or its bytes are not the `pre` image; or (step 4) a
    /// staged file, named here, is not what step 3 wrote.
    StaleFile(Vec<u8>),
    /// It is not a regular file at its real path: a symbolic link, a
    /// directory, a device, or reached through a linked directory.
    NotRegular(Vec<u8>),
    /// Something has its staged name (step 1); the path named is the
    /// staged one.
    StagedExists(Vec<u8>),
    /// Its owner's write bit is clear: its owner marked it read-only.
    ReadOnly(Vec<u8>),
    /// The staged file's owner or group is not the file's: the rename
    /// would change them.
    OwnerChanged(Vec<u8>),
    /// `recover`: the journal names a path the ownership does not own.
    /// Nothing was changed and the journal is kept.
    JournalNotOwned(Vec<u8>),
}

/// What `write` and `recover` fail with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WriteError {
    Refused(Refused),
    /// An operation failed. `journal_left`: a journal may remain, and
    /// `recover` must run; false only when `write` undid all it did.
    Io {
        path: Vec<u8>,
        message: String,
        journal_left: bool,
    },
    /// The journal is malformed, or (`write`) a journal is already there.
    Journal(String),
    /// The overlay is malformed or is not the overlay its id names, or the
    /// ownership is malformed.
    Invalid(String),
}

/// What `recover` did, by path: restored to the pre-image, left (already
/// the pre-image), reported (neither image; left, and the journal kept).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Recovered {
    pub restored: Vec<Vec<u8>>,
    pub left: Vec<Vec<u8>>,
    pub reported: Vec<Vec<u8>>,
}

/// The path `Io` errors of the journal name.
pub const JOURNAL_PATH: &[u8] = b"write.journal";

const UNFINISHED: &str = "the store has an unfinished write";

fn is_image(bytes: &[u8], c: &Content) -> bool {
    bytes.len() as u64 == c.size && content_digest(bytes) == c.content
}

/// `Invalid` unless every entry is a canonical absolute path (so not
/// empty: `""` would cover every path), `owned` is not empty, and no entry
/// is in both lists (`spec/edits.md` §2.1).
fn check_ownership(o: &Ownership) -> Result<(), WriteError> {
    let bad = |what: String| Err(WriteError::Invalid(format!("ownership: {what}")));
    if o.owned.is_empty() {
        return bad("owned is empty".to_string());
    }
    for (list, name) in [(&o.owned, "owned"), (&o.readonly, "readonly")] {
        if let Some(e) = list.iter().find(|e| !is_canonical(e)) {
            return bad(format!(
                "{name}: {:?} is not a canonical absolute path",
                String::from_utf8_lossy(e)
            ));
        }
    }
    if let Some(e) = o.readonly.iter().find(|e| o.owned.contains(e)) {
        return bad(format!("{:?} is both owned and readonly", String::from_utf8_lossy(e)));
    }
    Ok(())
}

/// A failure inside a step: a refusal, or an operation that failed.
enum Fail {
    Refused(Refused),
    Io(Vec<u8>, io::Error),
}

impl From<Refused> for Fail {
    fn from(r: Refused) -> Fail {
        Fail::Refused(r)
    }
}

fn io_at(path: &[u8]) -> impl FnOnce(io::Error) -> Fail + '_ {
    move |e| Fail::Io(path.to_vec(), e)
}

fn io_error(path: &[u8], e: &io::Error, journal_left: bool) -> WriteError {
    WriteError::Io { path: path.to_vec(), message: e.to_string(), journal_left }
}

/// What the file at `path` must be, wherever it is checked: present, a
/// regular file at its real path, one link, its owner's write bit set.
fn file_ok(path: &[u8], st: Option<Stat>) -> Result<Stat, Refused> {
    let Some(st) = st else {
        return Err(Refused::StaleFile(path.to_vec()));
    };
    if !st.regular {
        return Err(Refused::NotRegular(path.to_vec()));
    }
    if st.links != 1 {
        return Err(Refused::HardLinked(path.to_vec()));
    }
    if st.mode & 0o200 == 0 {
        return Err(Refused::ReadOnly(path.to_vec()));
    }
    Ok(st)
}

/// The staged file is the one step 3 wrote, as far as `stat` shows: a
/// regular file of one link.
fn staged_ok(f: &JournalEntry, st: Option<Stat>) -> Result<Stat, Refused> {
    match st {
        Some(s) if s.regular && s.links == 1 => Ok(s),
        _ => Err(Refused::StaleFile(f.staged.clone())),
    }
}

/// Step 1 (`again` false) or step 4 (`again` true) for one file. Returns
/// what `stat` found of the file and (step 4) of its staged file.
fn check(
    f: &JournalEntry,
    ownership: &Ownership,
    tree: &dyn Tree,
    again: bool,
) -> Result<(Stat, Option<Stat>), Fail> {
    let path = &f.path;
    if !ownership.is_owned(path) {
        return Err(Refused::NotOwned(path.clone()).into());
    }
    let st = file_ok(path, tree.stat(path).map_err(io_at(path))?)?;
    if !st.dir_writable {
        return Err(Refused::NotWritable(path.clone()).into());
    }
    let staged = tree.stat(&f.staged).map_err(io_at(&f.staged))?;
    let mut seen = None;
    if again {
        let s = staged_ok(f, staged)?;
        seen = Some(s);
        if !is_image(&tree.read(&f.staged).map_err(io_at(&f.staged))?, &f.post) {
            return Err(Refused::StaleFile(f.staged.clone()).into());
        }
        if (s.uid, s.gid) != (st.uid, st.gid) {
            return Err(Refused::OwnerChanged(path.clone()).into());
        }
    } else if staged.is_some() {
        return Err(Refused::StagedExists(f.staged.clone()).into());
    }
    if !is_image(&tree.read(path).map_err(io_at(path))?, &f.pre) {
        return Err(Refused::StaleFile(path.clone()).into());
    }
    Ok((st, seen))
}

/// The re-check just before a rename (step 5): the file and its staged
/// file are still what step 4 found, as `stat` (`lstat`, and the
/// directory's real path) shows: the same kind, links and owner write bit,
/// and the same device, inode, size, modification and change time. A
/// difference is `StaleFile` naming the file (or the staged file). No
/// bytes are read.
fn recheck(f: &JournalEntry, seen: &(Stat, Stat), tree: &dyn Tree) -> Result<(), Fail> {
    let now = file_ok(&f.path, tree.stat(&f.path).map_err(io_at(&f.path))?)?;
    if !seen.0.same_file(&now) {
        return Err(Refused::StaleFile(f.path.clone()).into());
    }
    let now = staged_ok(f, tree.stat(&f.staged).map_err(io_at(&f.staged))?)?;
    if !seen.1.same_file(&now) {
        return Err(Refused::StaleFile(f.staged.clone()).into());
    }
    Ok(())
}

/// The directories of a journal's files, sorted, each once.
fn dirs(j: &Journal) -> BTreeSet<&[u8]> {
    j.files.iter().map(|f| parent(&f.path)).collect()
}

fn remove_journal(journal: &dyn JournalStore) -> io::Result<()> {
    match journal.remove() {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// Undoes steps 2 and 3 (nothing was renamed): removes the staged files of
/// `staged`, syncs their directories, removes the journal. `Err`: what
/// failed; the journal is then left.
fn undo_staged(
    j: &Journal,
    staged: &[bool],
    tree: &dyn Tree,
    journal: &dyn JournalStore,
) -> Result<(), (Vec<u8>, io::Error)> {
    for (f, _) in j.files.iter().zip(staged).filter(|(_, s)| **s) {
        match tree.remove(&f.staged) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => return Err((f.staged.clone(), e)),
            _ => {}
        }
    }
    for d in dirs(j) {
        tree.sync_dir(d).map_err(|e| (d.to_vec(), e))?;
    }
    remove_journal(journal).map_err(|e| (JOURNAL_PATH.to_vec(), e))
}

/// The error of a failure from step 2 on, after `undo` ran: `journal_left`
/// unless the undo finished.
fn after_undo(path: &[u8], e: &io::Error, undo: Result<(), (Vec<u8>, io::Error)>) -> WriteError {
    match undo {
        Ok(()) => io_error(path, e, false),
        Err((up, ue)) => WriteError::Io {
            path: path.to_vec(),
            message: format!("{e}; undoing it failed at {}: {ue}", String::from_utf8_lossy(&up)),
            journal_left: true,
        },
    }
}

/// §7.3 steps 1 to 5 over the overlay's files whose `post` differs from
/// `pre`. Returns the number of files replaced.
pub fn write(
    overlay: &Overlay,
    overlay_id: Digest,
    ownership: &Ownership,
    tree: &dyn Tree,
    blobs: &dyn Blobs,
    journal: &dyn JournalStore,
) -> Result<u64, WriteError> {
    overlay.validate().map_err(WriteError::Invalid)?;
    check_ownership(ownership)?;
    if formats::content_id(&overlay.to_bytes()).ok() != Some(overlay_id) {
        return Err(WriteError::Invalid(
            "the overlay id is not the content id of the overlay".to_string(),
        ));
    }
    let j = Journal {
        overlay: overlay_id,
        files: overlay
            .files
            .iter()
            .filter(|f| f.pre != f.post)
            .map(|f| JournalEntry {
                path: f.path.clone(),
                staged: staged_name(&f.path, overlay_id),
                pre: f.pre,
                post: f.post,
            })
            .collect(),
    };
    if j.files.is_empty() {
        return Ok(0);
    }
    match journal.read() {
        Ok(None) => {}
        Ok(Some(_)) => return Err(WriteError::Journal(UNFINISHED.to_string())),
        Err(e) => return Err(io_error(JOURNAL_PATH, &e, false)),
    }
    let fail = |f: Fail| match f {
        Fail::Refused(r) => WriteError::Refused(r),
        Fail::Io(p, e) => io_error(&p, &e, false),
    };

    // 1. Check.
    let mut modes = Vec::with_capacity(j.files.len());
    for f in &j.files {
        modes.push(check(f, ownership, tree, false).map_err(fail)?.0.mode);
    }

    // 2. Journal: the pre-images as blobs, then the journal.
    for f in &j.files {
        let bytes = tree.read(&f.path).map_err(|e| io_error(&f.path, &e, false))?;
        if !is_image(&bytes, &f.pre) {
            return Err(WriteError::Refused(Refused::StaleFile(f.path.clone())));
        }
        match blobs.put(&bytes) {
            Ok(id) if id == f.pre.content => {}
            Ok(_) => {
                let e = io::Error::other("the blob store named the pre-image by another digest");
                return Err(io_error(&f.path, &e, false));
            }
            Err(e) => return Err(io_error(&f.path, &e, false)),
        }
    }
    match journal.publish(&j.to_bytes()) {
        Ok(()) => {}
        // Another write's journal: not ours to remove.
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
            return Err(WriteError::Journal(UNFINISHED.to_string()));
        }
        Err(e) => {
            // It may have been published all the same.
            let left = remove_journal(journal).is_err();
            return Err(io_error(JOURNAL_PATH, &e, left));
        }
    }

    // 3. Stage.
    let mut staged = vec![false; j.files.len()];
    for (i, f) in j.files.iter().enumerate() {
        let post = match blobs.get(f.post.content) {
            Ok(b) if is_image(&b, &f.post) => b,
            got => {
                let e = got.err().unwrap_or_else(|| {
                    io::Error::other("the post-image blob does not hash to its name")
                });
                return Err(after_undo(&f.path, &e, undo_staged(&j, &staged, tree, journal)));
            }
        };
        if let Err(e) = tree.create(&f.staged, modes[i], &post) {
            // A create that failed because the name exists created nothing
            // of ours; any other may have left part of a file.
            staged[i] = e.kind() != io::ErrorKind::AlreadyExists;
            return Err(after_undo(&f.staged, &e, undo_staged(&j, &staged, tree, journal)));
        }
        staged[i] = true;
    }

    // 4. Check again.
    let mut seen = Vec::with_capacity(j.files.len());
    for f in &j.files {
        match check(f, ownership, tree, true) {
            Ok((file, Some(staged))) => seen.push((file, staged)),
            Ok((_, None)) => unreachable!("step 4 stats the staged file"),
            Err(fl) => {
                let undo = undo_staged(&j, &staged, tree, journal);
                return Err(match (fl, undo) {
                    (Fail::Refused(r), Ok(())) => WriteError::Refused(r),
                    (Fail::Io(p, e), undo) => after_undo(&p, &e, undo),
                    (Fail::Refused(r), Err((up, ue))) => WriteError::Io {
                        path: up,
                        message: format!("refused ({r:?}); undoing the write failed: {ue}"),
                        journal_left: true,
                    },
                });
            }
        }
    }

    // 5. Commit.
    commit(&j, &seen, tree, blobs, journal)
}

/// Step 5: each rename just after a re-check of its two names, the
/// directories synced, the journal removed.
fn commit(
    j: &Journal,
    seen: &[(Stat, Stat)],
    tree: &dyn Tree,
    blobs: &dyn Blobs,
    journal: &dyn JournalStore,
) -> Result<u64, WriteError> {
    for (f, seen) in j.files.iter().zip(seen) {
        let step = recheck(f, seen, tree).and_then(|()| {
            tree.rename(&f.staged, &f.path).map_err(|e| Fail::Io(f.path.clone(), e))
        });
        match step {
            Ok(()) => {}
            Err(Fail::Io(p, e)) => return Err(rolled_back(&p, &e, j, tree, blobs, journal)),
            Err(Fail::Refused(r)) => {
                let e = io::Error::other(format!("refused before its rename: {r:?}"));
                return Err(match rolled_back(&f.path, &e, j, tree, blobs, journal) {
                    WriteError::Io { journal_left: false, .. } => WriteError::Refused(r),
                    other => other,
                });
            }
        }
    }
    for d in dirs(j) {
        if let Err(e) = tree.sync_dir(d) {
            return Err(rolled_back(d, &e, j, tree, blobs, journal));
        }
    }
    remove_journal(journal).map_err(|e| io_error(JOURNAL_PATH, &e, true))?;
    Ok(j.files.len() as u64)
}

/// A failure in step 5: the journal is published and not removed, so the
/// tree is returned to the pre-image as `recover` does.
fn rolled_back(
    path: &[u8],
    e: &io::Error,
    j: &Journal,
    tree: &dyn Tree,
    blobs: &dyn Blobs,
    journal: &dyn JournalStore,
) -> WriteError {
    match restore(j, tree, blobs, journal) {
        Ok(r) if r.reported.is_empty() => io_error(path, e, false),
        Ok(r) => WriteError::Io {
            path: path.to_vec(),
            message: format!(
                "{e}; returning to the pre-image left {} file(s) of other content",
                r.reported.len()
            ),
            journal_left: true,
        },
        Err((up, ue)) => WriteError::Io {
            path: path.to_vec(),
            message: format!(
                "{e}; returning to the pre-image failed at {}: {ue}",
                String::from_utf8_lossy(&up)
            ),
            journal_left: true,
        },
    }
}

/// The last paragraph of §7.3 for a journal, for each file: `stat` first
/// (a missing file, one not regular, or one under a directory that is a
/// link is reported, and nothing is removed under it); then its bytes: the
/// `pre` image is left, anything but the two images reported (its staged
/// name untouched), and the `post` image restored: its staged name
/// cleared, the pre-image blob created there, the file re-checked (the
/// same identity as the commit's re-check, against the `stat` taken before
/// the read) and the staged file renamed over it. A file that changed
/// since that `stat` is reported and kept, with the staged file (the
/// pre-image) left beside it. Then the directories are synced and the
/// journal removed if nothing was reported.
fn restore(
    j: &Journal,
    tree: &dyn Tree,
    blobs: &dyn Blobs,
    journal: &dyn JournalStore,
) -> Result<Recovered, (Vec<u8>, io::Error)> {
    let mut out = Recovered::default();
    for f in &j.files {
        let at = |p: &[u8]| {
            let p = p.to_vec();
            move |e| (p, e)
        };
        let clear_staged = || match tree.remove(&f.staged) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err((f.staged.clone(), e)),
            _ => Ok(()),
        };
        let st = match tree.stat(&f.path).map_err(at(&f.path))? {
            Some(st) if st.regular => st,
            _ => {
                out.reported.push(f.path.clone());
                continue;
            }
        };
        let bytes = tree.read(&f.path).map_err(at(&f.path))?;
        if is_image(&bytes, &f.pre) {
            clear_staged()?;
            out.left.push(f.path.clone());
        } else if is_image(&bytes, &f.post) {
            drop(bytes);
            clear_staged()?;
            let pre = blobs.get(f.pre.content).map_err(at(&f.path))?;
            if !is_image(&pre, &f.pre) {
                return Err((
                    f.path.clone(),
                    io::Error::other("the pre-image blob does not hash to its name"),
                ));
            }
            tree.create(&f.staged, st.mode, &pre).map_err(at(&f.staged))?;
            match tree.stat(&f.path).map_err(at(&f.path))? {
                Some(now) if now.regular && st.same_file(&now) => {}
                _ => {
                    out.reported.push(f.path.clone());
                    continue;
                }
            }
            tree.rename(&f.staged, &f.path).map_err(at(&f.path))?;
            out.restored.push(f.path.clone());
        } else {
            out.reported.push(f.path.clone());
        }
    }
    for d in dirs(j) {
        tree.sync_dir(d).map_err(at_dir(d))?;
    }
    if out.reported.is_empty() {
        remove_journal(journal).map_err(|e| (JOURNAL_PATH.to_vec(), e))?;
    }
    Ok(out)
}

fn at_dir(d: &[u8]) -> impl FnOnce(io::Error) -> (Vec<u8>, io::Error) + '_ {
    move |e| (d.to_vec(), e)
}

/// `slip apply --recover` (§7.3): finishes an unfinished write by
/// returning every file to the pre-image. `None` when there is no
/// journal. `ownership` is the one the write was given: a journal naming a
/// path it does not own is refused (`JournalNotOwned`) before anything is
/// touched, and kept. Idempotent, and safe to interrupt.
pub fn recover(
    ownership: &Ownership,
    tree: &dyn Tree,
    blobs: &dyn Blobs,
    journal: &dyn JournalStore,
) -> Result<Option<Recovered>, WriteError> {
    check_ownership(ownership)?;
    let Some(bytes) = journal.read().map_err(|e| io_error(JOURNAL_PATH, &e, true))? else {
        return Ok(None);
    };
    let j = Journal::from_bytes(&bytes).map_err(WriteError::Journal)?;
    if let Some(f) = j.files.iter().find(|f| !ownership.is_owned(&f.path)) {
        return Err(WriteError::Refused(Refused::JournalNotOwned(f.path.clone())));
    }
    restore(&j, tree, blobs, journal).map(Some).map_err(|(p, e)| io_error(&p, &e, true))
}
