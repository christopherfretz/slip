//! Developer tasks for Slip: the test, lint, benchmark and corpus harness
//! (`spec/harness.md`). Run as `cargo xtask <command>`; `main.rs` only
//! dispatches to one module per command:
//!
//! - [`lint`]: `cargo xtask lint` (§3),
//! - [`wp`]: `cargo xtask wp [--ready]` (§4),
//! - [`ci`]: `cargo xtask ci [--quick] [--step <name>]… [--keep-going]` (§2),
//! - [`corpus`]: `cargo xtask corpus list|fetch|configure|status|path` (§8),
//! - [`jobserver`]: `cargo xtask jobserver status|refill` (§5),
//! - [`perf`]: `cargo xtask perf summarize|record|check` (§7).
//!
//! Every command takes the repository root as a parameter so the tests can
//! point it at fixture trees; [`repo_root`] is the root of the checkout this
//! binary was built from. [`repo::Repo`] lists the files a command reads.

#![forbid(unsafe_code)]

pub mod ci;
pub mod corpus;
pub mod jobserver;
pub mod lint;
pub mod perf;
pub mod repo;
pub mod wp;

use std::path::PathBuf;

/// The root of the Slip checkout (the parent of `xtask/`).
///
/// `cargo xtask` runs this binary through `cargo run`, which sets
/// `CARGO_MANIFEST_DIR` to the package being run: that is the checkout the
/// user is in. The directory compiled in is only the fallback (the binary
/// started directly), because a target directory shared by two checkouts
/// can hold a binary built from the other one.
pub fn repo_root() -> PathBuf {
    let manifest = std::env::var_os("CARGO_MANIFEST_DIR")
        .filter(|dir| !dir.is_empty())
        .map_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")), PathBuf::from);
    manifest.parent().map_or(manifest.clone(), PathBuf::from)
}
