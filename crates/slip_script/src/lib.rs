//! Host side of the transform universe: implements host calls over the
//! subject model and runs transforms (`spec/api.md`).

#![forbid(unsafe_code)]

pub mod fake;
pub mod host;
pub mod output;
pub mod subject;
pub mod wire;
