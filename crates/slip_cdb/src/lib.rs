//! Compilation-database readers, command normalisation and the plan
//! (`spec/cdb.md`).

#![forbid(unsafe_code)]

pub mod check;
pub mod database;
pub mod normalise;
pub mod plan;
pub mod rsp;
pub mod split;
