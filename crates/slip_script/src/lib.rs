//! Host side of the transform universe: implements host calls over the
//! subject model and runs transforms (`spec/api.md`).

#![forbid(unsafe_code)]

mod effects;
pub mod fake;
pub mod host;
pub mod kiln_subject;
pub mod output;
pub mod resource;
pub mod subject;
pub mod wire;
