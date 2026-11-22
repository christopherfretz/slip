//! Slip's data model and on-disk formats (`spec/model.md`).
//!
//! Everything the phases exchange is defined here: file and observer
//! identity, entity keys, ranges, edits, facts, shards, plans and reports,
//! with their versioned binary encoding and JSON rendering. This crate
//! depends on nothing (not on Kiln, not on I/O policy), so that reduce,
//! apply and verify can be tested with hand-written inputs and
//! re-implemented from the specification.

#![forbid(unsafe_code)]
