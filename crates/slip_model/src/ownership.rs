//! Section 35 (`ownership`) of a verify shard and its content-addressed
//! [`OwnershipList`] blob (`spec/model.md` §8.9; work package WP-150 stage 2).
//!
//! Plain data. The binary and JSON forms and the reader's invariants live in
//! `formats::ownership`.

use crate::Digest;

/// Which test put a dependency entity in [`OwnershipList::fixed`]
/// (`spec/landed/verify/WP-146.md` §3.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FixedAs {
    /// Split test: its own key is one a rule could make.
    T = 0,
    /// Merge test: its key were it owned differs from its own key.
    M = 1,
}

impl FixedAs {
    /// Every value, in code order.
    pub const ALL: [FixedAs; 2] = [FixedAs::T, FixedAs::M];

    /// Its name in JSON (`spec/model.md` §8.9).
    pub fn name(self) -> &'static str {
        match self {
            FixedAs::T => "t",
            FixedAs::M => "m",
        }
    }
}

/// One owned entity whose key a rule made, in an [`OwnershipList`]
/// (`spec/model.md` §8.9).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MadeEntry {
    /// Its mapped key's id (the pre-image key): the T join's.
    pub key: Digest,
    /// The id of its key were it a dependency's (the post-image key): the M
    /// join's; `None` where that key could not be formed.
    pub post: Option<Digest>,
    /// The rule's index in `Declared.rules`.
    pub rule: u64,
    /// 1-based line of its first declaration; 0 iff `column` is 0.
    pub line: u64,
    /// 1-based column of its first declaration; 0 iff `line` is 0.
    pub column: u64,
}

/// One dependency entity in an [`OwnershipList`] (`spec/model.md` §8.9).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FixedEntry {
    /// Its own key's id (unmapped).
    pub key: Digest,
    pub as_: FixedAs,
    /// 1-based line of its first read-only declaration; 0 iff `column` is 0.
    pub line: u64,
    /// 1-based column of its first read-only declaration; 0 iff `line` is 0.
    pub column: u64,
    /// The entity as a person writes it, at most `MESSAGE_MAX` (4096) bytes.
    pub name: String,
}

/// One file's ownership list blob (`spec/model.md` §8.9), published under its
/// content digest (`Store::publish_blob`) and named from section 35 of a
/// verify shard.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct OwnershipList {
    /// Subject path of the declaration's file (empty only when the
    /// declaration has no location).
    pub file: Vec<u8>,
    /// Empty, else the subject path of the read-only file through which
    /// `file` was entered when `file` is read-only here by the include-chain
    /// rule alone.
    pub through: Vec<u8>,
    /// Strictly ascending by `(key, post)` (`None < Some`).
    pub made: Vec<MadeEntry>,
    /// Strictly ascending by `(key, as_)` (`T < M`).
    pub fixed: Vec<FixedEntry>,
}
