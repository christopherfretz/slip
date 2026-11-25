//! WP-118 R7.3 and R1.3 (`spec/verify.md` §9.1, §9.4): the fold of
//! section 33 (`rebuilt_use`) over shards written by hand: `relied` (an
//! entry only an `unverifiable` observer relied on is not unused), unused
//! entries, `declares` with each count the greatest over observers, hints
//! printed only for `mismatch` or stale observers, the caps, and an index
//! out of range making the run `incomplete`.

mod common;

use common::{Run, d, rule, used};
use slip_model::Shard;
use slip_model::agreement::{Declares, RebuiltUse};
use slip_model::json;
use slip_model::keys::KeyKind;
use slip_model::transform::Ownership;
use slip_model::verdict::VerdictKind;
use slip_verify::run::{Report, RunVerdict, State};

/// One rule, the run's files `/s` owned and `/b/a` and `/b/c` rebuilt.
fn run() -> Run {
    let mut run = Run::new(vec![rule(KeyKind::Namespace, "n", "m")], None);
    let files = Ownership {
        owned: vec![b"/s".to_vec()],
        readonly: vec![],
        rebuilt: vec![b"/b/a".to_vec(), b"/b/c".to_vec()],
    };
    run.declared = run.declared.clone().with_files(Some(files));
    run
}

fn with(mut s: Shard, section: RebuiltUse) -> Shard {
    s.rebuilt_use = Some(section);
    s.validate().expect("a valid shard");
    s
}

fn relied(r: &[u64]) -> RebuiltUse {
    RebuiltUse { relied: r.to_vec(), ..RebuiltUse::default() }
}

fn row(file: &str, rule: u64, mapped: u64, left: u64, left_kept: u64) -> Declares {
    Declares { file: file.as_bytes().to_vec(), rule, mapped, left, left_kept }
}

fn hints(files: &[String]) -> RebuiltUse {
    RebuiltUse {
        hints: files.iter().map(|f| f.as_bytes().to_vec()).collect(),
        ..RebuiltUse::default()
    }
}

/// `add_at` at the next position with main file `/m/<obs>.cc` and the
/// build directory `/b`.
fn add(r: &mut Report, position: u64, s: &Shard) {
    let main = format!("/m/{:02x}.cc", s.observer.0[0]);
    r.add_at(position, &s.observer, main.as_bytes(), Some(s), common::MAP_OK);
    r.directory_at(position, b"/b");
}

fn lines(r: &Report, prefix: &str) -> Vec<String> {
    r.to_text().lines().filter(|l| l.starts_with(prefix)).map(str::to_owned).collect()
}

fn json(r: &Report) -> String {
    json::write_compact(&r.to_json()).unwrap()
}

#[test]
fn relied_over_every_ok_shard_whatever_its_verdict() {
    let run = run();
    let mut r = run.report();
    add(&mut r, 0, &with(run.verified(0xa0), relied(&[0])));
    add(&mut r, 1, &with(run.unverifiable(0xa1, "post-image: x"), relied(&[1])));
    assert_eq!(
        lines(&r, "rebuilt /b/"),
        ["rebuilt /b/a relied on by 1 observer(s)", "rebuilt /b/c relied on by 1 observer(s)"]
    );
    assert!(lines(&r, "unused rebuilt").is_empty());
    assert!(
        json(&r).contains(
            r#""rebuilt":{"entries":[{"path":"/b/a","observers":1},{"path":"/b/c","observers":1}]"#
        ),
        "{}",
        json(&r)
    );

    let mut r = run.report();
    add(&mut r, 0, &with(run.verified(0xa0), relied(&[0])));
    add(&mut r, 1, &run.verified(0xa1));
    assert_eq!(lines(&r, "unused rebuilt"), ["unused rebuilt /b/c"]);
    // An unused entry changes no verdict.
    assert_eq!(r.verdict(), RunVerdict::Verified);
}

#[test]
fn declares_counts_are_the_greatest_over_observers() {
    let run = run();
    let mut r = run.report();
    let a = RebuiltUse {
        declares: vec![row("/b/a/x.h", 0, 1, 2, 0), row("/b/a/y.h", 0, 4, 0, 1)],
        ..RebuiltUse::default()
    };
    let b = RebuiltUse { declares: vec![row("/b/a/x.h", 0, 3, 1, 5)], ..RebuiltUse::default() };
    add(&mut r, 0, &with(run.verified(0xa0), a));
    add(&mut r, 1, &with(run.mismatch(0xa1), b));
    let j = json(&r);
    assert!(
        j.contains(
            r#""declares":[{"file":"/b/a/x.h","rule":0,"observers":2,"mapped":3,"left":2,"left_kept":5},{"file":"/b/a/y.h","rule":0,"observers":1,"mapped":4,"left":0,"left_kept":1}]"#
        ),
        "{j}"
    );
    // Only rows with `left > 0` have a line; `entit(y|ies)` by the count.
    assert_eq!(
        lines(&r, "rebuilt file"),
        [
            "rebuilt file /b/a/x.h still declares 2 entities under the name rule 0 says was left (not regenerated)"
        ]
    );
    let mut r = run.report();
    let one = RebuiltUse { declares: vec![row("/b/a/x.h", 0, 0, 1, 0)], ..RebuiltUse::default() };
    add(&mut r, 0, &with(run.verified(0xa0), one));
    assert_eq!(
        lines(&r, "rebuilt file"),
        [
            "rebuilt file /b/a/x.h still declares 1 entity under the name rule 0 says was left (not regenerated)"
        ]
    );
}

#[test]
fn hints_printed_for_mismatch_or_stale_observers_only() {
    let run = run();
    let hint = || hints(&["/b/u.cpp".to_owned()]);
    // Verified: the cut cost nothing; the JSON has the hint.
    let mut r = run.report();
    add(&mut r, 0, &with(run.verified(0xa0), hint()));
    assert!(lines(&r, "hint:").is_empty());
    assert!(
        json(&r).contains(
            r#""hints":[{"file":"/b/u.cpp","directory":"/b","observers":1,"first":"/m/a0.cc"}]"#
        ),
        "{}",
        json(&r)
    );
    // Mismatch.
    let mut r = run.report();
    add(&mut r, 0, &with(run.verified(0xa0), hint()));
    add(&mut r, 1, &with(run.mismatch(0xa1), hint()));
    assert_eq!(
        lines(&r, "hint:"),
        [
            "hint: /b/u.cpp is read-only, is under the build directory /b, and makes code of the run read-only in 1 observer(s) (first /m/a1.cc); if the build writes it again from source, pass --rebuilt /b/u.cpp (only if no binary outside this run was compiled against it)"
        ]
    );
    // Stale for a rule.
    let mut r = run.report();
    let v = run.verdict(VerdictKind::Verified);
    add(&mut r, 0, &with(run.ok(0xa0, v, &[], &[used(0, 0, 1)]), hint()));
    assert_eq!(count(&r, State::Verified), 1);
    assert_eq!(lines(&r, "hint:").len(), 1);
    // Unverifiable (WP-118 review S5): the cut did not make the verdict,
    // so no line; the JSON still has the hint.
    let mut r = run.report();
    add(&mut r, 0, &with(run.unverifiable(0xa1, "post-image: x"), hint()));
    assert_eq!(count(&r, State::Unverifiable), 1);
    assert!(lines(&r, "hint:").is_empty(), "{:?}", lines(&r, "hint:"));
    assert!(json(&r).contains(r#""hints":[{"file":"/b/u.cpp""#), "{}", json(&r));
}

fn count(r: &Report, st: State) -> u64 {
    r.states()[State::ALL.iter().position(|&x| x == st).unwrap()]
}

#[test]
fn the_caps() {
    let mut run = run();
    run.plan_observers = 20;
    let mut r = run.report();
    // 101 rows with `left > 0`, over two observers.
    let rows = |from: usize, to: usize| RebuiltUse {
        declares: (from..to).map(|i| row(&format!("/b/a/f{i:03}.h"), 0, 0, 1, 0)).collect(),
        ..RebuiltUse::default()
    };
    add(&mut r, 0, &with(run.verified(0x10), rows(0, 60)));
    add(&mut r, 1, &with(run.verified(0x11), rows(60, 101)));
    let file = lines(&r, "rebuilt file");
    assert_eq!(file.len(), 101, "100 lines and the `more` line");
    assert_eq!(file[100], "rebuilt file: 1 more (see the report)");
    // 21 hinted files in `mismatch` observers (at most 16 per shard).
    let names = |from: usize, to: usize| -> Vec<String> {
        (from..to).map(|i| format!("/b/u{i:02}.cpp")).collect()
    };
    add(&mut r, 2, &with(run.mismatch(0x12), hints(&names(0, 16))));
    add(&mut r, 3, &with(run.mismatch(0x13), hints(&names(16, 21))));
    let hint = lines(&r, "hint:");
    assert_eq!(hint.len(), 21);
    assert_eq!(hint[20], "hint: 1 more (see the report)");
}

#[test]
fn an_index_out_of_range_makes_the_run_incomplete() {
    let run = run();
    let mut r = run.report();
    add(&mut r, 0, &with(run.verified(0xa0), relied(&[2])));
    assert_eq!(count(&r, State::Failed), 1);
    assert_eq!(r.verdict(), RunVerdict::Incomplete);
    assert!(
        r.error_lines().contains("the shard's rebuilt_use names rebuilt entry 2; the run has 2"),
        "{}",
        r.error_lines()
    );

    let mut r = run.report();
    let bad = RebuiltUse { declares: vec![row("/b/a/x.h", 1, 1, 0, 0)], ..RebuiltUse::default() };
    add(&mut r, 0, &with(run.verified(0xa0), bad));
    assert_eq!(r.verdict(), RunVerdict::Incomplete);
    assert!(
        r.error_lines().contains("the shard's rebuilt_use names rule 1; the run has 1"),
        "{}",
        r.error_lines()
    );
}

/// P0: a run with no rebuilt entry and no hint has no `"rebuilt"` member
/// and no line of it.
#[test]
fn no_list_and_no_hint_no_member() {
    let run = Run::new(vec![rule(KeyKind::Namespace, "n", "m")], None);
    let mut r = run.report();
    add(&mut r, 0, &run.verified(0xa0));
    assert!(!json(&r).contains("\"rebuilt\""), "{}", json(&r));
    assert!(!r.to_text().contains("rebuilt"), "{}", r.to_text());
    // A hint alone makes the member.
    let mut r = run.report();
    add(&mut r, 0, &with(run.verified(0xa0), hints(&["/b/u.cpp".to_owned()])));
    assert!(
        json(&r).contains(r#""rebuilt":{"entries":[],"declares":[],"hints":[{"#),
        "{}",
        json(&r)
    );
    let _ = d(0);
}
