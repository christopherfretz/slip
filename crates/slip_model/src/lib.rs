//! Slip's data model and on-disk formats (`spec/model.md`).
//!
//! Everything the phases exchange is defined here: file and observer
//! identity, entity keys, ranges, edits, facts, shards, plans and reports,
//! with their versioned binary encoding and JSON rendering. This crate
//! depends on nothing (not on Kiln, not on I/O policy), so that reduce,
//! apply and verify can be tested with hand-written inputs and
//! re-implemented from the specification.

#![forbid(unsafe_code)]

pub mod agreement;
pub mod container;
pub mod declared;
mod digest;
pub mod digests;
pub mod edits;
pub mod editset;
pub mod formats;
pub mod hash;
pub mod json;
pub mod keys;
pub mod names;
pub mod overlay;
pub mod ownership;
mod plan;
pub mod result;
mod run;
mod shard;
pub mod spellings;
pub mod transform;
pub mod transform_run;
pub mod verdict;
pub mod wire;

pub use agreement::{Binding, RebuiltUse, RuleUse};
pub use digest::Digest;
pub use edits::{Claim, Edits, Piece, Place};
pub use editset::EditSet;
pub use overlay::Overlay;
pub use ownership::OwnershipList;
pub use plan::{
    Command, Configuration, EnvVar, Excluded, FileRef, Language, Observer, Origin, Plan, Reason,
};
pub use result::TransformResult;
pub use run::{End, Job, Run, RunRecord};
pub use shard::{Counter, Found, FoundPlace, Outcome, Phase, Shard, Status};
pub use spellings::Spellings;
pub use transform::Transform;
pub use transform_run::TransformRun;
