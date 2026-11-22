//! Coordinator, worker pool, jobserver client, memory admission and the
//! content-addressed store (`spec/exec.md`).
//!
//! This crate does not link Kiln: what a worker does with an observer is
//! supplied by the caller (`slip_cli`), so the execution machinery is tested
//! with fake work.

#![forbid(unsafe_code)]
