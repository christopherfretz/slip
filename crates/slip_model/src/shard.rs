//! The shard: one observer's output for one phase (`spec/model.md` §8).
//!
//! Plain data. The binary and JSON forms are implemented in `formats`
//! (work package WP-006).

use crate::agreement::{Binding, RuleUse};
use crate::digests::Digests;
use crate::edits::Edits;
use crate::names::Names;
use crate::spellings::Spellings;
use crate::verdict::Verdict;
use crate::{Digest, FileRef};

/// A parallel phase.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Phase {
    Map = 1,
    Verify = 2,
}

/// How an observer's work ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Status {
    /// The front end finished with no error.
    Ok = 0,
    /// The front end reported errors.
    Diagnosed = 1,
    /// The front end refused the command line.
    Rejected = 2,
    /// The worker could not produce a result; written by the coordinator.
    Failed = 3,
}

/// Status and diagnostic totals.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    pub status: Status,
    /// Diagnostics of severity error or fatal.
    pub errors: u64,
    pub warnings: u64,
    /// Empty for `Ok`; otherwise the first error, at most 4096 bytes.
    pub message: String,
}

/// A deterministic quantity, named `word.word` in lower-case ASCII.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Counter {
    pub name: String,
    pub value: u64,
}

/// A shard (`spec/model.md` §8). Deterministic content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shard {
    pub observer: Digest,
    pub phase: Phase,
    pub round: u64,
    pub outcome: Outcome,
    /// Every file the front end read, sorted by path; absent when unknown or failed.
    pub inputs: Option<Vec<FileRef>>,
    /// Sorted by name.
    pub counters: Vec<Counter>,
    /// The semantic digest (`spec/digest.md` §11); only for status `Ok`.
    pub digests: Option<Digests>,
    /// The verdict of a verify shard (`spec/verify.md` §6); only for phase
    /// `Verify`, status `Ok` and with `digests`.
    pub verdict: Option<Verdict>,
    /// Section 17: what a transform's run over this observer produced
    /// (`spec/edits.md` §4); only for phase `Map` and status `Ok`.
    pub edits: Option<Edits>,
    /// Section 24: what a verify shard was made with (`spec/verify.md`
    /// §9.1); only for phase `Verify` and a status other than `Failed`.
    pub binding: Option<Binding>,
    /// Section 25: how the walk used each rule (`spec/verify.md` §9.1);
    /// only for phase `Verify` and status `Ok`.
    pub rule_use: Option<RuleUse>,
    /// Section 26: the spelled-entity checks (`spec/verify.md` §10.4); only
    /// for phase `Verify` and status `Ok`.
    pub spellings: Option<Spellings>,
    /// Section 27: the names digests (`spec/model.md` §8.7), and in a
    /// verify shard the notes (`spec/verify.md` §4.4); present exactly
    /// when `digests` is.
    pub names: Option<Names>,
    /// Section 28: the negative inputs (`spec/model.md` §8.6), each place
    /// the map parse looked for a file and found none, as a path, sorted
    /// and unique; only for phase `Map` and with `inputs`. Absent: the
    /// parse that wrote the shard did not record them.
    pub negatives: Option<Vec<Vec<u8>>>,
}
