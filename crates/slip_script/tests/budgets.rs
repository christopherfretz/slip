//! `spec/api.md` §2.5 and §2.8 through the driver (`docs/wp/WP-089.md`
//! "Tests"): each failure kind with the smallest transform that causes it,
//! an uncaught `slip::error` with the host's message and place, a caught
//! one, a failed run with no effects, and after each failure a normal run
//! that completes. Also the errors of the worker: a cancelled run, a host
//! failure that retires the transform, a poisoned session. Decisions:
//! `spec/landed/api/WP-089.md`.

mod common;

use std::panic::{AssertUnwindSafe, catch_unwind};

use common::{Liar, kind, mode, run, subject, with, with_meta};
use slip_kiln::session::Cancel;
use slip_script::driver::{Budgets, Outcome};
use slip_script::fake::FakeSubject;

/// One transform; what it does is chosen by the subject (`mode::<m>`).
const MODES: &str = r#"
using u64 = unsigned long long;
constexpr int deref(int* p) { return *p; }
constexpr void spin(u64 n) { for (u64 i = 0; i < n; ++i) {} }
constexpr void hold() { char* p = new char[1 << 20]; delete[] p; }
constexpr int down(int n) { return n == 0 ? 0 : down(n - 1) + 1; }
struct mine { int code; };
consteval bool is(const char* m) { return !slip::lookup(std::string("mode::") + m).empty(); }
consteval void misuse() { (void)slip::extent_of(slip::lookup("lib::foo")[0]); }
consteval void primitive() {
  u64 in[1] = {0};
  const char bin[1] = {0};
  u64 out[1] = {0};
  char bout[1] = {0};
  (void)__builtin_kiln_host_call(1u, in, 5, bin, 0, out, 1, bout, 1);
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
}
"#;

/// The reports of a completed run.
fn reports(o: &Outcome) -> usize {
    match o {
        Outcome::Completed(out) => out.reports.len(),
        Outcome::Failed { .. } => panic!("the run failed: {o:?}"),
    }
}

// §2.8, each failure kind, then a normal run that completes (§2.4 rule 3:
// it starts from the compiled unit, and the failed run's report is not
// in it, §2.8 rule 1).
#[test]
fn s2_8_each_failure_kind_then_a_normal_run() {
    let small = Budgets::default();
    let cases: [(&str, Budgets, &str, &str); 7] = [
        ("not_constant", small, "not-constant", "error:"),
        ("steps", Budgets { steps: 100_000, ..small }, "step-budget", "error:"),
        ("memory", Budgets { bytes: 1 << 16, ..small }, "memory-budget", "error:"),
        ("depth", Budgets { depth: 16, ..small }, "depth-limit", "error:"),
        ("uncaught", small, "uncaught", "error:"),
        ("refused", small, "uncaught", "the most recent refusal was: extent_of"),
        // §3.5 rule 5: Kiln's own check of the primitive's buffers is not
        // a host failure (§3.5 rule 4).
        ("primitive", small, "not-constant", "error:"),
    ];
    let normal = subject();
    with("modes", &with_meta(MODES), &[], |l| {
        let (first, _) = run(l, &normal).unwrap();
        assert_eq!(reports(&first), 1, "a normal run");
        for (m, budgets, want, message) in cases {
            let s = mode(m);
            let (o, stats) = l.run(&s, budgets, &Cancel::new()).unwrap();
            let Outcome::Failed { kind, message: got, rendered, .. } = &o else {
                panic!("{m}: completed")
            };
            assert_eq!(*kind, want, "{m}: {rendered}");
            assert!(got.contains(message), "{m}: {got}");
            if m == "primitive" {
                assert!(
                    rendered.contains("of __builtin_kiln_host_call"),
                    "Kiln's check: {rendered}"
                );
            }
            assert!(rendered.contains("modes.cpp:"), "{m}: the place in the transform: {rendered}");
            assert!(stats.steps > 0 && stats.host_calls > 0, "{m}: {stats:?}");
            assert_eq!(l.retired(), None, "{m}: a failure of the transform does not retire it");
            let (after, _) = run(l, &normal).unwrap();
            assert_eq!(after, first, "{m}: the normal run after it is as the first");
        }
    });
}

// §2.8 rule 3, WP-089 rule 3: an uncaught `slip::error` is reported with
// the host's message and the place of the failed call's value in the
// subject; the refusal is the run's most recent.
#[test]
fn s2_8_uncaught_slip_error_has_the_hosts_message_and_place() {
    let s = mode("refused");
    let foo_name = s.range_nth(slip_script::subject::File(1), "foo", 0);
    with("refused", &with_meta(MODES), &[], |l| {
        let (o, _) = run(l, &s).unwrap();
        let Outcome::Failed { kind, message, place, refusal, rendered } = o else {
            panic!("completed")
        };
        assert_eq!(kind, "uncaught");
        assert_eq!(
            message,
            "an uncaught slip::error; the most recent refusal was: extent_of: expected a \
             reference or an expression; given an entity (wrong_kind)"
        );
        assert_eq!(place, Some(foo_name), "the place of lib::foo's declaration");
        let refusal = refusal.expect("the refusal");
        assert_eq!((refusal.errc, refusal.place), (2, Some(foo_name)));
        assert!(rendered.contains("unhandled exception of type 'error'"), "{rendered}");
    });
}

// §2.8: an exception of the transform's own type is reported as Kiln
// renders it, with no refusal; and one of its own named `error` is not
// taken for a `slip::error` when no call of the run was refused
// (decision 4 of the landed file).
#[test]
fn s2_8_uncaught_other_types() {
    let body = "struct error { int n; };\nconsteval void transform() { throw error{1}; }";
    with("own_error", &with_meta(body), &[], |l| {
        let (o, _) = run(l, &subject()).unwrap();
        let Outcome::Failed { kind, message, place, refusal, .. } = o else { panic!("completed") };
        assert_eq!((kind, place, refusal), ("uncaught", None, None));
        assert!(message.contains("own_error.cpp:") && message.contains("error:"), "{message}");
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
// failure with the host's text, and an error of the worker: the
// transform is retired and the next run is `Err` without reaching Kiln.
#[test]
fn s2_8_a_host_error_is_host_failure_and_retires() {
    let lying = mode("lie");
    let liar = Liar(&lying);
    with("lie", &with_meta(MODES), &[], |l| {
        let (o, stats) = run(l, &liar).unwrap();
        let Outcome::Failed { kind, message, .. } = &o else { panic!("completed") };
        assert_eq!(*kind, "host-failure");
        assert!(
            message.contains("lookup: the subject answered with an entity that is not live"),
            "{message}"
        );
        assert!(stats.host_calls > 0);
        let why = l.retired().expect("retired").to_owned();
        assert!(why.starts_with("host-failure: "), "{why}");
        let e = run(l, &subject()).unwrap_err();
        assert!(e.contains(&why), "{e}");
    });
}

// WP-089 "Decided since" item 4: a run whose host panicked unwinds to the
// caller; the session is then unusable, and the driver ends the use of the
// transform with an internal error.
#[test]
fn decided_4_a_poisoned_session_ends_the_transform() {
    let boom = mode("boom");
    let liar = Liar(&boom);
    with("boom", &with_meta(MODES), &[], |l| {
        let unwound = catch_unwind(AssertUnwindSafe(|| run(l, &liar)));
        assert!(unwound.is_err(), "the panic reaches the caller");
        let e = run(l, &subject()).unwrap_err();
        assert!(e.starts_with("internal error:") && e.contains("panic"), "{e}");
        assert!(l.retired().is_some());
        let again = run(l, &subject()).unwrap_err();
        assert!(again.contains("internal error:"), "{again}");
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
