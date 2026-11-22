//! Long statement spines (WP-034, `src/stmt.rs` "The walk"): an `else if`
//! chain and a run of `case` labels 20 000 long, and a block of 100 000
//! statements, digest without a unit indeterminate for a cause of their
//! own, in work linear in the length.

mod common;

use std::time::{Duration, Instant};

use slip_digest::{Cause, Detail, Input, Options, Output};
use slip_model::keys::RuleTable;

/// `if (0) ; else if (1) ; … else ;`, `n` `if`s: each one's `else` is
/// the next. (Kiln's parse of the chain is quadratic, about 1.3 s for
/// 4 000 in the test profile, and slower with a name in each condition;
/// only the walk is timed.)
fn else_ifs(n: usize) -> String {
    let mut s = String::from("void f(int x) {\n");
    for i in 0..n {
        if i > 0 {
            s.push_str(" else ");
        }
        s.push_str(&format!("if ({i}) ;"));
    }
    s.push_str(" else ;\n}\n");
    s
}

/// `switch (x) { case 0: case 1: … ; }`, `n` labels: each one's statement
/// is the next.
fn case_labels(n: usize) -> String {
    let mut s = String::from("void f(int x) {\nswitch (x) {\n");
    for i in 0..n {
        s.push_str(&format!("case {i}:\n"));
    }
    s.push_str(";\n}\n}\n");
    s
}

/// `{ x = 0; x = 1; … }`, `n` statements in one block.
fn statements(n: usize) -> String {
    let mut s = String::from("void f(int &x) {\n");
    for i in 0..n {
        s.push_str(&format!("x = {i};\n"));
    }
    s.push_str("}\n");
    s
}

/// The output of digesting `src` (the same in each of three walks), the
/// fastest walk's time, and `digest.bytes`.
fn digest(src: &str) -> (Output, Duration, u64) {
    let files = [("main.cpp", src)];
    let snippet = slip_kiln::snippet::Snippet {
        files: &files,
        main: "main.cpp",
        include: &[""],
        std: "c++20",
    };
    slip_kiln::snippet::parse(&snippet, |p| {
        assert_eq!(p.errors, 0, "{}", p.diagnostics);
        let (ast, sm) = (p.ast, p.sm);
        common::on_walk_stack(|| {
            let path = |_| Vec::new();
            let input = Input { ast, sm, subject_path: &path };
            let rules = RuleTable::empty();
            let options = Options { rules: &rules, detail: Detail::All, names: true };
            let mut best = Duration::MAX;
            let mut out: Option<Output> = None;
            for _ in 0..3 {
                let t = Instant::now();
                let o = slip_digest::digest(&input, &options);
                best = best.min(t.elapsed());
                if let Some(before) = &out {
                    assert!(before.digests == o.digests, "the digests of two walks differ");
                }
                out = Some(o);
            }
            let out = out.expect("three walks");
            let bytes =
                out.counters.iter().find(|c| c.name == "digest.bytes").map_or(0, |c| c.value);
            (out, best, bytes)
        })
    })
    .expect("parses")
}

/// No unit is indeterminate for a cause other than `Pending`.
fn determinate(o: &Output) {
    let hard: Vec<&Cause> =
        o.causes.iter().map(|(_, c)| c).filter(|c| !matches!(c, Cause::Pending(_))).collect();
    assert!(hard.is_empty(), "indeterminate: {hard:?}");
}

fn linear(make: fn(usize) -> String, small_n: usize) {
    let (small, t_small, b_small) = digest(&make(small_n));
    let (large, t_large, b_large) = digest(&make(small_n * 10));
    determinate(&small);
    determinate(&large);
    // Ten times the length: the bytes written grow by about ten, and the
    // time by well under the hundred a quadratic walk would take.
    assert!(
        b_large >= b_small * 9 && b_large <= b_small * 11,
        "digest.bytes {b_small} -> {b_large}"
    );
    let limit = t_small * 25 + Duration::from_millis(20);
    assert!(t_large <= limit, "time {t_small:?} -> {t_large:?}");
}

#[test]
fn an_else_if_chain_is_linear() {
    linear(else_ifs, 2_000);
}

#[test]
fn a_run_of_case_labels_is_linear() {
    linear(case_labels, 2_000);
}

#[test]
fn a_long_block_is_linear() {
    linear(statements, 10_000);
}
