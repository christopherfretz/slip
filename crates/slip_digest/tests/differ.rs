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

use std::collections::BTreeMap;

#[test]
fn differ() {
    let report = std::env::var_os("SLIP_DIGEST_DERIVED").is_some_and(|v| v == "report");
    let outcome = common::run_collect("differ", |dir| {
        common::pairs::differ(dir).map(|lines| {
            if report {
                lines.into_iter().map(|l| format!("unnamed {l}")).collect()
            } else {
                Vec::new()
            }
        })
    });
    if report {
        let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
        let mut by_dir: BTreeMap<&str, usize> = BTreeMap::new();
        for n in &outcome.notes {
            let Some((dir, line)) = n.split_once(": unnamed ") else { continue };
            let word = line.split(' ').next().unwrap_or_default();
            let loose = if line.ends_with(" (written derived-changed)") {
                " (written derived-changed)"
            } else {
                ""
            };
            *kinds.entry(format!("{word}{loose}")).or_default() += 1;
            *by_dir.entry(dir).or_default() += 1;
        }
        let mut most: Vec<_> = by_dir.iter().collect();
        most.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        eprintln!(
            "SLIP_DIGEST_DERIVED=report: {} unnamed derived differences in {} directories",
            kinds.values().sum::<usize>(),
            by_dir.len()
        );
        for (k, n) in &kinds {
            eprintln!("  {n:>6}  {k}");
        }
        eprintln!("the directories with the most:");
        for (d, n) in most.iter().take(20) {
            eprintln!("  {n:>6}  {d}");
        }
    }
    outcome.finish();
}
