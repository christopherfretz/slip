//! Indeterminate tests (`spec/digest.md` §10, §14): side A only; the units
//! flagged for a cause other than `Pending` are exactly the
//! `indeterminate` lines, and none of those has the cause `Pending`. A
//! unit indeterminate only for `Pending` is ignored while `Area` exists.
//! `expect.txt` has `indeterminate` lines only (`common::pairs::indeterminate`).

mod common;

#[test]
fn indeterminate() {
    common::run_all("indeterminate", common::pairs::indeterminate);
}
