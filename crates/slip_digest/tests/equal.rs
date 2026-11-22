//! Must-equal pairs (`spec/digest.md` §14): the two sides' `Digests` are
//! equal with `Detail::All`; a unit only one side has, declared or
//! derived, is a failure, and `expect.txt` has no `derived-*` line
//! (`common::pairs::equal`, `spec/landed/harness/WP-047.md`).

mod common;

#[test]
fn equal() {
    common::run_all("equal", common::pairs::equal);
}
