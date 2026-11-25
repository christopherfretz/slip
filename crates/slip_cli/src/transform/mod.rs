//! The commands of a transform (`spec/exec.md` §8.11 to §8.14;
//! `spec/edits.md` §10): `map`, `reduce`, `check`, `apply` and `rename`,
//! and the glue between the Kiln-free crates (`slip_reduce`, `slip_apply`,
//! `slip_verify::conclude`) and the store. Work packages WP-075 (`reduce`,
//! `apply`) and WP-076 (`map`, `check`, `rename`); what the specs leave
//! open and these modules decide is `spec/landed/edits/WP-075.md` and
//! `spec/landed/edits/WP-076.md`.
//!
//! The commands ask, they do not decide: whether a result may be written
//! is `slip_verify::conclude::permits`'s answer, and the ownership a write
//! checks is the request's, found through the result.

pub mod apply;
pub mod check;
pub mod lines;
pub mod map;
pub mod reduce;
pub mod rename;
pub mod report;
pub mod residue;
pub mod run_record;
pub mod store;

use std::fmt;

use slip_exec::store::{Object, Store, StoreError};
use slip_model::Digest;
use slip_model::formats;
use slip_model::wire::Error as FormatError;

use crate::commands::Failure;

/// Why a step of a transform's command failed: the line to print and the
/// status (`spec/exec.md` §8.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandError {
    /// Status 2; the usage text follows the line.
    Usage(String),
    /// Status 3; the line is `slip: <what>: <why>`.
    Input(String),
    /// The command was interrupted (§7): the signal path ends the process.
    Interrupted,
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommandError::Usage(line) | CommandError::Input(line) => f.write_str(line),
            CommandError::Interrupted => f.write_str("interrupted"),
        }
    }
}

impl std::error::Error for CommandError {}

/// The command's failure for `e`.
pub(crate) fn failure(e: CommandError) -> Failure {
    match e {
        CommandError::Usage(line) => Failure::Usage(line),
        CommandError::Input(line) => Failure::Input(line),
        CommandError::Interrupted => Failure::Interrupted,
    }
}

/// `e` as a [`CommandError`].
pub(crate) fn command_error(f: Failure) -> CommandError {
    match f {
        Failure::Usage(line) => CommandError::Usage(line),
        Failure::Input(line) => CommandError::Input(line),
        Failure::Interrupted => CommandError::Interrupted,
    }
}

/// A store error as an input error (`spec/exec.md` §8.5).
pub(crate) fn store_error(store: &Store, e: &StoreError) -> CommandError {
    command_error(crate::commands::store_failure(store.root(), e))
}

/// Reads `object` `id` from the store, decodes it with `decode` (every
/// rule of its format) and checks that its content id is `id`; `what`
/// names it in the message of a missing object. Every failure is an input
/// error naming the file.
pub(crate) fn read_checked<T>(
    store: &Store,
    object: Object,
    id: &Digest,
    what: &str,
    decode: fn(&[u8]) -> Result<T, FormatError>,
) -> Result<T, CommandError> {
    let path = store.object_path(object, id);
    let bytes =
        store.read_object(object, id).map_err(|e| store_error(store, &e))?.ok_or_else(|| {
            CommandError::Input(format!("{}: {what} {id} is not in the store", path.display()))
        })?;
    let value =
        decode(&bytes).map_err(|e| CommandError::Input(format!("{}: {e}", path.display())))?;
    match formats::content_id(&bytes) {
        Ok(c) if c == *id => Ok(value),
        _ => Err(CommandError::Input(format!(
            "{}: the file's content id is not its name",
            path.display()
        ))),
    }
}
