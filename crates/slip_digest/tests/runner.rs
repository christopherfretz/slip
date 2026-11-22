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
const G: &str = "g<int>";

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
    assert_eq!(common::derived_differences(&a, &b), set(&[d_changed("X<int>::~X()")]));
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
    let base = [changed("f")];
    for exact in [false, true] {
        assert!(ok(judge_differ(&set(&[changed("f"), d_added(G)]), exact, &a, &b)).is_empty());
        // The wrong way round, or on an instantiation both have.
        let e = err(judge_differ(&set(&[changed("f"), d_removed(G)]), exact, &a, &b));
        assert!(e.contains("DerivedRemoved(\"g<int>\")"), "{e}");
    }
    let (a2, b2) = sides("derived/equal");
    for exact in [false, true] {
        let e = err(judge_differ(&set(&[added("z"), d_added(G)]), exact, &a2, &b2));
        assert!(e.contains("expected but not seen: [DerivedAdded(\"g<int>\")]"), "{e}");
    }
    // Without the directive the instantiation is free.
    assert_eq!(ok(judge_differ(&set(&base), false, &a, &b)), ["derived-added g<int>"]);
}

/// `derived-removed`, the same way.
#[test]
fn wp047_differ_derived_removed() {
    let (a, b) = sides("derived/disappears");
    for exact in [false, true] {
        assert!(ok(judge_differ(&set(&[changed("f"), d_removed(G)]), exact, &a, &b)).is_empty());
        let e = err(judge_differ(&set(&[changed("f"), d_added(G)]), exact, &a, &b));
        assert!(e.contains("expected but not seen: [DerivedAdded(\"g<int>\")]"), "{e}");
    }
    assert_eq!(ok(judge_differ(&set(&[changed("f")]), false, &a, &b)), ["derived-removed g<int>"]);
}

/// An instantiation both sides have that changes: `derived-changed`
/// holds with and without the directive, `derived-added` and
/// `derived-removed` do not.
#[test]
fn wp047_differ_derived_both_sides_changes() {
    let (a, b) = sides("derived/changes");
    let x = "X<int>::~X()";
    let base = changed("X::~X<T>()");
    for exact in [false, true] {
        assert!(ok(judge_differ(&set(&[base.clone(), d_changed(x)]), exact, &a, &b)).is_empty());
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
    assert!(ok(judge_differ(&set(&[added("z")]), true, &a, &b)).is_empty());
    for exact in [false, true] {
        for wrong in [d_added(G), d_removed(G), d_changed(G)] {
            assert!(judge_differ(&set(&[added("z"), wrong]), exact, &a, &b).is_err());
        }
    }
}

/// `exact derived` with one unnamed derived difference fails and names it.
#[test]
fn wp047_exact_derived_names_the_unnamed() {
    let (a, b) = sides("derived/appears");
    let e = err(judge_differ(&set(&[changed("f")]), true, &a, &b));
    assert_eq!(e, "expected but not seen: []\nseen but not expected: [DerivedAdded(\"g<int>\")]");
    let (a, b) = sides("derived/changes");
    let e = err(judge_differ(&set(&[changed("X::~X<T>()")]), true, &a, &b));
    assert!(e.contains("seen but not expected: [DerivedChanged(\"X<int>::~X()\")]"), "{e}");
}

/// `derived-changed` without the directive behaves as before WP-047: it
/// holds when the derived units under a key of its name differ in any
/// way, presence included, and not when they are equal.
#[test]
fn wp047_derived_changed_as_before() {
    // Presence.
    let (a, b) = sides("derived/appears");
    assert_eq!(
        ok(judge_differ(&set(&[changed("f"), d_changed(G)]), false, &a, &b)),
        ["derived-added g<int> (written derived-changed)"]
    );
    let (a, b) = sides("derived/disappears");
    assert!(judge_differ(&set(&[changed("f"), d_changed(G)]), false, &a, &b).is_ok());
    // Content.
    let (a, b) = sides("derived/changes");
    let lines = set(&[changed("X::~X<T>()"), d_changed("X<int>::~X()")]);
    assert!(ok(judge_differ(&lines, false, &a, &b)).is_empty());
    // Neither.
    let (a, b) = sides("derived/equal");
    let e = err(judge_differ(&set(&[added("z"), d_changed(G)]), false, &a, &b));
    assert!(e.contains("expected but not seen: [DerivedChanged(\"g<int>\")]"), "{e}");
    // Under the directive, presence is not `derived-changed`.
    let (a, b) = sides("derived/appears");
    assert!(judge_differ(&set(&[changed("f"), d_changed(G)]), true, &a, &b).is_err());
}

// --- demand ----------------------------------------------------------------

fn demand(rel: &str, lines: &[Expect], exact: bool) -> Result<(), String> {
    let (a, b) = sides(rel);
    judge_demand(&set(lines), exact, &a, &b)
}

#[test]
fn wp047_demand_derived_added() {
    for exact in [false, true] {
        demand("demand/appears", &[added("f"), d_added(G)], exact).unwrap();
    }
    // Free without the directive, named under it.
    demand("demand/appears", &[added("f")], false).unwrap();
    let e = demand("demand/appears", &[added("f")], true).unwrap_err();
    assert!(e.contains("seen but not expected: [DerivedAdded(\"g<int>\")]"), "{e}");
    // A line that does not hold fails with or without the directive.
    for exact in [false, true] {
        let e = demand("derived/equal", &[added("z"), d_added(G)], exact).unwrap_err();
        assert!(e.contains("expected but not seen: [DerivedAdded(\"g<int>\")]"), "{e}");
    }
    // `derived/equal` itself is a demand pair with nothing derived moving.
    demand("derived/equal", &[added("z")], true).unwrap();
}

#[test]
fn wp047_demand_refuses_other_derived_lines() {
    for line in [d_removed(G), d_changed(G)] {
        let e = demand("demand/appears", &[added("f"), line], false).unwrap_err();
        assert!(e.contains("a demand test has `added` and `derived-added` lines only"), "{e}");
    }
    // A derived unit only A has: the declared check fails anyway; under
    // the directive the derived one is named too.
    let e = demand("demand/removes", &[], true).unwrap_err();
    assert!(e.contains("DerivedRemoved(\"g<int>\")"), "{e}");
}

// --- equal and indeterminate -------------------------------------------------

/// A derived unit only one side has fails `equal` (as before WP-047: the
/// groups differ), naming it.
#[test]
fn wp047_equal_one_sided_derived() {
    let (a, b) = sides("demand/appears");
    let e = judge_equal(&a, &b).unwrap_err();
    assert!(e.lines().any(|l| l == "only in B: g<int>"), "{e}");
    let (a, b) = sides("demand/removes");
    let e = judge_equal(&a, &b).unwrap_err();
    assert!(e.lines().any(|l| l == "only in A: g<int>"), "{e}");
}

#[test]
fn wp047_equal_and_indeterminate_refuse_derived_lines() {
    let e = common::pairs::equal(&fixture("expect/equal-derived")).unwrap_err();
    assert_eq!(e, "`derived-added x`: an equal test has no derived lines");
    let e = common::pairs::no_derived_lines(
        &fixture("expect/indeterminate-derived"),
        "an indeterminate test",
    )
    .unwrap_err();
    assert_eq!(e, "`exact derived`: an indeterminate test has no derived lines");
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
    assert_eq!(common::exact_derived(&twice).unwrap_err(), "expect.txt:3: a second `exact derived`");
    assert!(common::expect(&twice).is_err());
    for e in [changed("f"), d_added("a b"), d_removed("b"), d_changed("c")] {
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

/// What `SLIP_DIGEST_DERIVED=report` prints for a passing directory: its
/// unnamed derived differences as the lines that would name them, and
/// the directory passes.
#[test]
fn wp047_report_mode_lines() {
    let dirs = [fixture("derived/appears"), fixture("derived/disappears")];
    // These fixtures have no expect.txt: give `differ` the lines through a
    // closure over the sides, as `pairs::differ` does from the file.
    let outcome = common::run_dirs("report", &dirs, 2, |dir| {
        let a = common::digest_side(dir, Side::A)?;
        let b = common::digest_side(dir, Side::B)?;
        judge_differ(&set(&[changed("f")]), false, &a, &b)
    });
    let (text, failure) = outcome.report();
    assert_eq!(failure, None);
    assert_eq!(
        text,
        "runner/derived/appears: derived-added g<int>\n\
         runner/derived/disappears: derived-removed g<int>\n\
         report: 2 run, 0 skipped, 0 failed\n"
    );
}
