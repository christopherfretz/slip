//! `spec/api.md` §2.6 through the driver (`docs/wp/WP-089.md` "Tests"):
//! each context where a host function may not be evaluated, in a plain
//! function and in a template; the failure is the same each time; a call
//! from a nested evaluation (WP-089 rule 6). Decisions:
//! `spec/landed/api/WP-089.md`.
//!
//! At this Kiln revision every template an entry uses is instantiated when
//! the unit is compiled (`spec/landed/api/WP-080.md`, s1), so the template
//! cases of §2.6 rule 3 are found when the transform is loaded, as the
//! plain ones are (`transform-compile`), not by the run that first
//! instantiates them (decision 5 of the landed file).

mod common;

use common::{load, run, subject, with, with_meta};
use slip_script::driver::{LoadError, Outcome};

/// A host function's value, as an integral constant expression would use it.
const Q: &str = "slip::lookup(\"lib::foo\").size()";

/// The contexts of §2.6 rules 2 and 3: (name, text, Kiln's diagnostic).
/// `{q}` is the host call.
#[allow(clippy::too_many_lines, reason = "one row per context of §2.6")]
fn contexts() -> Vec<(&'static str, String, &'static str)> {
    let t = |s: &str| s.replace("{q}", Q);
    vec![
        // Rule 2: while the unit is compiled.
        (
            "namespace_variable",
            t("constexpr auto n = {q};\nconsteval void transform() {}"),
            "must be initialized by a constant expression",
        ),
        (
            "namespace_assert",
            t("static_assert({q} == 1);\nconsteval void transform() {}"),
            "static assertion expression is not an integral constant expression",
        ),
        (
            "class_assert",
            t("struct S { static_assert({q} == 1); };\nconsteval void transform() {}"),
            "static assertion expression is not an integral constant expression",
        ),
        (
            "consteval_block",
            t("consteval { (void){q}; }\nconsteval void transform() {}"),
            "consteval block is not a constant expression",
        ),
        // Rule 3, in the entry's own body (a plain function) ...
        (
            "template_argument",
            t("template <unsigned long N> struct K {};\n\
               consteval void transform() { K<{q}> k; (void)k; }"),
            "non-type template argument is not a constant expression",
        ),
        (
            "array_bound",
            t("consteval void transform() { int a[{q} + 1] = {}; (void)a; }"),
            "error:",
        ),
        (
            "constexpr_variable",
            t("consteval void transform() { constexpr auto n = {q}; (void)n; }"),
            "must be initialized by a constant expression",
        ),
        (
            "if_constexpr",
            t("consteval void transform() { if constexpr ({q} == 1) {} }"),
            "constexpr if condition is not a constant expression",
        ),
        (
            "case_label",
            t("consteval void transform() { switch (1) { case {q}: break; } }"),
            "error:",
        ),
        (
            "static_assert",
            t("consteval void transform() { static_assert({q} == 1); }"),
            "static assertion expression is not an integral constant expression",
        ),
        (
            "constraint",
            t("template <class T> requires ({q} == 1) consteval void f() {}\n\
               consteval void transform() { f<int>(); }"),
            "error:",
        ),
        (
            "noexcept",
            t("consteval void g() noexcept({q} == 1) {}\nconsteval void transform() { g(); }"),
            "error:",
        ),
        (
            "explicit",
            t("struct E { explicit({q} == 1) consteval E(int) {} };\n\
               consteval void transform() { E e(1); (void)e; }"),
            "error:",
        ),
        // ... and in a template the entry instantiates.
        (
            "template_argument_in_template",
            t("template <unsigned long N> struct K {};\n\
               template <class T> consteval void f() { K<{q}> k; (void)k; }\n\
               consteval void transform() { f<int>(); }"),
            "non-type template argument is not a constant expression",
        ),
        (
            "array_bound_in_template",
            t("template <class T> consteval void f() { int a[{q} + sizeof(T)] = {}; (void)a; }\n\
               consteval void transform() { f<int>(); }"),
            "error:",
        ),
        (
            "constexpr_variable_in_template",
            t(
                "template <class T> consteval void f() { constexpr auto n = {q} + sizeof(T); (void)n; }\n\
               consteval void transform() { f<int>(); }",
            ),
            "must be initialized by a constant expression",
        ),
        (
            "if_constexpr_in_template",
            t("template <class T> consteval void f() { if constexpr ({q} == sizeof(T)) {} }\n\
               consteval void transform() { f<int>(); }"),
            "constexpr if condition is not a constant expression",
        ),
        (
            "case_label_in_template",
            t(
                "template <class T> consteval void f() { switch (1) { case {q} + sizeof(T): break; } }\n\
               consteval void transform() { f<int>(); }",
            ),
            "error:",
        ),
        (
            "static_assert_in_template",
            t("template <class T> consteval void f() { static_assert({q} == sizeof(T)); }\n\
               consteval void transform() { f<int>(); }"),
            "static assertion expression is not an integral constant expression",
        ),
        (
            "constraint_in_template",
            t("template <class T> struct C { template <class U> requires ({q} == sizeof(U)) \
               static consteval void f() {} };\n\
               consteval void transform() { C<int>::f<int>(); }"),
            "error:",
        ),
        (
            "noexcept_in_template",
            t("template <class T> consteval void g() noexcept({q} == sizeof(T)) {}\n\
               consteval void transform() { g<int>(); }"),
            "error:",
        ),
        (
            "explicit_in_template",
            t("template <class T> struct E { explicit({q} == sizeof(T)) consteval E(int) {} };\n\
               consteval void transform() { E<int> e(1); (void)e; }"),
            "error:",
        ),
        (
            "generic_lambda",
            t("consteval void transform() {\n\
               auto l = [](auto x) { constexpr auto n = {q}; return x + n; }; (void)l(1); }"),
            "must be initialized by a constant expression",
        ),
    ]
}

// §2.6 rules 2 and 3: each context, in a plain function and in a
// template, is `transform-compile` with Kiln's diagnostic at the context's
// line; loading it three times gives the same error each time.
#[test]
fn s2_6_each_forbidden_context_is_refused() {
    for (name, text, diagnostic) in contexts() {
        // The three loads at once: each compiles on a thread of its own.
        let errors: Vec<LoadError> = std::thread::scope(|t| {
            let loads: Vec<_> = (0..3)
                .map(|_| t.spawn(|| load(name, &with_meta(&text), &[]).expect_err(name)))
                .collect();
            loads.into_iter().map(|h| h.join().unwrap()).collect()
        });
        let LoadError::Compile { rendered } = &errors[0] else { panic!("{name}: {}", errors[0]) };
        assert!(rendered.contains(diagnostic), "{name}: {rendered}");
        assert!(rendered.contains(&format!("{name}.cpp:")), "{name}: {rendered}");
        for e in &errors[1..] {
            assert_eq!(normalise(e, name), normalise(&errors[0], name), "{name}");
        }
    }
}

/// An error with its directory (one per load) taken out.
fn normalise(e: &LoadError, name: &str) -> String {
    let text = e.to_string();
    text.lines()
        .map(|l| match l.find(&format!("{name}.cpp")) {
            Some(i) => l[i..].to_owned(),
            None => l.to_owned(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// §2.6 rule 1, the contrast: the same host call evaluated by the run, in
// the entry and in a template it instantiates, completes; three runs give
// the same outcome.
#[test]
fn s2_6_rule1_the_run_may_call_the_host() {
    let body = "template <class T> consteval unsigned long f() { return slip::lookup(\"lib::foo\").size() + sizeof(T); }\n\
                consteval void transform() {\n\
                  if (slip::lookup(\"lib::foo\").size() != 1 || f<char>() != 2) throw 0;\n\
                }";
    with("allowed", &with_meta(body), &[], |l| {
        let s = subject();
        let outs: Vec<Outcome> = (0..3).map(|_| run(l, &s).unwrap().0).collect();
        assert!(matches!(outs[0], Outcome::Completed(_)), "{:?}", outs[0]);
        assert!(outs.iter().all(|o| *o == outs[0]));
    });
}

// WP-089 rule 6, `kiln-landed-2026-11.md` §3.4 item 3: a call from a
// nested evaluation (the initialiser of a `constexpr` variable in a
// function template the entry calls) never reaches the subject: Kiln
// evaluates it with the unit's placeholder handler, which refuses every
// call, and the transform does not load. The driver's own flag is tested
// in `driver::tests` (it refuses a call outside `Session::run`).
#[test]
fn rule6_a_call_from_a_nested_evaluation_does_not_reach_the_host() {
    let body = "template <class T> consteval int f() { constexpr auto n = slip::lookup(\"lib::foo\").size(); return n; }\n\
                consteval void transform() { (void)f<int>(); }";
    let e = load("nested", &with_meta(body), &[]).unwrap_err();
    let LoadError::Compile { rendered } = &e else { panic!("{e}") };
    assert!(
        rendered.contains("host calls are served only in evaluation-session runs"),
        "{rendered}"
    );
}

/// KN2 of WP-089 (M5 of its review): a host call in a requires-expression
/// of a template the entry instantiates.
const KN2: &str = "template <unsigned long N> struct K {};\n\
                   template <class T> consteval bool has() {\n\
                     return requires { typename K<slip::lookup(\"lib::foo\").size() + sizeof(T)>; };\n\
                   }\n\
                   consteval void transform() {\n\
                     slip::report(slip::severity::note, has<int>() ? \"yes\" : \"no\");\n\
                   }";

/// The text of a completed run's one report.
fn report_text(o: &Outcome) -> String {
    match o {
        Outcome::Completed(out) => out
            .reports
            .iter()
            .map(|r| String::from_utf8_lossy(&r.message).into_owned())
            .collect::<Vec<_>>()
            .join(","),
        Outcome::Failed(f) => panic!("failed: {}", f.message),
    }
}

// KN2, confirmed: §2.6 rule 3's context "a constraint" includes a
// requires-expression, and there the host call is not diagnosed: the
// requirement is silently unsatisfied, `has<int>()` is false, the
// transform loads and the run completes. Not a diagnosed failure, which
// §2.6 promises (Kiln finding KN2).
#[test]
fn kn2_a_host_call_in_a_requires_expression_is_silently_false() {
    with("kn2", &with_meta(KN2), &[], |l| {
        let (o, _) = run(l, &subject()).unwrap();
        assert_eq!(report_text(&o), "no");
    });
}

// The outcome KN2 waits for: a diagnosed failure, `transform-compile` (the
// entry is not a template, so `has<int>` is instantiated when the unit is
// compiled, as for the other contexts).
#[test]
#[ignore = "needs: Kiln KN2 (a host call in a requires-expression is diagnosed)"]
fn kn2_pending_a_host_call_in_a_requires_expression_is_diagnosed() {
    let e = load("kn2_pending", &with_meta(KN2), &[]).unwrap_err();
    assert_eq!(e.name(), "transform-compile", "{e}");
}

// §2.6 rule 3 in a template instantiated at run time, through reflection
// (`substitute`, then `extract`): the host call in its template argument
// fails the run that instantiates it, `not-constant`, the same in each of
// three runs; the transform is not retired.
#[test]
fn s2_6_rule3_a_template_instantiated_at_run_time() {
    let body = "#include \"kiln_meta.h\"\n\
                template <unsigned long N> struct K { static constexpr unsigned long v = N; };\n\
                template <class T> struct W { static constexpr unsigned long v = K<slip::lookup(\"lib::foo\").size() + sizeof(T)>::v; };\n\
                consteval void transform() {\n\
                  auto w = std::meta::substitute(^^W, {^^int});\n\
                  auto m = std::meta::static_data_members_of(w, std::meta::access_context::unchecked())[0];\n\
                  (void)std::meta::extract<unsigned long>(m);\n\
                }";
    let support = common::kiln_support();
    with("run_time", &with_meta(body), &["-I", &support], |l| {
        let s = subject();
        let outs: Vec<Outcome> = (0..3).map(|_| run(l, &s).unwrap().0).collect();
        let Outcome::Failed(f) = &outs[0] else { panic!("{:?}", outs[0]) };
        assert_eq!(f.kind.name(), "not-constant", "{}", f.rendered);
        assert!(outs.iter().all(|o| *o == outs[0]));
        assert_eq!(l.retired(), None);
    });
}
