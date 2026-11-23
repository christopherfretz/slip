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
use slip_model::transform::Ownership;

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
}

/// The working tree.
pub trait Tree {
    /// `None`: no such file.
    fn stat(&self, path: &[u8]) -> io::Result<Option<Stat>>;
    fn read(&self, path: &[u8]) -> io::Result<Vec<u8>>;
    /// Creates `path` with these permission bits and bytes, written and
    /// synced before it returns; `exclusive`: fails if `path` exists.
    fn create(&self, path: &[u8], mode: u32, bytes: &[u8], exclusive: bool) -> io::Result<()>;
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
    /// Publishes atomically and durably: after `Ok`, a reader sees exactly
    /// these bytes, also after a crash.
    fn publish(&self, bytes: &[u8]) -> io::Result<()>;
    fn read(&self) -> io::Result<Option<Vec<u8>>>;
    /// Removes it, durably; `Ok` or `NotFound` when there is none.
    fn remove(&self) -> io::Result<()>;
}

/// Why `write` refused before replacing anything (§7.3 steps 1 and 4).
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
    /// A file has its staged name (step 1); the path named is the staged one.
    StagedExists(Vec<u8>),
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
    /// The overlay is malformed or is not the overlay its id names.
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

fn is_image(bytes: &[u8], c: &Content) -> bool {
    bytes.len() as u64 == c.size && content_digest(bytes) == c.content
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

/// Step 1 (`again` false) or step 4 (`again` true) for one file. Returns
/// its permission bits.
fn check(
    f: &JournalEntry,
    ownership: &Ownership,
    tree: &dyn Tree,
    again: bool,
) -> Result<u32, Fail> {
    let path = &f.path;
    if !ownership.is_owned(path) {
        return Err(Refused::NotOwned(path.clone()).into());
    }
    let Some(st) = tree.stat(path).map_err(io_at(path))? else {
        return Err(Refused::StaleFile(path.clone()).into());
    };
    if !st.regular {
        return Err(Refused::NotRegular(path.clone()).into());
    }
    if st.links != 1 {
        return Err(Refused::HardLinked(path.clone()).into());
    }
    if !st.dir_writable {
        return Err(Refused::NotWritable(path.clone()).into());
    }
    let staged = tree.stat(&f.staged).map_err(io_at(&f.staged))?;
    if again {
        // The staged file is the one step 3 wrote: a regular file of one
        // link with the post-image's bytes.
        let ours = staged.is_some_and(|s| s.regular && s.links == 1)
            && is_image(&tree.read(&f.staged).map_err(io_at(&f.staged))?, &f.post);
        if !ours {
            return Err(Refused::StaleFile(f.staged.clone()).into());
        }
    } else if staged.is_some() {
        return Err(Refused::StagedExists(f.staged.clone()).into());
    }
    if !is_image(&tree.read(path).map_err(io_at(path))?, &f.pre) {
        return Err(Refused::StaleFile(path.clone()).into());
    }
    Ok(st.mode)
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
        Ok(Some(_)) => {
            return Err(WriteError::Journal("the store has an unfinished write".to_string()));
        }
        Err(e) => return Err(io_error(JOURNAL_PATH, &e, false)),
    }
    let fail = |f: Fail| match f {
        Fail::Refused(r) => WriteError::Refused(r),
        Fail::Io(p, e) => io_error(&p, &e, false),
    };

    // 1. Check.
    let mut modes = Vec::with_capacity(j.files.len());
    for f in &j.files {
        modes.push(check(f, ownership, tree, false).map_err(fail)?);
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
    if let Err(e) = journal.publish(&j.to_bytes()) {
        // It may have been published all the same.
        let left = remove_journal(journal).is_err();
        return Err(io_error(JOURNAL_PATH, &e, left));
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
        if let Err(e) = tree.create(&f.staged, modes[i], &post, true) {
            // A create that failed because the name exists created nothing
            // of ours; any other may have left part of a file.
            staged[i] = e.kind() != io::ErrorKind::AlreadyExists;
            return Err(after_undo(&f.staged, &e, undo_staged(&j, &staged, tree, journal)));
        }
        staged[i] = true;
    }

    // 4. Check again.
    for f in &j.files {
        if let Err(fl) = check(f, ownership, tree, true) {
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

    // 5. Commit.
    for f in &j.files {
        if let Err(e) = tree.rename(&f.staged, &f.path) {
            return Err(rolled_back(&f.path, &e, &j, tree, blobs, journal));
        }
    }
    for d in dirs(&j) {
        if let Err(e) = tree.sync_dir(d) {
            return Err(rolled_back(d, &e, &j, tree, blobs, journal));
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

/// The last paragraph of §7.3 for a journal: for each file, its staged
/// file removed; then the file left (the `pre` image), restored (the
/// `post` image: the pre-image blob written to the staged name and renamed
/// over it) or reported (anything else); the directories synced; the
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
        match tree.remove(&f.staged) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => return Err((f.staged.clone(), e)),
            _ => {}
        }
        let st = match tree.stat(&f.path).map_err(at(&f.path))? {
            Some(st) if st.regular => st,
            _ => {
                out.reported.push(f.path.clone());
                continue;
            }
        };
        let bytes = tree.read(&f.path).map_err(at(&f.path))?;
        if is_image(&bytes, &f.pre) {
            out.left.push(f.path.clone());
        } else if is_image(&bytes, &f.post) {
            drop(bytes);
            let pre = blobs.get(f.pre.content).map_err(at(&f.path))?;
            if !is_image(&pre, &f.pre) {
                return Err((
                    f.path.clone(),
                    io::Error::other("the pre-image blob does not hash to its name"),
                ));
            }
            tree.create(&f.staged, st.mode, &pre, true).map_err(at(&f.staged))?;
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
/// journal. Idempotent, and safe to interrupt.
pub fn recover(
    tree: &dyn Tree,
    blobs: &dyn Blobs,
    journal: &dyn JournalStore,
) -> Result<Option<Recovered>, WriteError> {
    let Some(bytes) = journal.read().map_err(|e| io_error(JOURNAL_PATH, &e, true))? else {
        return Ok(None);
    };
    let j = Journal::from_bytes(&bytes).map_err(WriteError::Journal)?;
    restore(&j, tree, blobs, journal).map(Some).map_err(|(p, e)| io_error(&p, &e, true))
}
