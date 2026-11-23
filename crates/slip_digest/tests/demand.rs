//! Demand independence (`spec/digest.md` §14,
//! `spec/landed/harness/WP-047.md`): every key in both sides has the same
//! digests and flags, and the declared keys only B has are exactly the
//! `added` lines (none if there are none); A has no declared key B lacks.
//! A key both sides declare is in the declared group of one file on both,
//! and a declared group that holds no key of one side only has the same
//! sum on both (`spec/digest.md` §11). Every `derived-added` line holds;
//! with `exact derived` the derived differences are exactly those lines
//! (`common::pairs::demand`).

mod common;

#[test]
fn demand() {
    common::run_all("demand", common::pairs::demand);
}
