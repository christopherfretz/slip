//! Long chains of closures (WP-040 "Linear and bounded", `lib.rs` "What
//! bounds the stack"): 20 000 lambdas one after another in a body, each
//! calling the one before, and 500 functions each returning a closure
//! that captures the previous one's result, digest in work linear in the
//! length; lambdas nested in one another reach `walk::DEPTH_LIMIT` at a
//! stated depth and are indeterminate there, well formed.

mod common;

use std::fmt::Write as _;
use std::time::{Duration, Instant};

use slip_digest::{Cause, Detail, Input, Options, Output};
use slip_model::keys::RuleTable;

/// `auto l0 = [] { … }; auto l1 = [&] { return l0() + 1; }; …` in one
/// body: `n` closures of one owner.
fn chained(n: usize) -> String {
    let mut s = String::from("int f() {\n  auto l0 = [] { return 0; };\n");
    for i in 1..n {
        let _ = writeln!(s, "  auto l{i} = [&] {{ return l{}() + 1; }};", i - 1);
    }
    let _ = write!(s, "  return l{}();\n}}\n", n - 1);
    s
}

/// `auto f0() { return [] { … }; } auto f1() { return [p = f0()] { … }; } …`:
/// `n` owners, each closure holding the previous one's.
fn returning(n: usize) -> String {
    let mut s = String::from("auto f0() { return [] { return 0; }; }\n");
    for i in 1..n {
        let _ = writeln!(s, "auto f{i}() {{ return [p = f{}()] {{ return p() + 1; }}; }}", i - 1);
    }
    s
}

/// `[] { return [] { … return 0; }(); }()`, `n` lambdas deep, in a body.
fn nested(n: usize) -> String {
    format!("int f() {{ return {}0{}; }}\n", "[] { return ".repeat(n), "; }()".repeat(n))
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
            let options = Options {
                rules: &rules,
                read_only: slip_digest::ReadOnly::NO_FILE,
                detail: Detail::All,
                names: true,
            };
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

/// The causes other than `Pending` of the units of `o`.
fn hard(o: &Output) -> Vec<Cause> {
    o.causes.iter().map(|(_, c)| *c).filter(|c| !matches!(c, Cause::Pending(_))).collect()
}

/// As `tests/expr_chains.rs`: the chain of `small_n` and of
/// `small_n * factor` links, determinate; the bytes grow by `factor`
/// (within one either way), the time by at most `2.5 * factor` with 20 ms
/// of slack.
fn linear(make: fn(usize) -> String, small_n: usize, factor: u64) {
    let large_n = small_n * usize::try_from(factor).expect("a small factor");
    let (small, t_small, b_small) = digest(&make(small_n));
    let (large, t_large, b_large) = digest(&make(large_n));
    assert!(hard(&small).is_empty(), "indeterminate: {:?}", hard(&small));
    assert!(hard(&large).is_empty(), "indeterminate: {:?}", hard(&large));
    assert!(
        b_large >= b_small * (factor - 1) && b_large <= b_small * (factor + 1),
        "digest.bytes {b_small} -> {b_large}"
    );
    let limit = t_small * u32::try_from(factor * 5 / 2).expect("a small factor")
        + Duration::from_millis(20);
    assert!(t_large <= limit, "time {t_small:?} -> {t_large:?}");
}

#[test]
fn chained_lambdas_are_linear() {
    linear(chained, 2_000, 10);
}

// 500 links, not 20 000: Kiln's parse of this chain is not linear (2 000
// links take minutes in the test profile, `docs/wp/WP-040.md` Handback).
#[test]
fn closures_returned_through_a_chain_of_functions_are_linear() {
    linear(returning, 100, 5);
}

/// Nested lambdas: each level is a closure's stream and its call
/// operator's, two levels of `DEPTH_LIMIT`; 40 levels are determinate,
/// 150 are `walk/depth` (the unit, well formed: it has a digest), in time
/// linear in the depth.
#[test]
fn nested_lambdas_reach_the_depth_limit() {
    let (shallow, _, _) = digest(&nested(40));
    assert!(hard(&shallow).is_empty(), "indeterminate: {:?}", hard(&shallow));
    let (deep, _, _) = digest(&nested(150));
    assert_eq!(hard(&deep), vec![Cause::Unaccounted("walk/depth")]);
    // The lambda in the deepest place a numbering can be asked for: the
    // nested closures named from another unit.
    let src = format!("{}auto g() {{ return f(); }}\n", nested(150));
    let (other, _, _) = digest(&src);
    assert!(
        hard(&other).iter().all(|c| *c == Cause::Unaccounted("walk/depth")),
        "{:?}",
        hard(&other)
    );
}

/// `auto f() { return [] { return [] { … return [] {}; }(); }(); }`: `f`
/// returns the innermost of `n` nested closures, which `g` names through
/// `f`'s type: past the depth limit `f`'s numbering is cut, and the
/// closure's number carries `walk/depth`, not "undeclared".
#[test]
fn the_innermost_closure_named_from_another_unit() {
    let n = 150;
    let src = format!(
        "auto f() {{ return {}[] {{}}{}; }}\nauto g() {{ return f(); }}\n",
        "[] { return ".repeat(n),
        "; }()".repeat(n)
    );
    let (out, _, _) = digest(&src);
    let hard = hard(&out);
    assert!(
        !hard.is_empty() && hard.iter().all(|c| *c == Cause::Unaccounted("walk/depth")),
        "{hard:?}"
    );
}
