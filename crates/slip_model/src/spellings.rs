//! Section 26, `spellings`, of a verify shard (`spec/verify.md` §10.4): the
//! spelled-entity checks of one observer. Work package WP-060.
//!
//! Plain data. The binary and JSON forms and the invariants are in
//! `formats` (`spec/landed/edits/WP-060.md`).

use crate::edits::KeyId;

/// A run of obligation indices, `first .. first + count`; `count` at least 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Span {
    pub first: u64,
    pub count: u64,
}

/// An obligation that fails in this observer (`spec/verify.md` §10.4).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Failed {
    /// The obligation's index in the overlay's `spellings`.
    pub index: u64,
    /// The key, under the rules, of the origin of the first answering
    /// reference in unit order that does not have the obligation's key;
    /// absent if it has none.
    pub found: Option<KeyId>,
    /// That entity as a person would write it; for the report only.
    pub name: String,
}

/// Section 26 (`spec/verify.md` §10.4).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Spellings {
    /// Obligations whose file is in this shard's `inputs`.
    pub seen: u64,
    /// The obligations that hold, as runs of indices: ascending, no two
    /// touching.
    pub held: Vec<Span>,
    /// Strictly ascending by `index`.
    pub failed: Vec<Failed>,
}
