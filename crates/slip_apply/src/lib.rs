//! Apply (`spec/edits.md` §7): from an accepted edit set and the
//! pre-image, the overlay (the post-image of every edited file, as blobs
//! named by digest, and the spelling obligations), the unified patch, and
//! the journaled write of the working tree with its recovery. Works on
//! model types only; no Kiln.
//!
//! This is the only code of Slip that changes a user's files. What the
//! specification leaves open and this crate decides is
//! `spec/landed/edits/WP-063.md`.

#![forbid(unsafe_code)]

mod build;
mod compose;
#[cfg(unix)]
pub mod fs;
pub mod journal;
mod patch;
mod splice;
mod write;

use slip_model::Digest;
use slip_model::overlay::Overlay;

pub use build::{POST_MAX, build, build_into, plan};
pub use patch::{CONTEXT, MYERS_WORK, patch};
pub use splice::splice;
pub use write::{
    Blobs, JOURNAL_PATH, JournalStore, Recovered, Refused, Stat, Tree, WriteError, recover, write,
};

/// The pre-image: the file system, or a map in tests.
pub trait Files {
    /// The bytes of the file at a real path.
    fn read(&self, path: &[u8]) -> std::io::Result<Vec<u8>>;
}

/// What `build` returns: the overlay and the post-image bytes, in overlay
/// order, each with its content digest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Built {
    pub overlay: Overlay,
    pub blobs: Vec<(Digest, Vec<u8>)>,
}

/// What apply fails with (§7.1, §7.2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApplyError {
    /// A file, edited or copied from, cannot be read or is not the
    /// `FileRef`'s size and digest (`stale_file`).
    StaleFile(Vec<u8>),
    /// The post-image of this file would pass 2^31 bytes (`too_large`).
    TooLarge(Vec<u8>),
    /// The edit set is refused.
    NotAccepted,
    /// The input breaks a rule: the edit set's invariants, a cycle, an
    /// edit that is not effective, a splice out of order, a name a patch
    /// cannot carry. `<path>: <rule>`.
    Invalid(String),
    /// Writing a blob or the patch failed, at this file.
    Io(Vec<u8>, String),
}
