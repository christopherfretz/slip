//! `slip apply` (`spec/exec.md` §8.14, `spec/edits.md` §7.2, §7.3,
//! §10.2): the post-image of a published result written to the working
//! tree or as a patch, and `--recover`.
//!
//! Whether a result may be written is `slip_verify::conclude::permits`'s
//! answer, asked first; the ownership the write checks is the request's
//! (`TRFM`, found through the result), never the command line's.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use slip_apply::{ApplyError, Refused, WriteError};
use slip_exec::signals;
use slip_exec::store::{self as exec_store, Object, Store};
use slip_model::Digest;
use slip_model::editset::EditSet;
use slip_model::overlay::Overlay;
use slip_model::result::{State, TransformResult};
use slip_model::transform::{Ownership, Transform};
use slip_verify::conclude::{Allow, permits};

use super::store::{DiskFiles, StoreBlobs, StoreJournal, lossy};
use super::{CommandError, failure, read_checked};
use crate::args::{self, ApplyAction, ApplyArgs, DEFAULT_STORE};
use crate::commands::{self, Failure, store_failure};
use crate::status;

/// What `apply_result` did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Applied {
    /// `--write`: this many files replaced (an entry whose `post` is its
    /// `pre` is not one).
    Written(u64),
    /// `--patch`: the patch is at this path, as given.
    Patch(PathBuf),
    /// `permits` said no: nothing was read beyond the result and its set,
    /// nothing written. The result's state.
    NotPermitted(State),
    /// The write was refused (§7.3 step 1 or 4, or the re-check before a
    /// rename): the tree is as it was.
    Refused(Refused),
}

/// The name of a refusal (§7.3) and the path it names.
pub fn refused_name(r: &Refused) -> (&'static str, &[u8]) {
    match r {
        Refused::NotOwned(p) => ("not_owned", p),
        Refused::HardLinked(p) => ("hard_linked", p),
        Refused::NotWritable(p) => ("not_writable", p),
        Refused::StaleFile(p) => ("stale_file", p),
        Refused::NotRegular(p) => ("not_regular", p),
        Refused::StagedExists(p) => ("staged_exists", p),
        Refused::ReadOnly(p) => ("read_only", p),
        Refused::OwnerChanged(p) => ("owner_changed", p),
        Refused::JournalNotOwned(p) => ("journal_not_owned", p),
    }
}

/// A result and its edit set, read and checked against their ids.
pub(crate) fn load_result(
    store: &Store,
    result_id: Digest,
) -> Result<(TransformResult, EditSet), CommandError> {
    let result =
        read_checked(store, Object::Result, &result_id, "the result", TransformResult::from_bytes)?;
    let set =
        read_checked(store, Object::EditSet, &result.edits, "the edit set", EditSet::from_bytes)?;
    Ok((result, set))
}

/// The overlay `id`, checked against its id and against the edit set it
/// must be of.
fn load_overlay(store: &Store, id: Digest, edits: Digest) -> Result<Overlay, CommandError> {
    let overlay = read_checked(store, Object::Overlay, &id, "the overlay", Overlay::from_bytes)?;
    if overlay.edits != edits {
        return Err(CommandError::Input(format!(
            "{}: the overlay is of edit set {}, not of the result's, {edits}",
            store.object_path(Object::Overlay, &id).display(),
            overlay.edits
        )));
    }
    Ok(overlay)
}

/// The ownership of request `id` (§2.1).
fn ownership_of(store: &Store, id: Digest) -> Result<Ownership, CommandError> {
    let t = read_checked(store, Object::Transform, &id, "the request", Transform::from_bytes)?;
    Ok(t.ownership)
}

/// §7.2 or §7.3 for the published result `result_id`, once
/// `slip_verify::conclude::permits` has said yes for `allow`. The
/// ownership `--write` checks is the request's (`result.transform`, which
/// `permits` checked is the set's), and the overlay must be the result's
/// and of its set. Writes `written <n> file(s)` or `patch <path>` to `out`
/// when done.
///
/// `Input` errors: anything missing from the store or not the file its id
/// names; for `--write`, a failed operation (with the journal named when
/// one is left) or a journal already there; for `--patch`, a stale file, a
/// name a patch cannot carry, or a patch that cannot be written. `Usage`
/// for `--patch` with an overlay file outside `relative_to`.
pub fn apply_result(
    store: &Store,
    result_id: Digest,
    action: &ApplyAction,
    relative_to: Option<&Path>,
    allow: Allow,
    out: &mut dyn Write,
) -> Result<Applied, CommandError> {
    let (result, set) = load_result(store, result_id)?;
    if !permits(&result, &set, allow) {
        return Ok(Applied::NotPermitted(result.state));
    }
    let Some(overlay_id) = result.overlay else {
        unreachable!("permits says yes only to a result of a state that has an overlay")
    };
    let overlay = load_overlay(store, overlay_id, result.edits)?;
    match action {
        ApplyAction::Write => {
            let ownership = ownership_of(store, result.transform)?;
            let written = slip_apply::write(
                &overlay,
                overlay_id,
                &ownership,
                &slip_apply::fs::DiskTree,
                &StoreBlobs(store),
                &StoreJournal(store),
            );
            match written {
                Ok(n) => {
                    let _ = writeln!(out, "written {n} file(s)");
                    Ok(Applied::Written(n))
                }
                Err(WriteError::Refused(r)) => Ok(Applied::Refused(r)),
                Err(e) => Err(write_error(store, e)),
            }
        }
        ApplyAction::Patch(file) => {
            write_patch(store, &overlay, file, relative_to)?;
            let _ = out.write_all(b"patch ");
            let _ = out.write_all(&args::os_bytes(file.as_os_str()));
            let _ = writeln!(out);
            Ok(Applied::Patch(file.clone()))
        }
        ApplyAction::Recover => {
            Err(CommandError::Usage("`--recover` is not an action on a result".to_owned()))
        }
    }
}

/// An error of `write` or `recover` other than a refusal, as an input
/// error (status 3).
fn write_error(store: &Store, e: WriteError) -> CommandError {
    match e {
        WriteError::Io { path, message, journal_left } => {
            let journal = if journal_left {
                format!(
                    "; the store keeps the journal ({}): run `slip apply --recover`",
                    store.journal_path().display()
                )
            } else {
                String::new()
            };
            CommandError::Input(format!("{}: {message}{journal}", lossy(&path)))
        }
        WriteError::Journal(m) => {
            CommandError::Input(format!("{}: {m}", store.journal_path().display()))
        }
        WriteError::Invalid(m) => CommandError::Input(format!("apply: {m}")),
        WriteError::Refused(r) => {
            let (name, path) = refused_name(&r);
            CommandError::Input(format!("{}: refused: {name}", lossy(path)))
        }
    }
}

/// §7.2: the patch written to a temporary file beside `file` and renamed
/// to it, so that `file` is the whole patch or is left as it was.
fn write_patch(
    store: &Store,
    overlay: &Overlay,
    file: &Path,
    relative_to: Option<&Path>,
) -> Result<(), CommandError> {
    use std::os::unix::ffi::OsStrExt;
    let dir = match relative_to {
        None => None,
        Some(d) => Some(
            fs::canonicalize(d)
                .map_err(|e| CommandError::Input(format!("{}: {e}", d.display())))?,
        ),
    };
    let dir_bytes = dir.as_ref().map(|d| d.as_os_str().as_bytes().to_vec());
    if let Some(d) = &dir_bytes {
        // §7.2: an overlay file outside the directory is a usage error.
        if let Some(f) = overlay.files.iter().find(|f| f.pre != f.post && !inside(&f.path, d)) {
            return Err(CommandError::Usage(format!(
                "{}: the file is not inside `--relative-to` {}",
                lossy(&f.path),
                lossy(d)
            )));
        }
    }
    let name = file.file_name().ok_or_else(|| {
        CommandError::Usage(format!("`--patch`: `{}` is not a file name", file.display()))
    })?;
    let mut tmp_name = std::ffi::OsString::from(".");
    tmp_name.push(name);
    tmp_name.push(format!(".slip-tmp.{}", std::process::id()));
    let tmp = file.with_file_name(tmp_name);
    let io_failure = |p: &Path, e: &io::Error| CommandError::Input(format!("{}: {e}", p.display()));
    let created = fs::OpenOptions::new().write(true).create_new(true).open(&tmp);
    let handle = created.map_err(|e| io_failure(&tmp, &e))?;
    let mut sink = io::BufWriter::new(handle);
    let blobs =
        |id: Digest| -> io::Result<Vec<u8>> { slip_apply::Blobs::get(&StoreBlobs(store), id) };
    let made = slip_apply::patch(overlay, &DiskFiles, &blobs, dir_bytes.as_deref(), &mut sink);
    let finished = made.map_err(patch_error).and_then(|()| {
        let handle = sink.into_inner().map_err(|e| io_failure(&tmp, e.error()))?;
        handle.sync_all().map_err(|e| io_failure(&tmp, &e))?;
        fs::rename(&tmp, file).map_err(|e| io_failure(file, &e))
    });
    if finished.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    finished
}

fn inside(path: &[u8], dir: &[u8]) -> bool {
    dir == b"/" || matches!(path.strip_prefix(dir), Some([b'/', rest @ ..]) if !rest.is_empty())
}

/// An error of `slip_apply::patch` as an input error (status 3, §10.5: "a
/// stale file").
fn patch_error(e: ApplyError) -> CommandError {
    match e {
        ApplyError::StaleFile(p) => CommandError::Input(format!(
            "{}: stale_file: the file is not the pre-image the result was made from",
            lossy(&p)
        )),
        ApplyError::TooLarge(p) => CommandError::Input(format!("{}: too_large", lossy(&p))),
        ApplyError::NotAccepted => CommandError::Input("apply: the edit set is refused".to_owned()),
        ApplyError::Invalid(m) => CommandError::Input(format!("apply: {m}")),
        ApplyError::Io(p, m) => CommandError::Input(format!("{}: {m}", lossy(&p))),
    }
}

/// `slip apply` (§10.2, §10.5).
pub(crate) fn main(
    a: &ApplyArgs,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> commands::Result<u8> {
    let dir = a.store.clone().unwrap_or_else(|| PathBuf::from(DEFAULT_STORE));
    if a.action == ApplyAction::Recover {
        return recover(&dir, out, err);
    }
    let Some(prefix) = &a.result else {
        return Err(Failure::Usage("`apply` needs `--result` or `--recover`".to_owned()));
    };
    // Exclusive: a store with a journal is refused (`StoreError::UnfinishedWrite`).
    let store = commands::open_store(&dir, exec_store::Access::Exclusive)?;
    signals::install(Box::new(exec_store::cleanup_on_signal(&store)));
    let result_id = find_result(&store, prefix)?;
    let (result, _) = load_result(&store, result_id).map_err(failure)?;
    let _ = writeln!(out, "result {result_id}");
    let _ = writeln!(out, "state {}", result.state.name());
    let allow =
        Allow { residue: a.allow_residue, errors: a.allow_errors, partial: a.allow_partial };
    let applied = apply_result(&store, result_id, &a.action, a.relative_to.as_deref(), allow, out)
        .map_err(failure)?;
    let asked = if a.action == ApplyAction::Write { "--write" } else { "--patch" };
    match applied {
        Applied::Written(_) | Applied::Patch(_) => {
            // §10.5: a `verified_with_residue` result written under the
            // options that permit it has something to report.
            Ok(if result.state == State::Verified { status::OK } else { status::FINDINGS })
        }
        Applied::NotPermitted(state) => {
            let _ = writeln!(
                err,
                "slip: result {result_id}: its state `{}` does not permit `{asked}` with the \
                 options given; nothing was written",
                state.name()
            );
            Ok(status::FINDINGS)
        }
        Applied::Refused(r) => {
            let (name, path) = refused_name(&r);
            let _ = write!(err, "slip: refused {name}: ");
            let _ = err.write_all(path);
            let _ = writeln!(err, "; nothing was written");
            Ok(status::FINDINGS)
        }
    }
}

/// The one result whose id starts with `prefix` (4 to 64 lower-case
/// hexadecimal digits), as `--plan` finds a plan (§8.5).
fn find_result(store: &Store, prefix: &str) -> commands::Result<Digest> {
    let ids =
        store.find_object(Object::Result, prefix).map_err(|e| store_failure(store.root(), &e))?;
    match ids.as_slice() {
        [id] => Ok(*id),
        [] => Err(Failure::Input(format!(
            "result {prefix}: no result in the store {} has an id starting with it",
            store.root().display()
        ))),
        several => {
            let names: Vec<String> = several.iter().map(ToString::to_string).collect();
            Err(Failure::Input(format!(
                "result {prefix}: {} results have an id starting with it: {}",
                several.len(),
                names.join(", ")
            )))
        }
    }
}

/// The ownership of the write a journal records: the overlay it names, its
/// edit set, the set's request. Each link is a content id, and the write
/// was given that request's ownership (`apply_result` checked that the
/// overlay is the result's and of its set, and `permits` that the result's
/// request is the set's), so this is the ownership of the run that wrote
/// the journal (`spec/landed/edits/WP-063.md` §7).
fn ownership_of_journal(store: &Store, overlay_id: Digest) -> Result<Ownership, CommandError> {
    let overlay = read_checked(
        store,
        Object::Overlay,
        &overlay_id,
        "the journal's overlay",
        Overlay::from_bytes,
    )?;
    let set = read_checked(
        store,
        Object::EditSet,
        &overlay.edits,
        "the overlay's edit set",
        EditSet::from_bytes,
    )?;
    ownership_of(store, set.transform)
}

/// `slip apply --recover` (§7.3): status 0 when the tree is the
/// pre-image again (or there was no journal), 3 when a file had to be
/// reported (the journal is kept) or recovery could not start.
fn recover(dir: &Path, out: &mut dyn Write, err: &mut dyn Write) -> commands::Result<u8> {
    let store = Store::open_for_recovery(dir).map_err(|e| store_failure(dir, &e))?;
    signals::install(Box::new(exec_store::cleanup_on_signal(&store)));
    let journal = StoreJournal(&store);
    let bytes = slip_apply::JournalStore::read(&journal)
        .map_err(|e| Failure::Input(format!("{}: {e}", store.journal_path().display())))?;
    let Some(bytes) = bytes else {
        let _ = writeln!(out, "recovered: no unfinished write");
        return Ok(status::OK);
    };
    let j = slip_apply::journal::Journal::from_bytes(&bytes)
        .map_err(|m| Failure::Input(format!("{}: {m}", store.journal_path().display())))?;
    let ownership = ownership_of_journal(&store, j.overlay).map_err(|e| match e {
        CommandError::Input(m) => Failure::Input(format!(
            "{m}; the ownership of the unfinished write cannot be found, the journal is kept"
        )),
        e => failure(e),
    })?;
    let recovered =
        slip_apply::recover(&ownership, &slip_apply::fs::DiskTree, &StoreBlobs(&store), &journal);
    let r = match recovered {
        Ok(None) => {
            let _ = writeln!(out, "recovered: no unfinished write");
            return Ok(status::OK);
        }
        Ok(Some(r)) => r,
        Err(WriteError::Refused(Refused::JournalNotOwned(p))) => {
            return Err(Failure::Input(format!(
                "{}: refused: journal_not_owned: the journal names a file the write's request \
                 does not own; nothing was changed, the journal is kept",
                lossy(&p)
            )));
        }
        Err(e) => return Err(failure(write_error(&store, e))),
    };
    let _ = writeln!(
        out,
        "recovered: restored {}, left {}, reported {}",
        r.restored.len(),
        r.left.len(),
        r.reported.len()
    );
    for p in &r.reported {
        let _ = write!(err, "slip: ");
        let _ = err.write_all(p);
        let _ = writeln!(
            err,
            ": neither the pre-image nor the post-image of the write; left as it is, the \
             journal is kept ({})",
            store.journal_path().display()
        );
    }
    Ok(if r.reported.is_empty() { status::OK } else { status::INPUT })
}
