//! The transform run record, kind `RUNT` (`spec/edits.md` §10.6): what one
//! `map`, `rename` or `check` invocation did and what it cost. Not
//! deterministic. Work package WP-061.
//!
//! Plain data. The binary and JSON forms are in `formats`; the record has
//! no invariants beyond its encodings.

use crate::Digest;

/// A step of the command (§10.6).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StepName {
    Plan = 0,
    Map = 1,
    Reduce = 2,
    Apply = 3,
    Verify = 4,
    Scan = 5,
    Conclude = 6,
    Write = 7,
    /// Compiling the transform unit in the coordinator.
    Load = 8,
}

/// One step, in the order run (§10.6).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Step {
    pub name: StepName,
    pub wall_ns: u64,
    /// Of the coordinator process during the step; 0 if unknown.
    pub cpu_ns: u64,
    /// Of the coordinator process at the end of the step, bytes; 0 if unknown.
    pub peak_rss: u64,
    /// For map and verify: the content id of the phase's run record
    /// (`spec/model.md` §9).
    pub run: Option<Digest>,
}

/// The transform run record (§10.6), its fields in the spec's order.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TransformRun {
    pub tool_slip: String,
    pub tool_kiln: String,
    /// Content id of the plan.
    pub plan: Digest,
    pub round: u64,
    /// The request id.
    pub transform: Digest,
    pub edits: Option<Digest>,
    pub declared: Option<Digest>,
    pub overlay: Option<Digest>,
    pub result: Option<Digest>,
    pub started_unix_ns: u64,
    /// The whole command.
    pub wall_ns: u64,
    /// In the order run.
    pub steps: Vec<Step>,
}
