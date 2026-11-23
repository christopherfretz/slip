//! The pair runners' own tests (`spec/landed/harness/WP-047.md`,
//! `spec/digest.md` §14): the `derived-*` lines and `exact derived` of
//! `differ` and `demand`, what `equal` and `indeterminate` refuse, the
//! pool of `common::run_all` and the report mode of `differ`. The
//! fixture directories are under `tests/runner/`, which no runner walks.

mod common;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use common::pairs::{judge_demand, judge_differ, judge_equal};
use common::{Digested, Expect, Side};

fn fixture(rel: &str) -> PathBuf {
    common::manifest().join("tests/runner").join(rel)
}

fn sides(rel: &str) -> (Digested, Digested) {
    let dir = fixture(rel);
    let a = common::digest_side(&dir, Side::A).unwrap_or_else(|e| panic!("{rel}: {e}"));
    let b = common::digest_side(&dir, Side::B).unwrap_or_else(|e| panic!("{rel}: {e}"));
    (a, b)
}

fn set(lines: &[Expect]) -> BTreeSet<Expect> {
    lines.iter().cloned().collect()
}

fn changed(n: &str) -> Expect {
    Expect::Changed(n.to_owned())
}
fn added(n: &str) -> Expect {
    Expect::Added(n.to_owned())
}
fn d_added(n: &str) -> Expect {
    Expect::DerivedAdded(n.to_owned())
}
fn d_removed(n: &str) -> Expect {
    Expect::DerivedRemoved(n.to_owned())
}
fn d_changed(n: &str) -> Expect {
    Expect::DerivedChanged(n.to_owned())
}

/// The instantiation the `derived/*` and `demand/*` fixtures use.
const G: &str = "g<int>()";

fn ok(r: Result<Vec<String>, String>) -> Vec<String> {
    r.unwrap_or_else(|e| panic!("expected to hold: {e}"))
}

fn err(r: Result<Vec<String>, String>) -> String {
    r.expect_err("expected to fail")
}

// --- derived_differences -------------------------------------------------

#[test]
fn wp047_derived_differences() {
    let (a, b) = sides("derived/appears");
    assert_eq!(common::derived_differences(&a, &b), set(&[d_added(G)]));
    assert_eq!(common::derived_differences(&b, &a), set(&[d_removed(G)]));
    let (a, b) = sides("derived/disappears");
    assert_eq!(common::derived_differences(&a, &b), set(&[d_removed(G)]));
    let (a, b) = sides("derived/changes");
    assert_eq!(
        common::derived_differences(&a, &b),
        set(&[d_changed("X<int>::~X()"), d_changed("X<int>::~X() requires c<T>")])
    );
    let (a, b) = sides("derived/equal");
    assert!(common::derived_differences(&a, &b).is_empty());
}

// --- differ ----------------------------------------------------------------

/// `derived-added`: holds when only B has it, with and without
/// `exact derived`; fails otherwise whether or not the directive is
/// written.
#[test]
fn wp047_differ_derived_added() {
    let (a, b) = sides("derived/appears");
    // `f`'s body names `g<int>` on B only: `f` changes (WP-034).
    let base = [changed("f()")];
    for exact in [false, true] {
        assert!(ok(judge_differ(&set(&[changed("f()"), d_added(G)]), exact, &a, &b)).is_empty());
        // The wrong way round, or on an instantiation both have.
        let e = err(judge_differ(&set(&[changed("f()"), d_removed(G)]), exact, &a, &b));
        assert!(e.contains("DerivedRemoved(\"g<int>()\")"), "{e}");
    }
    let (a2, b2) = sides("derived/equal");
    for exact in [false, true] {
        let e =
            err(judge_differ(&set(&[added("z"), changed("<order>"), d_added(G)]), exact, &a2, &b2));
        assert!(e.contains("expected but not seen: [DerivedAdded(\"g<int>()\")]"), "{e}");
    }
    // Without the directive the instantiation is free.
    assert_eq!(ok(judge_differ(&set(&base), false, &a, &b)), ["derived-added g<int>()"]);
}

/// `derived-removed`, the same way.
#[test]
fn wp047_differ_derived_removed() {
    let (a, b) = sides("derived/disappears");
    for exact in [false, true] {
        assert!(ok(judge_differ(&set(&[changed("f()"), d_removed(G)]), exact, &a, &b)).is_empty());
        let e = err(judge_differ(&set(&[changed("f()"), d_added(G)]), exact, &a, &b));
        assert!(e.contains("expected but not seen: [DerivedAdded(\"g<int>()\")]"), "{e}");
    }
    assert_eq!(
        ok(judge_differ(&set(&[changed("f()")]), false, &a, &b)),
        ["derived-removed g<int>()"]
    );
}

/// An instantiation both sides have that changes: `derived-changed`
/// holds with and without the directive, `derived-added` and
/// `derived-removed` do not.
#[test]
fn wp047_differ_derived_both_sides_changes() {
    let (a, b) = sides("derived/changes");
    let x = "X<int>::~X()";
    let base = changed("c");
    for exact in [false, true] {
        let both = set(&[base.clone(), d_changed(x), d_changed("X<int>::~X() requires c<T>")]);
        assert!(ok(judge_differ(&both, exact, &a, &b)).is_empty());
        for wrong in [d_added(x), d_removed(x)] {
            assert!(judge_differ(&set(&[base.clone(), wrong]), exact, &a, &b).is_err());
        }
    }
}

/// An instantiation both sides have that is equal: no `derived-*` line
/// holds, and the directive with none written holds.
#[test]
fn wp047_differ_derived_both_sides_equal() {
    let (a, b) = sides("derived/equal");
    assert!(ok(judge_differ(&set(&[added("z"), changed("<order>")]), true, &a, &b)).is_empty());
    for exact in [false, true] {
        for wrong in [d_added(G), d_removed(G), d_changed(G)] {
            assert!(
                judge_differ(&set(&[added("z"), changed("<order>"), wrong]), exact, &a, &b)
                    .is_err()
            );
        }
    }
}

/// `exact derived` with one unnamed derived difference fails and names it.
#[test]
fn wp047_exact_derived_names_the_unnamed() {
    let (a, b) = sides("derived/appears");
    let e = err(judge_differ(&set(&[changed("f()")]), true, &a, &b));
    assert_eq!(e, "expected but not seen: []\nseen but not expected: [DerivedAdded(\"g<int>()\")]");
    let (a, b) = sides("derived/disappears");
    let e = err(judge_differ(&set(&[changed("f()")]), true, &a, &b));
    assert_eq!(
        e,
        "expected but not seen: []\nseen but not expected: [DerivedRemoved(\"g<int>()\")]"
    );
    let (a, b) = sides("derived/changes");
    let e = err(judge_differ(&set(&[changed("c")]), true, &a, &b));
    assert!(
        e.contains(
            "seen but not expected: [DerivedChanged(\"X<int>::~X()\"), \
             DerivedChanged(\"X<int>::~X() requires c<T>\")]"
        ),
        "{e}"
    );
}

/// `derived-changed` without the directive behaves as before WP-047: it
/// holds when the derived units under a key of its name differ in any
/// way, presence included, and not when they are equal.
#[test]
fn wp047_derived_changed_as_before() {
    // Presence.
    let (a, b) = sides("derived/appears");
    assert_eq!(
        ok(judge_differ(&set(&[changed("f()"), d_changed(G)]), false, &a, &b)),
        ["derived-added g<int>() (written derived-changed)"]
    );
    let (a, b) = sides("derived/disappears");
    assert!(judge_differ(&set(&[changed("f()"), d_changed(G)]), false, &a, &b).is_ok());
    // Content.
    let (a, b) = sides("derived/changes");
    let lines =
        set(&[changed("c"), d_changed("X<int>::~X()"), d_changed("X<int>::~X() requires c<T>")]);
    assert!(ok(judge_differ(&lines, false, &a, &b)).is_empty());
    // Neither.
    let (a, b) = sides("derived/equal");
    let e = err(judge_differ(&set(&[added("z"), changed("<order>"), d_changed(G)]), false, &a, &b));
    assert!(e.contains("expected but not seen: [DerivedChanged(\"g<int>()\")]"), "{e}");
    // Under the directive, presence is not `derived-changed`.
    let (a, b) = sides("derived/appears");
    assert!(judge_differ(&set(&[changed("f()"), d_changed(G)]), true, &a, &b).is_err());
}

// --- demand ----------------------------------------------------------------

fn demand(rel: &str, lines: &[Expect], exact: bool) -> Result<(), String> {
    let (a, b) = sides(rel);
    judge_demand(&set(lines), exact, &a, &b)
}

#[test]
fn wp047_demand_derived_added() {
    for exact in [false, true] {
        demand("demand/appears", &[added("f()"), d_added(G)], exact).unwrap();
    }
    // Free without the directive, named under it.
    demand("demand/appears", &[added("f()")], false).unwrap();
    let e = demand("demand/appears", &[added("f()")], true).unwrap_err();
    assert!(e.contains("seen but not expected: [DerivedAdded(\"g<int>()\")]"), "{e}");
    // A line that does not hold fails with or without the directive.
    for exact in [false, true] {
        let e = demand("derived/equal", &[added("z"), d_added(G)], exact).unwrap_err();
        assert!(e.contains("expected but not seen: [DerivedAdded(\"g<int>()\")]"), "{e}");
    }
}

#[test]
fn wp047_demand_refuses_other_derived_lines() {
    for line in [d_removed(G), d_changed(G)] {
        let e = demand("demand/appears", &[added("f()"), line], false).unwrap_err();
        assert!(e.contains("a demand test has `added` and `derived-added` lines only"), "{e}");
    }
    // A derived unit only A has: the declared check fails anyway; under
    // the directive the derived one is named too.
    let e = demand("demand/removes", &[], true).unwrap_err();
    assert!(e.contains("DerivedRemoved(\"g<int>()\")"), "{e}");
}

// --- equal and indeterminate -------------------------------------------------

/// A derived unit only one side has fails `equal` (as before WP-047: the
/// groups differ), naming it.
#[test]
fn wp047_equal_one_sided_derived() {
    let (a, b) = sides("demand/appears");
    let e = judge_equal(&a, &b).unwrap_err();
    assert!(e.lines().any(|l| l == "only in B: g<int>()"), "{e}");
    let (a, b) = sides("demand/removes");
    let e = judge_equal(&a, &b).unwrap_err();
    assert!(e.lines().any(|l| l == "only in A: g<int>()"), "{e}");
}

/// `equal` reads no line and `indeterminate` only `indeterminate` lines:
/// any other line, and the directive, is an error naming it. The
/// functions are the ones the runners call.
#[test]
fn wp047_equal_and_indeterminate_refuse_other_lines() {
    let e = common::pairs::equal(&fixture("expect/equal-derived")).unwrap_err();
    assert_eq!(e, "`derived-added x`: not a line of an equal test");
    let e = common::pairs::equal(&fixture("expect/equal-line")).unwrap_err();
    assert_eq!(e, "`changed x`: not a line of an equal test");
    let e = common::pairs::indeterminate(&fixture("expect/indeterminate-derived")).unwrap_err();
    assert_eq!(e, "`exact derived`: not a line of an indeterminate test");
    let e = common::pairs::indeterminate(&fixture("expect/indeterminate-line")).unwrap_err();
    assert_eq!(e, "`changed y`: not a line of an indeterminate test");
}

// --- the runners' functions over fixture directories --------------------------

/// `pairs::differ` reads `exact derived` from the file: the same lines
/// hold without it and fail with it.
#[test]
fn wp047_differ_reads_the_directive() {
    let loose = common::pairs::differ(&fixture("file/differ-loose")).unwrap();
    assert_eq!(loose, ["derived-added g<int>() (written derived-changed)"]);
    let e = common::pairs::differ(&fixture("file/differ-exact")).unwrap_err();
    assert!(e.contains("expected but not seen: [DerivedChanged(\"g<int>()\")]"), "{e}");
    assert!(e.contains("seen but not expected: [DerivedAdded(\"g<int>()\")]"), "{e}");
    assert!(common::pairs::differ(&fixture("file/differ-exact-ok")).unwrap().is_empty());
    let e = common::pairs::differ(&fixture("file/differ-only-directive")).unwrap_err();
    assert_eq!(e, "a differ test needs at least one line in expect.txt besides `exact derived`");
}

/// A `differ` test with only `derived-*` lines is still held to its
/// declared differences.
#[test]
fn wp047_differ_derived_lines_only_still_declared() {
    let e = common::pairs::differ(&fixture("file/differ-declared")).unwrap_err();
    assert_eq!(e, "expected but not seen: []\nseen but not expected: [Added(\"f()\")]");
}

/// `pairs::demand` reads `exact derived` from the file.
#[test]
fn wp047_demand_reads_the_directive() {
    common::pairs::demand(&fixture("file/demand-loose")).unwrap();
    let e = common::pairs::demand(&fixture("file/demand-exact")).unwrap_err();
    assert!(e.contains("seen but not expected: [DerivedAdded(\"g<int>()\")]"), "{e}");
}

// --- expect.txt --------------------------------------------------------------

#[test]
fn wp047_expect_lines_and_directive() {
    let dir = fixture("expect/exact");
    assert_eq!(
        common::expect(&dir).unwrap(),
        set(&[changed("f"), d_added("a"), d_removed("b"), d_changed("c")])
    );
    assert!(common::exact_derived(&dir).unwrap());
    assert!(!common::exact_derived(&fixture("derived/appears")).unwrap());
    let twice = fixture("expect/twice");
    assert_eq!(
        common::exact_derived(&twice).unwrap_err(),
        "expect.txt:3: a second `exact derived`"
    );
    assert!(common::expect(&twice).is_err());
    let e = common::expect(&fixture("expect/unknown")).unwrap_err();
    assert_eq!(e, "expect.txt:1: unknown word `frobbed`");
    // The directive is exactly `exact derived`.
    let e = common::exact_derived(&fixture("expect/trailing")).unwrap_err();
    assert_eq!(e, "expect.txt:1: unknown word `exact`");
    let e = common::expect(&fixture("expect/twice-line")).unwrap_err();
    assert_eq!(e, "expect.txt:3: `changed f` a second time");
    for e in [d_added("a b"), d_removed("b"), d_changed("c")] {
        let line = e.line();
        assert_eq!(e.name(), line.split_once(' ').unwrap().1);
    }
}

// --- the pool ------------------------------------------------------------------

/// What the pool fixtures do: `what.txt` says.
fn pool_work(dir: &Path) -> Result<Vec<String>, String> {
    let what = std::fs::read_to_string(dir.join("what.txt")).map_err(|e| e.to_string())?;
    // Uneven work, so that the threads finish out of order.
    let n = dir.file_name().map_or(0, |n| n.as_encoded_bytes()[0]);
    std::thread::sleep(std::time::Duration::from_millis(u64::from(n % 3) * 15));
    match what.trim() {
        "ok" => Ok(Vec::new()),
        "note" => Ok(vec!["one note".into(), "another".into()]),
        "fail" => Err(format!("{} fails", common::rel(dir))),
        "panic" => panic!("the work of {} panics", common::rel(dir)),
        other => Err(format!("unknown `{other}`")),
    }
}

#[test]
fn wp047_pool_report_independent_of_threads() {
    let base = fixture("pool");
    let dirs: Vec<PathBuf> = ["a", "b", "c", "d", "e", "f"].iter().map(|d| base.join(d)).collect();
    let reports: Vec<_> = [1, 2, 8]
        .into_iter()
        .map(|n| common::run_dirs("pool", &dirs, n, pool_work).report())
        .collect();
    let (text, failure) = &reports[0];
    assert_eq!(
        text,
        "runner/pool/e: one note\nrunner/pool/e: another\nskipped (pending): runner/pool/c\n\
         pool: 5 run, 1 skipped, 3 failed\n"
    );
    assert_eq!(
        failure.as_deref(),
        Some(
            "3 of 5 failed:\nrunner/pool/b: runner/pool/b fails\n\n\
             runner/pool/d: panicked: the work of runner/pool/d panics\n\n\
             runner/pool/f: runner/pool/f fails"
        )
    );
    assert_eq!(reports[1], reports[0]);
    assert_eq!(reports[2], reports[0]);
}

/// A panicking directory has its time too, and `slowest` breaks ties by
/// directory.
#[test]
fn wp047_pool_times() {
    let base = fixture("pool");
    let dirs: Vec<PathBuf> = ["a", "b", "c", "d", "e", "f"].iter().map(|d| base.join(d)).collect();
    let o = common::run_dirs("pool", &dirs, 3, pool_work);
    let timed: Vec<&str> = o.times.iter().map(|(d, _)| d.as_str()).collect();
    assert_eq!(
        timed,
        ["runner/pool/a", "runner/pool/b", "runner/pool/d", "runner/pool/e", "runner/pool/f"]
    );
    let ms = std::time::Duration::from_millis;
    let o = common::Outcome {
        root: "t".into(),
        ran: 4,
        skipped: Vec::new(),
        failures: Vec::new(),
        notes: Vec::new(),
        times: vec![
            ("d".into(), ms(5)),
            ("b".into(), ms(9)),
            ("c".into(), ms(5)),
            ("a".into(), ms(5)),
        ],
    };
    let names: Vec<String> = o.slowest(3).into_iter().map(|(d, _)| d).collect();
    assert_eq!(names, ["b", "a", "c"]);
}

#[test]
fn wp047_pool_size() {
    use std::ffi::OsStr;
    let default = common::pool_size_from(None).unwrap();
    assert!((1..=4).contains(&default), "{default}");
    assert_eq!(common::pool_size_from(Some(OsStr::new("3"))), Ok(3));
    for bad in ["0", "x", "", "-1", "+2", " 2"] {
        let e = common::pool_size_from(Some(OsStr::new(bad))).unwrap_err();
        assert_eq!(e, format!("SLIP_TEST_THREADS={bad}: not a positive number"));
    }
}

/// `pool_map` keeps the items' order and turns a panic into that item's
/// error.
#[test]
fn wp047_pool_map() {
    let items: Vec<u32> = (0..50).collect();
    for n in [1, 3, 8, 64] {
        let out = common::pool_map(&items, n, |i| {
            assert!(*i != 7, "seven");
            i * 2
        });
        for (i, r) in items.iter().zip(&out) {
            if *i == 7 {
                assert_eq!(r.as_ref().unwrap_err(), "seven");
            } else {
                assert_eq!(*r.as_ref().unwrap(), i * 2);
            }
        }
    }
    assert!(common::pool_map(&[] as &[u32], 4, |i| *i).is_empty());
}

// --- the report mode -------------------------------------------------------------

/// What `SLIP_DIGEST_DERIVED=report` prints: the `differ` runner's own
/// functions (`report_mode`, `differ_noted`, `report_summary`) over
/// fixture directories. A passing directory's unnamed derived differences
/// are notes, the run passes, and the summary counts them by kind and by
/// directory; a failing directory gives no notes.
#[test]
fn wp047_report_mode() {
    use common::pairs::{differ_noted, report_mode, report_summary};
    use std::ffi::OsStr;
    assert!(report_mode(Some(OsStr::new("report"))));
    assert!(!report_mode(None));
    assert!(!report_mode(Some(OsStr::new("1"))));
    let dirs = [
        fixture("file/differ-loose"),
        fixture("file/differ-report-changes"),
        fixture("file/differ-exact-ok"),
    ];
    let outcome = common::run_dirs("report", &dirs, 2, |d| differ_noted(d, true));
    let (text, failure) = outcome.report();
    assert_eq!(failure, None);
    assert_eq!(
        text,
        "runner/file/differ-loose: unnamed derived-added g<int>() (written derived-changed)\n\
         runner/file/differ-report-changes: unnamed derived-changed X<int>::~X()\n\
         runner/file/differ-report-changes: unnamed derived-changed X<int>::~X() requires c<T>\n\
         report: 3 run, 0 skipped, 0 failed\n"
    );
    assert_eq!(
        report_summary(&outcome),
        "SLIP_DIGEST_DERIVED=report: 3 unnamed derived differences in 2 directories\n\
         \x20      1  derived-added (written derived-changed)\n\
         \x20      2  derived-changed\n\
         the directories with the most:\n\
         \x20      2  runner/file/differ-report-changes\n\
         \x20      1  runner/file/differ-loose\n"
    );
    // Without report mode there are no notes.
    let quiet = common::run_dirs("report", &dirs, 2, |d| differ_noted(d, false));
    assert!(quiet.notes.is_empty());
    // A failing directory gives its failure and no notes.
    let failing =
        common::run_dirs("report", &[fixture("file/differ-exact")], 1, |d| differ_noted(d, true));
    assert!(failing.notes.is_empty() && failing.failures.len() == 1);
}
