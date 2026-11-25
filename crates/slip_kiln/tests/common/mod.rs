//! What the tests of `slip_kiln` share: where they write the trees they
//! parse (`spec/harness.md` §11.5).

#![allow(dead_code, reason = "each test file uses a part")]

pub mod work_root;

use std::path::PathBuf;

/// The directory under which a test makes its trees: under
/// `$SLIP_TEST_ROOT` when set, else `CARGO_TARGET_TMPDIR`
/// ([`work_root::work_root_or`]).
pub fn work_root() -> PathBuf {
    work_root::work_root_or(env!("CARGO_TARGET_TMPDIR"))
}
