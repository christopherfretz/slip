//! `spec/api.md` §2.5 and §2.8 through the driver (`docs/wp/WP-089.md`
//! "Tests"): each failure kind with the smallest transform that causes it,
//! its message (the cause) and its place in the transform
//! (`spec/edits.md` §4.6); each budget at exactly what a run uses; an
//! uncaught `slip::error` with the host's refusal, a caught one, a failed
//! run with no effects, and after each failure a normal run that
//! completes. Also the errors of the worker: a cancelled run, a host
//! failure that retires the transform, a panic during a run. Decisions:
//! `spec/landed/api/WP-089.md`.

mod common;

use common::{Liar, failure, kind, mode, run, subject, with, with_meta};
use slip_kiln::session::Cancel;
use slip_script::driver::{Budgets, FailureKind, Outcome, SourcePlace};
use slip_script::fake::FakeSubject;

/// One transform; what it does is chosen by the subject (`mode::<m>`).
/// The tests name the lines of the causes by their text.
const MODES: &str = r#"
using u64 = unsigned long long;
constexpr int deref(int* p) { return *p; }
constexpr void spin(u64 n) { for (u64 i = 0; i < n; ++i) {} }
constexpr void hold() { char* p = new char[1 << 20]; delete[] p; }
constexpr int down(int n) { return n == 0 ? 0 : down(n - 1) + 1; }
struct mine { int code; };
namespace own { struct error { int n; }; }
struct derived : slip::error { constexpr derived(const slip::error& e) : slip::error(e) {} };
consteval bool is(const char* m) { return !slip::lookup(std::string("mode::") + m).empty(); }
consteval void misuse() { (void)slip::extent_of(slip::lookup("lib::foo")[0]); }
consteval void misuse_name() { (void)slip::extent_of(slip::info()); }
consteval void primitive() {
  u64 in[1] = {0};
  const char bin[1] = {0};
  u64 out[1] = {0};
  char bout[1] = {0};
  (void)__builtin_kiln_host_call(1u, in, 5, bin, 0, out, 1, bout, 1);
}
consteval void no_such_op() {
  u64 in[1] = {0};
  const char bin[1] = {0};
  u64 out[1] = {0};
  char bout[1] = {0};
  (void)slip::__access::__ok(__builtin_kiln_host_call(9999u, in, 0, bin, 0, out, 0, bout, 0));
}
consteval void transform() {
  slip::report(slip::severity::note, "ran");
  if (is("not_constant")) deref(nullptr);
  if (is("steps")) spin(1000000);
  if (is("memory")) hold();
  if (is("depth")) down(100);
  if (is("uncaught")) throw mine{3};
  if (is("refused")) misuse();
  if (is("caught")) { try { misuse(); } catch (const slip::error&) {} }
  if (is("primitive")) primitive();
  if (is("lie")) (void)slip::lookup("lie::lie");
  if (is("boom")) (void)slip::lookup("boom::boom");
  if (is("two")) { try { misuse(); } catch (const slip::error&) {} misuse_name(); }
  if (is("then_int")) { try { misuse(); } catch (const slip::error&) {} throw 42; }
  if (is("no_op")) no_such_op();
  if (is("rethrow_older")) {
    try { misuse(); } catch (const slip::error&) { try { misuse_name(); } catch (const slip::error&) {} throw; }
  }
  if (is("own_error")) { try { misuse(); } catch (const slip::error&) {} throw own::error{1}; }
  if (is("derived")) { try { misuse(); } catch (const slip::error& e) { throw derived(e); } }
}
"#;

/// The line of `needle` in `MODES` as the transform file has it (after
/// `with_meta`'s one line), from 1.
fn line_of(needle: &str) -> u32 {
    let i = MODES.find(needle).unwrap_or_else(|| panic!("{needle}"));
    u32::try_from(MODES[..i].matches('\n').count() + 2).unwrap()
}

/// A place's file name (the last component) and line.
fn at(p: Option<&SourcePlace>) -> Option<(String, u32)> {
    p.map(|p| {
        let f = String::from_utf8_lossy(&p.file).into_owned();
        (f.rsplit('/').next().unwrap_or_default().to_owned(), p.line)
    })
}

/// The reports of a completed run.
fn reports(o: &Outcome) -> usize {
    match o {
        Outcome::Completed(out) => out.reports.len(),
        Outcome::Failed(_) => panic!("the run failed: {o:?}"),
    }
}

/// The refusal of `misuse()`, as the host words it.
const EXTENT_OF: &str =
    "extent_of: expected a reference or an expression; given an entity (wrong_kind)";

// §2.8, `spec/edits.md` §4.6: each failure kind, its message (the cause,
// with no place in it) and its place (the line of the transform that
// caused it); then a normal run that completes (§2.4 rule 3: it starts
// from the compiled unit, and the failed run's report is not in it, §2.8
// rule 1).
#[test]
fn s2_8_each_failure_kind_its_cause_and_place_then_a_normal_run() {
    let small = Budgets::default();
    let cases: [(&str, Budgets, FailureKind, &str, &str); 7] = [
        ("not_constant", small, FailureKind::NotConstant, "null", "return *p;"),
        ("steps", Budgets { steps: 100_000, ..small }, FailureKind::StepBudget, "step", "i < n"),
        (
            "memory",
            Budgets { bytes: 1 << 16, ..small },
            FailureKind::MemoryBudget,
            "memory",
            "new char",
        ),
        ("depth", Budgets { depth: 16, ..small }, FailureKind::DepthLimit, "depth", "down(n - 1)"),
        (
            "uncaught",
            small,
            FailureKind::Uncaught,
            "unhandled exception of type 'mine'",
            "throw mine{3}",
        ),
        // Thrown in `<slip/meta.h>`: the place is the transform's call
        // that was refused, from Kiln's call stack of an uncaught
        // exception (KN5 of the Handback, in the pin since WP-066).
        (
            "refused",
            small,
            FailureKind::Uncaught,
            "an uncaught slip::error; the most recent refusal was: extent_of",
            "(void)slip::extent_of(slip::lookup",
        ),
        // §3.5 rule 5: Kiln's own check of the primitive's buffers is not
        // a host failure (§3.5 rule 4).
        (
            "primitive",
            small,
            FailureKind::NotConstant,
            "__builtin_kiln_host_call",
            "(void)__builtin_kiln_host_call(1u",
        ),
    ];
    let normal = subject();
    with("modes", &with_meta(MODES), &[], |l| {
        let (first, _) = run(l, &normal).unwrap();
        assert_eq!(reports(&first), 1, "a normal run");
        for (m, budgets, want, message, line) in cases {
            let s = mode(m);
            let (o, stats) = l.run(&s, budgets, &Cancel::new()).unwrap();
            let f = failure(&o);
            assert_eq!(f.kind, want, "{m}: {}", f.rendered);
            assert!(f.message.contains(message), "{m}: {:?}", f.message);
            assert!(!f.message.contains("modes.cpp"), "{m}: {:?}", f.message);
            assert_eq!(
                at(f.place.as_ref()),
                Some(("modes.cpp".to_owned(), line_of(line))),
                "{m}: {:?} {}",
                f.message,
                f.rendered
            );
            assert_eq!(f.refusal.is_some(), m == "refused", "{m}");
            assert!(f.rendered.contains("modes.cpp:"), "{m}: {}", f.rendered);
            assert!(stats.steps > 0 && stats.host_calls > 0, "{m}: {stats:?}");
            assert_eq!(l.retired(), None, "{m}: a failure of the transform does not retire it");
            let (after, _) = run(l, &normal).unwrap();
            assert_eq!(after, first, "{m}: the normal run after it is as the first");
        }
    });
}

// §2.5 rule 1, N3–N5 of the review: each budget is the limit itself. A
// run given exactly what it used completes; one less fails with the
// budget's kind.
#[test]
fn s2_5_rule1_each_budget_exactly() {
    with("exact", &with_meta(MODES), &[], |l| {
        let budget = |l: &mut slip_script::driver::Loaded<'_>, m: &str, b: Budgets| {
            let (o, used) = l.run(&mode(m), b, &Cancel::new()).unwrap();
            (kind(&o), used)
        };
        let (k, used) = budget(l, "depth", Budgets::default());
        assert_eq!(k, "completed");
        let steps = |n| Budgets { steps: n, ..Budgets::default() };
        assert_eq!(budget(l, "depth", steps(used.steps)).0, "completed", "{used:?}");
        assert_eq!(budget(l, "depth", steps(used.steps - 1)).0, "step-budget", "{used:?}");
        // A step budget under the bound of an array allocated is Kiln's
        // array-bound limit, not-constant (as clang's `-fconstexpr-steps`):
        // the memory transform is not used for steps.
        let (k, used) = budget(l, "memory", Budgets::default());
        assert_eq!(k, "completed");
        let bytes = |n| Budgets { bytes: n, ..Budgets::default() };
        assert_eq!(budget(l, "memory", bytes(used.peak_bytes)).0, "completed", "{used:?}");
        assert_eq!(budget(l, "memory", bytes(used.peak_bytes - 1)).0, "memory-budget", "{used:?}");
        // The depth `down(100)` needs, found once and pinned: it passes,
        // one less fails.
        let s = mode("depth");
        let mut depth = |d| {
            let (o, _) =
                l.run(&s, Budgets { depth: d, ..Budgets::default() }, &Cancel::new()).unwrap();
            kind(&o)
        };
        assert_eq!(depth(DEPTH), "completed", "depth {DEPTH}");
        assert_eq!(depth(DEPTH - 1), "depth-limit", "depth {}", DEPTH - 1);
    });
}

/// The depth a run of mode `depth` needs: `transform`, `down(100)` to
/// `down(0)`.
const DEPTH: u32 = 102;

// §2.8 rule 3, WP-089 rule 3: an uncaught `slip::error` is reported with
// the host's message; the refusal is the run's most recent and carries
// the place of the failed call's value in the subject; the failure's own
// place is in the transform (§4.6).
#[test]
fn s2_8_uncaught_slip_error_has_the_hosts_message_and_place() {
    let s = mode("refused");
    let foo_name = s.range_nth(slip_script::subject::File(1), "foo", 0);
    with("refused", &with_meta(MODES), &[], |l| {
        let (o, _) = run(l, &s).unwrap();
        let f = failure(&o);
        assert_eq!(f.kind, FailureKind::Uncaught);
        assert_eq!(
            f.message,
            format!("an uncaught slip::error; the most recent refusal was: {EXTENT_OF}")
        );
        // The refused call's place (`s2_8_the_place_of_a_refusal_is_the_call`).
        assert_eq!(
            at(f.place.as_ref()),
            Some(("refused.cpp".to_owned(), line_of("(void)slip::extent_of(slip::lookup")))
        );
        let refusal = f.refusal.as_ref().expect("the refusal");
        assert_eq!((refusal.errc, refusal.place), (2, Some(foo_name)));
        assert!(f.rendered.contains("unhandled exception of type 'error'"), "{}", f.rendered);
    });
}

// N9 of the review: with two refusals, a caught one and then an uncaught
// one, the refusal reported is the second.
#[test]
fn s2_8_rule3_of_two_refusals_the_most_recent() {
    with("two", &with_meta(MODES), &[], |l| {
        let (o, _) = run(l, &mode("two")).unwrap();
        let f = failure(&o);
        let refusal = f.refusal.as_ref().expect("a refusal");
        assert!(f.message.contains("the most recent refusal was: extent_of"), "{}", f.message);
        assert!(!f.message.ends_with("(wrong_kind)"), "not the first: {}", f.message);
        assert!(refusal.ordinal > 1, "{f:?}");
        assert!(
            f.message.ends_with("a null info where a value is required (invalid_handle)"),
            "{}",
            f.message
        );
    });
}

// N10 of the review: after a caught refusal, an exception of another type
// is reported as Kiln gives it, with no refusal.
#[test]
fn s2_8_rule3_another_type_after_a_caught_refusal() {
    with("then_int", &with_meta(MODES), &[], |l| {
        let (o, _) = run(l, &mode("then_int")).unwrap();
        let f = failure(&o);
        assert_eq!((f.kind, &f.refusal), (FailureKind::Uncaught, &None));
        assert!(f.message.starts_with("unhandled exception of type 'int'"), "{}", f.message);
        assert_eq!(at(f.place.as_ref()).map(|p| p.1), Some(line_of("throw 42")));
    });
}

// N14 of the review: a refusal of status 254 (an operation the host does
// not have) is named so in the message.
#[test]
fn s2_8_rule3_status_254_is_named() {
    with("no_op", &with_meta(MODES), &[], |l| {
        let (o, _) = run(l, &mode("no_op")).unwrap();
        let f = failure(&o);
        assert_eq!(f.kind, FailureKind::Uncaught);
        assert!(f.message.ends_with("(status 254 (no such operation))"), "{}", f.message);
    });
}

// §2.8: an exception of the transform's own type is reported as Kiln
// gives it, with no refusal; and one of its own named `error` is not
// taken for a `slip::error` when no call of the run was refused
// (decision 4 of the landed file).
#[test]
fn s2_8_uncaught_other_types() {
    let body = "struct error { int n; };\nconsteval void transform() { throw error{1}; }";
    with("own_error", &with_meta(body), &[], |l| {
        let (o, _) = run(l, &subject()).unwrap();
        let f = failure(&o);
        assert_eq!((f.kind, &f.refusal), (FailureKind::Uncaught, &None));
        assert!(f.message.starts_with("unhandled exception of type 'error'"), "{}", f.message);
        assert_eq!(at(f.place.as_ref()), Some(("own_error.cpp".to_owned(), 3)));
    });
}

// M3 of the review, since Kiln gives the call stack of an uncaught
// exception (KN5 of the Handback, in the pin since WP-066): the place of
// an uncaught `slip::error` is the line of the transform that made the
// refused call, not the entry.
#[test]
fn s2_8_the_place_of_a_refusal_is_the_call() {
    with("refused_at", &with_meta(MODES), &[], |l| {
        let (o, _) = run(l, &mode("refused")).unwrap();
        let f = failure(&o);
        assert_eq!(
            at(f.place.as_ref()),
            Some(("refused_at.cpp".to_owned(), line_of("(void)slip::extent_of")))
        );
    });
}

// M1 of the review: the three cases in which the rule "the most recent
// refusal, if the type is `slip::error`" was wrong (`spec/api.md` §2.8
// rule 3). Each asserts the right result. Kiln WP-2480 (in the pin since
// WP-066) names the type qualified, which fixes the second; the other two
// need Slip to read which object was thrown (WP-138).
#[test]
#[ignore = "needs: WP-138 (the thrown object, Kiln WP-2480)"]
fn m1_rethrowing_an_older_slip_error_reports_its_refusal() {
    with("rethrow_older", &with_meta(MODES), &[], |l| {
        let (o, _) = run(l, &mode("rethrow_older")).unwrap();
        let f = failure(&o);
        assert!(f.message.ends_with(EXTENT_OF), "the older refusal: {}", f.message);
    });
}

#[test]
fn m1_an_error_of_another_namespace_has_no_refusal() {
    with("own_ns", &with_meta(MODES), &[], |l| {
        let (o, _) = run(l, &mode("own_error")).unwrap();
        let f = failure(&o);
        assert_eq!((f.kind, &f.refusal), (FailureKind::Uncaught, &None), "{f:?}");
    });
}

#[test]
#[ignore = "needs: WP-138 (the thrown object, Kiln WP-2480)"]
fn m1_a_type_derived_from_slip_error_carries_its_refusal() {
    with("derived", &with_meta(MODES), &[], |l| {
        let (o, _) = run(l, &mode("derived")).unwrap();
        let f = failure(&o);
        assert!(f.refusal.is_some(), "{f:?}");
        assert!(f.message.ends_with(EXTENT_OF), "{}", f.message);
    });
}

// §2.8: a caught `slip::error` does not fail the run.
#[test]
fn s2_8_a_caught_slip_error_does_not_fail_the_run() {
    with("caught", &with_meta(MODES), &[], |l| {
        let (o, _) = run(l, &mode("caught")).unwrap();
        assert_eq!(kind(&o), "completed", "{o:?}");
        assert_eq!(reports(&o), 1);
    });
}

// §7.6 rule 6: a completed run carries the evaluator's three counters
// with the host's, sorted, each once.
#[test]
fn s7_6_rule6_the_evaluators_counters() {
    with("counters", &with_meta(MODES), &[], |l| {
        let (o, stats) = run(l, &subject()).unwrap();
        let Outcome::Completed(out) = o else { panic!("failed") };
        let value = |n: &str| out.counters.iter().find(|c| c.name == n).map(|c| c.value);
        assert_eq!(value("api.steps"), Some(stats.steps));
        assert_eq!(value("api.host_calls"), Some(stats.host_calls));
        assert_eq!(value("api.peak_bytes"), Some(stats.peak_bytes));
        assert_eq!(value("api.reports"), Some(1));
        assert!(out.counters.windows(2).all(|w| w[0] < w[1] && w[0].name != w[1].name));
        assert!(stats.wall_ns > 0);
    });
}

// §2.5 rule 4: a cancelled run has no result at all; once the token is
// reset the transform runs again.
#[test]
fn s2_5_rule4_a_cancelled_run_has_no_result() {
    with("cancel", &with_meta(MODES), &[], |l| {
        let cancel = Cancel::new();
        cancel.cancel();
        let e = l.run(&subject(), Budgets::default(), &cancel).unwrap_err();
        assert!(e.contains("cancelled"), "{e}");
        assert_eq!(l.retired(), None);
        cancel.reset();
        let (o, _) = l.run(&subject(), Budgets::default(), &cancel).unwrap();
        assert_eq!(kind(&o), "completed");
    });
}

// §2.8 `host-failure`, WP-089 rule 5: the host's own error is a host
// failure with the host's text; the transform is retired and the next run
// is `Err` without reaching Kiln.
#[test]
fn s2_8_a_host_error_is_host_failure_and_retires() {
    let lying = mode("lie");
    let liar = Liar(&lying);
    with("lie", &with_meta(MODES), &[], |l| {
        let (o, stats) = run(l, &liar).unwrap();
        let f = failure(&o);
        assert_eq!(f.kind, FailureKind::HostFailure);
        assert!(
            f.message.starts_with(
                "the host could not continue: lookup: the subject answered with an entity that \
                 is not live"
            ),
            "{}",
            f.message
        );
        assert!(stats.host_calls > 0);
        let why = l.retired().expect("retired").to_owned();
        assert!(why.starts_with("host-failure: "), "{why}");
        let e = run(l, &subject()).unwrap_err();
        assert!(e.contains(&why), "{e}");
    });
}

// M4 of the review: a panic of the subject during a run is contained: the
// run is a host failure with the panic's message, the transform is retired
// at once, and the session is not used again.
#[test]
fn m4_a_panic_during_a_run_is_a_host_failure_and_retires() {
    let boom = mode("boom");
    let liar = Liar(&boom);
    with("boom", &with_meta(MODES), &[], |l| {
        let (o, _) = run(l, &liar).expect("an outcome, not an unwinding");
        let f = failure(&o);
        assert_eq!(f.kind, FailureKind::HostFailure);
        assert_eq!(f.message, "the run panicked: the subject panics");
        let why = l.retired().expect("retired at once").to_owned();
        assert_eq!(why, "host-failure: the run panicked: the subject panics");
        let e = run(l, &subject()).unwrap_err();
        assert!(e.contains(&why), "{e}");
    });
}

// §2.4 rule 3, WP-089 rule 4: a run's host is new: the handles of one run
// are not live in the next (the report count is per run), and a failure of
// one run is not in the failure list of the next.
#[test]
fn s2_4_rule3_each_run_has_its_own_host() {
    let body = "consteval void transform() {\n\
                  try { (void)slip::extent_of(slip::info()); } catch (const slip::error& e) {\n\
                    if (e.message().empty()) throw 1;\n\
                  }\n\
                  slip::report(slip::severity::note, \"once\");\n\
                }";
    with("own_host", &with_meta(body), &[], |l| {
        let s = FakeSubject::new();
        let outs: Vec<Outcome> = (0..3).map(|_| run(l, &s).unwrap().0).collect();
        assert!(outs.iter().all(|o| *o == outs[0] && reports(o) == 1), "{outs:?}");
    });
}

/// The measurement behind §2.5's note on the step budget (L5 of the
/// review): steps per second of `spin(1000000)`, a plain loop.
/// `cargo test --test budgets -- --ignored measure_step_rate --nocapture`.
#[test]
#[ignore = "a measure"]
fn measure_step_rate() {
    with("rate", &with_meta(MODES), &[], |l| {
        let budget = Budgets { steps: 1 << 21, ..Budgets::default() };
        let (o, stats) = l.run(&mode("steps"), budget, &Cancel::new()).unwrap();
        let rate = u64::try_from(
            u128::from(stats.steps) * 1_000_000_000 / u128::from(stats.wall_ns.max(1)),
        )
        .unwrap();
        println!(
            "MEASURE {} steps in {} ms: {rate} steps/s; 2^32 steps take {} s ({})",
            stats.steps,
            stats.wall_ns / 1_000_000,
            (1u64 << 32) / rate.max(1),
            kind(&o)
        );
    });
}
