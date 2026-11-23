//! `spec/api.md` §2.5 rule 5, the stack (`spec/landed/api/WP-089.md`, "The
//! stack"; H1 of the review): every recursion a run can drive reaches one
//! of Kiln's own limits before Kiln's stack guard, on a thread of
//! `driver::STACK` bytes in a debug build. Each recursion is tested at its
//! limit: just under it the run completes, over it the run fails with
//! Kiln's limit (`not-constant`, or `depth-limit` for constexpr calls),
//! never "stack exhausted". Under a guard deliberately too small the trip
//! is a host failure that retires the transform; at load, the host's
//! limit.

mod common;

use common::{file, kiln_support, kind, loading, loading_on, resource, subject, words};
use slip_kiln::session::{Cancel, TEMPLATE_DEPTH};
use slip_script::driver::{self, Budgets, Outcome};

/// The recursions a run can drive at run time (H1 b of the review), each
/// a transform that recurses `DEPTH` levels through it. Every one that
/// instantiates reaches it through reflection (`std::meta::substitute`
/// and what reads the result): at run time, not while compiling.
const SHAPES: [(&str, &str); 9] = [
    (
        // Class template instantiation, a static member's initialiser.
        "class_static",
        "template <int K> struct R { static constexpr int v = R<K - 1>::v + 1; };\n\
         template <> struct R<0> { static constexpr int v = 0; };\n\
         consteval void transform() {\n\
           auto r = std::meta::substitute(^^R, {std::meta::reflect_constant(DEPTH)});\n\
           auto m = std::meta::static_data_members_of(r, std::meta::access_context::unchecked())[0];\n\
           if (std::meta::extract<int>(m) != DEPTH) throw 0;\n\
         }",
    ),
    (
        // Class template instantiation, a member's type (completion).
        "class_member",
        "template <int K> struct S { S<K - 1> m; int x; };\n\
         template <> struct S<0> { int x; };\n\
         consteval void transform() {\n\
           if (std::meta::size_of(std::meta::substitute(^^S, {std::meta::reflect_constant(DEPTH)})) == 0) throw 0;\n\
         }",
    ),
    (
        // An alias template over a chain of member types.
        "alias_chain",
        "template <int K> struct T { using type = typename T<K - 1>::type; };\n\
         template <> struct T<0> { using type = int; };\n\
         template <int K> using A = typename T<K>::type;\n\
         consteval void transform() {\n\
           if (std::meta::size_of(std::meta::substitute(^^A, {std::meta::reflect_constant(DEPTH)})) != sizeof(int)) throw 0;\n\
         }",
    ),
    (
        // Variable template instantiation, nested constant evaluation.
        "variable",
        "template <int K> constexpr int V = V<K - 1> + 1;\n\
         template <> constexpr int V<0> = 0;\n\
         consteval void transform() {\n\
           if (std::meta::extract<int>(std::meta::substitute(^^V, {std::meta::reflect_constant(DEPTH)})) != DEPTH) throw 0;\n\
         }",
    ),
    (
        // Function template instantiation, explicit arguments.
        "function",
        "template <int K> consteval int f() { if constexpr (K == 0) return 0; else return f<K - 1>() + 1; }\n\
         consteval void transform() {\n\
           auto p = std::meta::extract<int (*)()>(std::meta::substitute(^^f, {std::meta::reflect_constant(DEPTH)}));\n\
           if (p() != DEPTH) throw 0;\n\
         }",
    ),
    (
        // Substitution into constraints.
        "constraint",
        "template <int K> consteval bool g() requires (K == 0 || g<K - 1>()) { return true; }\n\
         consteval void transform() {\n\
           (void)std::meta::can_substitute(^^g, {std::meta::reflect_constant(DEPTH)});\n\
         }",
    ),
    (
        // Deduction and substitution into a return type (SFINAE).
        "deduce",
        "template <int K> struct ic {};\n\
         constexpr int h(ic<0>) { return 0; }\n\
         template <int K> constexpr auto h(ic<K>) -> decltype(h(ic<K - 1>{})) { return 0; }\n\
         template <int K> using H = decltype(h(ic<K>{}));\n\
         consteval void transform() {\n\
           if (std::meta::size_of(std::meta::substitute(^^H, {std::meta::reflect_constant(DEPTH)})) != sizeof(int)) throw 0;\n\
         }",
    ),
    (
        // Substitution into a lambda in a variable template.
        "lambda",
        "template <int K> constexpr int L = [] { return L<K - 1> + 1; }();\n\
         template <> constexpr int L<0> = 0;\n\
         consteval void transform() {\n\
           if (std::meta::extract<int>(std::meta::substitute(^^L, {std::meta::reflect_constant(DEPTH)})) != DEPTH) throw 0;\n\
         }",
    ),
    (
        // Instantiation with a deeply nested expression in each level (40
        // parentheses: the expression nesting no Kiln limit bounds).
        "nested_body",
        "template <int K> struct R { static constexpr int v = R<K - 1>::v + 1 + ((((((((((((((((((((((((((((((((((((((((K)))))))))))))))))))))))))))))))))))))))) - K; };\n\
         template <> struct R<0> { static constexpr int v = 0; };\n\
         consteval void transform() {\n\
           auto r = std::meta::substitute(^^R, {std::meta::reflect_constant(DEPTH)});\n\
           auto m = std::meta::static_data_members_of(r, std::meta::access_context::unchecked())[0];\n\
           if (std::meta::extract<int>(m) != DEPTH) throw 0;\n\
         }",
    ),
];

/// Constexpr calls: the Oven's own frames, no native stack.
const CALLS: &str = "constexpr int down(int n) { return n == 0 ? 0 : down(n - 1) + 1; }\n\
                     consteval void transform() { if (down(DEPTH) != DEPTH) throw 0; }";

/// What one run of `body` with `DEPTH` = `depth` gives on a thread of
/// `driver::STACK` bytes, guard `guard` (`None`: the driver's own): the
/// kind and the message, and whether the transform was retired.
fn outcome(name: &str, body: &str, depth: u32, guard: Option<usize>) -> (String, String, bool) {
    let text = format!("#include <slip/meta.h>\n#include \"kiln_meta.h\"\n{body}\n");
    let f = file(&format!("{name}{depth}"), &text);
    let author = words(&["-I", &kiln_support(), "-D", &format!("DEPTH={depth}")]);
    let s = subject();
    let go = |l: Result<driver::Loaded<'_>, driver::LoadError>| {
        let mut l = l.unwrap_or_else(|e| panic!("{name}: {e}"));
        let (o, _) =
            l.run(&s, Budgets::default(), &Cancel::new()).unwrap_or_else(|e| panic!("{name}: {e}"));
        let message = match &o {
            Outcome::Completed(_) => String::new(),
            Outcome::Failed(f) => f.message.clone(),
        };
        (kind(&o).to_owned(), message, l.retired().is_some())
    };
    match guard {
        None => loading(&resource(), &author, &f, go),
        Some(g) => loading_on(&resource(), &author, &f, g, go),
    }
}

// H1 b of the review: at the limit, for each recursion through templates.
// `TEMPLATE_DEPTH - 1` levels complete; `TEMPLATE_DEPTH + 88` fail with
// Kiln's own limit, as its message says. Neither trips the guard, nor
// retires the transform.
#[test]
fn s2_5_rule5_each_recursion_reaches_kilns_limit_before_the_guard() {
    assert_eq!(TEMPLATE_DEPTH, 512);
    for (name, body) in SHAPES {
        let under = TEMPLATE_DEPTH - 1;
        let (k, m, retired) = outcome(name, body, under, None);
        assert_eq!((k.as_str(), retired), ("completed", false), "{name} {under}: {m}");
        let over = TEMPLATE_DEPTH + 88;
        let (k, m, retired) = outcome(name, body, over, None);
        assert_eq!(
            (k.as_str(), m.as_str(), retired),
            (
                "not-constant",
                "recursive template instantiation exceeded maximum depth of 512",
                false
            ),
            "{name} {over}"
        );
    }
}

// H1 b: constexpr calls use the Oven's frames: the depth budget (§2.5
// rule 1) is the limit, at 4096, far under the guard.
#[test]
fn s2_5_rule5_constexpr_calls_reach_the_depth_budget() {
    let depth = Budgets::default().depth;
    let (k, m, _) = outcome("calls", CALLS, depth - 2, None);
    assert_eq!(k, "completed", "{m}");
    let (k, m, retired) = outcome("calls", CALLS, depth, None);
    assert_eq!(k, "depth-limit", "{m}");
    assert!(!retired);
}

// H1 c, N7 of the review: under a guard deliberately too small (4 MiB), a
// recursion the driver's stack would hold trips Kiln's guard on the
// loading thread: a host failure, and the transform is retired.
#[test]
fn s2_5_rule5_a_trip_in_a_run_is_a_host_failure_and_retires() {
    let (name, body) = SHAPES[0];
    let (k, m, retired) = outcome(name, body, 300, Some(4 << 20));
    assert_eq!(k, "host-failure", "{m}");
    assert!(m.starts_with("Kiln's stack guard tripped during the run"), "{m}");
    assert!(m.contains("stack exhausted during"), "{m}");
    assert!(retired);
}

// H1 b: a trip while compiling the transform (here `<slip/meta.h>` itself
// under a 2 MiB guard) is the host's limit, `host-failure`, not
// `transform-compile`.
#[test]
fn s12_a_trip_at_load_is_the_hosts_limit() {
    let f = file("load_trip", &common::with_meta("consteval void transform() {}"));
    let e = loading_on(&resource(), &[], &f, 2 << 20, |l| l.map(|l| l.id())).unwrap_err();
    assert_eq!(e.name(), "host-failure", "{e}");
    assert!(e.to_string().contains("the host's limit"), "{e}");
}

/// The measurement behind `driver::STACK` (`spec/landed/api/WP-089.md`,
/// "The stack"): for each shape and `NS` depths, what a run gives under a
/// guard of `GUARD_MIB` (default 16). `cargo test --test stack -- --ignored
/// measure --nocapture`.
#[test]
#[ignore = "a measure"]
fn measure() {
    let guard =
        std::env::var("GUARD_MIB").ok().and_then(|g| g.parse().ok()).unwrap_or(16usize) << 20;
    let ns: Vec<u32> = std::env::var("NS")
        .unwrap_or_else(|_| "250,500".into())
        .split(',')
        .map(|n| n.parse().unwrap())
        .collect();
    for (name, body) in SHAPES.iter().copied().chain([("calls", CALLS)]) {
        for &n in &ns {
            let (k, m, _) = outcome(name, body, n, Some(guard));
            println!("MEASURE {name} {n} guard={}MiB: {k}: {m}", guard >> 20);
        }
    }
}
