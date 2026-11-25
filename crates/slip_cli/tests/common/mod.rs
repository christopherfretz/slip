//! What the tests of `slip_cli` share: where they write the trees that a
//! front end or `slip` reads (`spec/harness.md` §11.5).

#![allow(dead_code, reason = "each test file uses a part")]

#[path = "../../../slip_kiln/tests/common/work_root.rs"]
pub mod work_root;

use std::path::PathBuf;

/// The directory under which a test makes its trees: under
/// `$SLIP_TEST_ROOT` when set, else `CARGO_TARGET_TMPDIR`
/// ([`work_root::work_root_or`]).
pub fn work_root() -> PathBuf {
    work_root::work_root_or(env!("CARGO_TARGET_TMPDIR"))
}

/// The directory under which a test about spelling on a volume that folds
/// makes its trees: under `CARGO_TARGET_TMPDIR` (on this project's host,
/// the workspace's mount, which folds), never under `$SLIP_TEST_ROOT`
/// ([`work_root::folding_root_or`]; no hard link there).
pub fn folding_root() -> PathBuf {
    work_root::folding_root_or(env!("CARGO_TARGET_TMPDIR"))
}

/// The folding root for the test `test` when it folds; `None`, with one
/// line saying so, when no volume here folds. PANICS on the gate's host
/// when the folding root does not fold ([`work_root::folding_checked_or`]).
/// Every test that needs a volume that folds asks this.
pub fn folding_volume(test: &str) -> Option<PathBuf> {
    work_root::folding_volume_or(env!("CARGO_TARGET_TMPDIR"), test)
}

/// The folding root and whether it folds, for a test with a half that
/// runs on a volume of either kind ([`work_root::folding_checked_or`]).
pub fn folding_checked(test: &str) -> (PathBuf, bool) {
    work_root::folding_checked_or(env!("CARGO_TARGET_TMPDIR"), test)
}
