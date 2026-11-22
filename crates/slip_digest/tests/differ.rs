//! Must-differ pairs (`spec/digest.md` §14, `spec/digest-kinds.md` §1.5,
//! `spec/landed/harness/WP-047.md`): over declared and `tu` units, the
//! keys whose units differ between the sides are exactly the `changed`,
//! `added` and `removed` lines; derived units are free unless a
//! `derived-*` line names one, or all of them are named under the
//! directive `exact derived` (`common::pairs::differ`).
//!
//! With `SLIP_DIGEST_DERIVED=report` the runner also prints, per
//! directory, the derived units that differ and no line names, as the
//! lines that would name them, and their count by kind; it still passes.

mod common;

use common::pairs;

#[test]
fn differ() {
    let report = pairs::report_mode(std::env::var_os("SLIP_DIGEST_DERIVED").as_deref());
    let outcome = common::run_collect("differ", |dir| pairs::differ_noted(dir, report));
    if report {
        eprint!("{}", pairs::report_summary(&outcome));
    }
    outcome.finish();
}
