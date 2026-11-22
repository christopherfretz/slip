//! Long chains (WP-031 "What bounds the stack", `lib.rs` "What bounds the
//! stack"): an operator chain and a member-call chain 20 000 levels deep
//! digest without a unit indeterminate for a cause of their own, in work
//! linear in the length.

mod common;

use std::time::{Duration, Instant};

use slip_digest::{Cause, Detail, Input, Options, Output};
use slip_model::keys::RuleTable;

/// `a + a + … + a`, `n` operators: a left spine `n` levels deep.
fn operators(n: usize) -> String {
    let mut s = String::from("int a;\nint v = a");
    for _ in 0..n {
        s.push_str(" + a");
    }
    s.push_str(";\n");
    s
}

/// `c.f().f()…f()`, `n` calls: each call's object is the call before it.
fn member_calls(n: usize) -> String {
    let mut s = String::from("struct C { C &f(); };\nC c;\nC &v = c");
    for _ in 0..n {
        s.push_str(".f()");
    }
    s.push_str(";\n");
    s
}

/// `((a ?: a) ?: a) …`, `n` operators: each one's common expression is
/// the one before it, which an opaque value names again (§7 "Opaque
/// values"). Parenthesised: Kiln limits bracket nesting, so `n` stays
/// small.
fn left_conditionals(n: usize) -> String {
    let mut e = String::from("a");
    for _ in 0..n {
        e = format!("({e} ?: a)");
    }
    format!("int a;\nint v = {e};\n")
}

/// `a ?: a ?: … ?: a`, `n` operators: each one's false arm is the next.
fn right_conditionals(n: usize) -> String {
    let mut s = String::from("int a;\nint v = a");
    for _ in 0..n {
        s.push_str(" ?: a");
    }
    s.push_str(";\n");
    s
}

/// The output of digesting `src`, the fastest of three walks' time, and
/// `digest.bytes`.
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
            let mut out = None;
            for _ in 0..3 {
                let t = Instant::now();
                let o = slip_digest::digest(&input, &options);
                best = best.min(t.elapsed());
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
    // time by well under the hundred a quadratic walk would take. (Kiln's
    // own parse of the member-call chain is quadratic; only the walk is
    // timed.)
    assert!(
        b_large >= b_small * 9 && b_large <= b_small * 11,
        "digest.bytes {b_small} -> {b_large}"
    );
    let limit = t_small * 25 + Duration::from_millis(20);
    assert!(t_large <= limit, "time {t_small:?} -> {t_large:?}");
}

#[test]
fn an_operator_chain_is_linear() {
    linear(operators, 2_000);
}

#[test]
fn a_member_call_chain_is_linear() {
    linear(member_calls, 2_000);
}

#[test]
fn left_nested_conditionals_are_linear() {
    linear(left_conditionals, 20);
}

#[test]
fn right_nested_conditionals_are_linear() {
    linear(right_conditionals, 2_000);
}
