//! The result of one run of a transform, kind `RSLT` (`spec/edits.md`
//! §8.5), with its residue (§8.4). Work package WP-061.
//!
//! Plain data. Which state a result has is the trusted base's to decide
//! (conclude, `slip_verify`); the format checks only the invariants §8.5
//! lists (`spec/landed/edits/WP-061.md`).
//!
//! [`Spellings`] here is a result's summary of the obligations over the
//! run (`spec/verify.md` §10.3), not the shard section
//! `slip_model::spellings::Spellings` (§10.4).

use crate::Digest;
use crate::edits::KeyId;

/// The scheme of the result this crate reads and writes (§8.5).
pub const SCHEME: u64 = 1;

/// What a residue entry is (§8.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    Dependent = 0,
    NotCompiled = 1,
    NotPlanned = 2,
    Mention = 3,
}

/// Text that spells the old name and that no observer accounted for (§8.4).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Residue {
    pub category: Category,
    /// A canonical real path.
    pub file: Vec<u8>,
    pub lo: u64,
    pub hi: u64,
    /// 1-based.
    pub line: u64,
    /// 1-based, in bytes.
    pub column: u64,
}

/// The counts of the run (§8.5): the seven states of `spec/verify.md` §7
/// and the four of the map phase.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Tally {
    pub selected: u64,
    pub unparsed: u64,
    pub failed_runs: u64,
    pub error_reports: u64,
    pub verified: u64,
    pub unchanged: u64,
    pub mismatch: u64,
    pub not_parsed: u64,
    pub unverifiable: u64,
    pub never_parsed: u64,
    pub failed: u64,
}

/// The verify phase's verdict (`spec/verify.md` §7, §9.3). The one type:
/// `slip_verify::run` re-exports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RunVerdict {
    Verified = 0,
    NotVerified = 1,
    Incomplete = 2,
}

/// An observer that still declares a rule's entity under its old name
/// (`spec/verify.md` §9.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Stale {
    /// Index of the rule in the declared changes.
    pub rule: u64,
    pub observer: Digest,
    pub direct: u64,
}

/// Why a spelling obligation failed (§8.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Why {
    DenotesOther = 0,
    Unseen = 1,
}

/// A failed spelling obligation (§8.5, `spec/verify.md` §10.3).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FailedSpelling {
    /// Its index in the overlay's `spellings`.
    pub obligation: u64,
    pub why: Why,
    pub observers: u64,
    /// Absent for `unseen`.
    pub first: Option<Digest>,
    pub found: Option<KeyId>,
    pub name: String,
}

/// The spelling obligations over the run (`spec/verify.md` §10.3).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Spellings {
    pub obligations: u64,
    /// `passed + failed.len() == obligations`.
    pub passed: u64,
    /// Strictly ascending by `obligation`.
    pub failed: Vec<FailedSpelling>,
}

/// The state of a result (§8.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum State {
    Verified = 0,
    VerifiedWithResidue = 1,
    NotVerified = 2,
    Incomplete = 3,
    Refused = 4,
}

/// The result of one run of a transform (§8.5). `Result` in the spec.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TransformResult {
    /// [`SCHEME`].
    pub scheme: u64,
    pub plan: Digest,
    pub round: u64,
    /// The request id.
    pub transform: Digest,
    /// The edit set id.
    pub edits: Digest,
    /// Content id of the `DECL` container; absent iff `state` is `Refused`.
    pub declared: Option<Digest>,
    /// Absent iff `state` is `Refused`.
    pub overlay: Option<Digest>,
    pub state: State,
    /// The run selected fewer observers than the plan has.
    pub partial: bool,
    /// The run was given `--allow-unparsed`.
    pub allow_unparsed: bool,
    /// Absent if the verify phase did not run or did not finish.
    pub verdict: Option<RunVerdict>,
    pub tally: Tally,
    /// Strictly ascending by `(rule, observer)`.
    pub agreement: Vec<Stale>,
    /// Strictly ascending.
    pub unused: Vec<u64>,
    pub spellings: Spellings,
    /// Strictly ascending by `(file, lo)`.
    pub residue: Vec<Residue>,
    /// `Declared.changed`; strictly ascending.
    pub exempt: Vec<KeyId>,
}
