//! Section 27, `names`, of a shard of phase `map` or `verify`
//! (`spec/model.md` §8.6): the names digest of each key whose units hold a
//! name-derived string written symbolically (`spec/digest.md` §9 "The
//! names digest"), and, in a verify shard, the keys whose names digest
//! differs from the pre shard's (`spec/verify.md` §4.4). Work package
//! WP-112.
//!
//! Plain data. The binary and JSON forms and the invariants are in
//! `formats` (`spec/landed/verify/WP-112.md`).

use crate::Digest;

/// The names digest of one key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NamesEntry {
    /// The mapped key.
    pub key: Digest,
    /// Its names digest: of its one unit that has one, or of its units'
    /// (`spec/model.md` §8.6).
    pub names: Digest,
}

/// A key whose names digest is not the pre shard's (`spec/verify.md`
/// §4.4): a note `names_changed`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct NameNote {
    pub key: Digest,
    /// The subject path of the key's unit in the post-image (its group's
    /// file; empty for a unit of the `tu` group).
    pub file: Vec<u8>,
    /// The unit as a person would write it; for the report only.
    pub name: String,
}

/// Section 27 (`spec/model.md` §8.6).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Names {
    /// Strictly ascending by key.
    pub units: Vec<NamesEntry>,
    /// Phase `verify` only: strictly ascending by key, each a key of
    /// `units`; empty unless the verdict is `verified` or `mismatch`.
    pub notes: Vec<NameNote>,
}
