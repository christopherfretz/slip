//! `KilnSubject` for calls and the extents of expressions
//! (`docs/wp/WP-086.md`): `spec/api.md` §6.6 rules 3 and 4, §6.7, §7.4
//! rule 3, on snippets parsed with Kiln's written-reference table,
//! calling the trait directly. Each test names its section and rule, or
//! the item of the package's test list.

use std::collections::HashSet;

use slip_kiln::snippet::{Extra, Parsed, Snippet, parse_with_tables};
use slip_script::host::parse_qualified_name;
use slip_script::kiln_subject::{KilnSubject, NoExtent};
use slip_script::subject::{Entity, Expression, PlaceOf, Range, Subject, Value};
use slip_script::wire::{self, Errc};

// ---------------------------------------------------------------------------
// Harness.

/// Parses `files` (the first is the main file) with the tables and calls
/// `f` with the subject over it. The snippet must parse without error.
fn with<R>(files: &[(&str, &str)], f: impl FnOnce(&Parsed<'_>, &KilnSubject<'_>) -> R) -> R {
    with_args(files, &[], f)
}

fn with_args<R>(
    files: &[(&str, &str)],
    args: &[&str],
    f: impl FnOnce(&Parsed<'_>, &KilnSubject<'_>) -> R,
) -> R {
    let snippet = Snippet { files, main: files[0].0, include: &[], std: "c++23" };
    let extra = Extra { args, target: None };
    parse_with_tables(&snippet, &extra, |p| {
        assert_eq!(p.errors, 0, "{}", p.diagnostics);
        let owned = |_: &[u8]| true;
        let s = KilnSubject::from_snippet(p, &owned);
        f(p, &s)
    })
    .expect("the snippet parses")
}

/// The one entity `name` finds.
fn one(s: &KilnSubject<'_>, name: &str) -> Entity {
    let (global, components) = parse_qualified_name(name.as_bytes()).expect("a qualified name");
    let found = s.lookup(global, &components);
    assert_eq!(found.len(), 1, "{name}: {found:?}");
    found[0]
}

/// The text of the extent of `x`, `None` for a null extent.
fn ext(s: &KilnSubject<'_>, x: Expression) -> Option<String> {
    assert!(s.is_live(Value::Expression(x)), "{x:?} is live");
    let r = s.extent(Value::Expression(x)).expect("no error")?;
    Some(String::from_utf8(s.text(r).to_vec()).expect("UTF-8"))
}

fn range(s: &KilnSubject<'_>, x: Expression) -> Option<Range> {
    s.extent(Value::Expression(x)).expect("no error")
}

/// The calls of `name`, each as its extent's text.
fn calls(s: &KilnSubject<'_>, name: &str) -> Vec<Option<String>> {
    calls_of(s, one(s, name)).iter().map(|&x| ext(s, x)).collect()
}

/// `calls_to(e)`, checked: each a live call whose callee's origin is `e`,
/// with a live callee reference whose referent is the callee.
fn calls_of(s: &KilnSubject<'_>, e: Entity) -> Vec<Expression> {
    let xs = s.calls_to(e).expect("a function");
    for &x in &xs {
        let v = Value::Expression(x);
        assert!(s.is_live(v));
        assert_eq!(s.kind_bits(v), wire::KIND_EXPRESSION | wire::KIND_CALL, "{x:?}");
        let callee = s.callee(x).expect("a call");
        assert_eq!(s.origin(callee), e);
        let r = s.callee_reference(x).expect("a reference");
        assert!(s.is_live(Value::Reference(r)), "{r:?}");
        assert_eq!(s.referent(r), callee);
        for a in s.arguments(x).expect("arguments") {
            assert!(s.is_live(Value::Expression(a)), "{a:?}");
        }
    }
    xs
}

/// The arguments of `x`, each as its extent's text.
fn args(s: &KilnSubject<'_>, x: Expression) -> Vec<Option<String>> {
    s.arguments(x).expect("a call").into_iter().map(|a| ext(s, a)).collect()
}

fn some(t: &[&str]) -> Vec<Option<String>> {
    t.iter().map(|t| Some((*t).to_owned())).collect()
}

/// Every call of the unit's census, for "no other call" assertions.
fn call_count(s: &KilnSubject<'_>) -> u64 {
    s.extent_census().calls
}

// ---------------------------------------------------------------------------
// Test 1: §6.7 rule 1, the six forms.

const SIX: &str = "\
namespace N { int f(int); }
int f(int);
struct C {
  static int f(int);
  int m(int);
};
int g(C a, C* p) {
  return f(1) + N::f(2) + (f)(3) + a.m(4) + p->m(5) + C::f(6);
}
";

#[test]
fn s6_7_rule1_the_six_forms_are_calls() {
    with(&[("main.cpp", SIX)], |_, s| {
        assert_eq!(calls(s, "f"), some(&["f(1)", "(f)(3)"]));
        assert_eq!(calls(s, "N::f"), some(&["N::f(2)"]));
        assert_eq!(calls(s, "C::m"), some(&["a.m(4)", "p->m(5)"]));
        assert_eq!(calls(s, "C::f"), some(&["C::f(6)"]));
        for (name, want) in [("f", ["1", "3"]), ("C::m", ["4", "5"])] {
            let xs = calls_of(s, one(s, name));
            let got: Vec<_> = xs.iter().map(|&x| args(s, x)).collect();
            assert_eq!(got, vec![some(&[want[0]]), some(&[want[1]])], "{name}");
        }
        // The callee is the function, its reference is the name.
        let x = calls_of(s, one(s, "N::f"))[0];
        assert_eq!(s.callee(x), Some(one(s, "N::f")));
        let r = s.callee_reference(x).unwrap();
        assert_eq!(s.referent(r), one(s, "N::f"));
        assert_eq!(call_count(s), 6);
        assert_eq!(s.calls_without_reference(), 0);
    });
}

/// Rule 1 inside a member function: a member named without `this`, and
/// one named with its class, are calls whose extent starts at the name.
#[test]
fn s6_7_rule1_member_named_without_this() {
    let src = "\
struct B { int m(int); };
struct D : B {
  int m(int);
  int k() { return m(1) + B::m(2) + this->m(3) + D::m(4); }
};
";
    with(&[("main.cpp", src)], |_, s| {
        assert_eq!(calls(s, "D::m"), some(&["m(1)", "this->m(3)", "D::m(4)"]));
        assert_eq!(calls(s, "B::m"), some(&["B::m(2)"]));
    });
}

/// Rule 1: a call through an explicit object parameter is a member
/// call; its object is no argument (rule 5).
#[test]
fn s6_7_rule1_explicit_object_parameter() {
    let src = "\
struct S { int f(this S, int); };
int g(S s) { return s.f(2); }
";
    with(&[("main.cpp", src)], |_, s| {
        let xs = calls_of(s, one(s, "S::f"));
        assert_eq!(xs.iter().map(|&x| ext(s, x)).collect::<Vec<_>>(), some(&["s.f(2)"]));
        assert_eq!(args(s, xs[0]), some(&["2"]));
    });
}

/// `calls_to` of something not a function is `wrong_kind` (rule 3); of a
/// function template, the calls of its specialisations.
#[test]
fn s6_7_rule3_calls_to_a_template_and_wrong_kind() {
    let src = "\
template <class T> int t(T);
int v;
int g() { return t(1) + t('c') + t<long>(2); }
";
    with(&[("main.cpp", src)], |_, s| {
        assert_eq!(calls(s, "t"), some(&["t(1)", "t('c')", "t<long>(2)"]));
        assert_eq!(s.calls_to(one(s, "v")), Err(Errc::WrongKind));
    });
}

// ---------------------------------------------------------------------------
// Test 2: rule 2, what is not a call.

#[test]
fn s6_7_rule2_what_is_not_a_call() {
    let src = "\
struct Q { int bar(); };
struct T { operator int() const; explicit operator bool() const; };
int conv(T t) { int a = t; long b = static_cast<int>(t); if (t) {} return a + b + (int)t; }
struct S {
  S(int);
  int operator+(S) const;
  int operator()(int) const;
  int operator-() const;
};
int h(int);
int foo(struct Q);
template <class T> int dep(T t) { return foo(t) + t.bar(); }
int use(S s) {
  int (*fp)(int) = h;
  int (&fr)(int) = h;
  S v(1);
  return (s + s) + (-s) + s(3) + S(1)(2) + S{1}(2) + fp(1) + fr(2) + (*fp)(3) + dep(Q());
}
";
    with(&[("main.cpp", src)], |_, s| {
        // An operator function or a constructor has no name `lookup`
        // finds: the count of every call of the unit stands for them.
        for f in ["h", "foo"] {
            assert_eq!(calls(s, f), Vec::<Option<String>>::new(), "{f}");
        }
        // The one call: `dep(Q())`; `foo(t)` and `t.bar()` are dependent,
        // and their calls in the instantiation are the instantiation's.
        assert_eq!(calls(s, "dep"), some(&["dep(Q())"]));
        assert!(calls(s, "Q::bar").is_empty());
        assert_eq!(call_count(s), 1);
    });
}

// ---------------------------------------------------------------------------
// Test 3: rule 5, the arguments as written.

#[test]
fn s6_7_rule5_default_arguments_are_absent() {
    let src = "\
int d(int a, int b = 2, int c = 3);
int g() { return d(1) + d(1, 5); }
";
    with(&[("main.cpp", src)], |_, s| {
        let xs = calls_of(s, one(s, "d"));
        assert_eq!(args(s, xs[0]), some(&["1"]));
        assert_eq!(args(s, xs[1]), some(&["1", "5"]));
        assert_eq!(ext(s, xs[0]).as_deref(), Some("d(1)"));
    });
}

#[test]
fn s6_7_rule5_the_object_is_no_argument() {
    let src = "\
struct M { int m(int); static int sm(int); };
int g(M o, M* p) { return o.m(3) + p->m(4) + o.sm(5); }
";
    with(&[("main.cpp", src)], |_, s| {
        let xs = calls_of(s, one(s, "M::m"));
        assert_eq!(args(s, xs[0]), some(&["3"]));
        assert_eq!(args(s, xs[1]), some(&["4"]));
        let ys = calls_of(s, one(s, "M::sm"));
        assert_eq!(ext(s, ys[0]).as_deref(), Some("o.sm(5)"));
        assert_eq!(args(s, ys[0]), some(&["5"]));
    });
}

/// Rule 5's pack expansion: a call whose argument is one has a
/// type-dependent argument, so its callee is a dependent name and it is
/// no call (rule 2); the calls of the instantiations are not either
/// (`spec/landed/api/WP-086.md`, decided where the spec was silent).
#[test]
fn s6_7_rule5_a_pack_expansion_is_in_no_call() {
    let src = "\
int d(int, int);
template <class... A> int pk(A... a) { return d(a...); }
int g() { return pk(1, 2); }
";
    with(&[("main.cpp", src)], |_, s| {
        assert!(calls(s, "d").is_empty());
        assert_eq!(calls(s, "pk"), some(&["pk(1, 2)"]));
        assert_eq!(call_count(s), 1);
    });
}

/// Rule 5 with an implicit conversion: the argument is what is written,
/// whatever Kiln wraps around it (a conversion, a temporary, a
/// converting or copying construction, a conversion function's call).
#[test]
fn s6_7_rule5_implicit_conversions_are_skipped() {
    let src = "\
struct S { S(int); };
struct T { operator int() const; };
int h(S);
int i(int);
int j(const S&);
int g(S s, T t, short c) {
  return h(1) + h(s) + i(t) + i(c) + j(2) + j(s) + h({3}) + i(t.operator int());
}
";
    with(&[("main.cpp", src)], |p, s| {
        let a: Vec<_> = calls_of(s, one(s, "h")).into_iter().map(|x| args(s, x)).collect();
        // `h({3})`: a construction at `{` from `3`; the braces are in no
        // node, so the argument is the construction, which has no extent.
        assert_eq!(a, vec![some(&["1"]), some(&["s"]), vec![None]]);
        let b: Vec<_> = calls_of(s, one(s, "i")).into_iter().map(|x| args(s, x)).collect();
        // `t.operator int()` is written: the call, not the object `t`.
        assert_eq!(b, vec![some(&["t"]), some(&["c"]), some(&["t.operator int()"])]);
        // The argument is the written expression, not Kiln's conversion.
        let kinds: Vec<String> = calls_of(s, one(s, "i"))
            .into_iter()
            .map(|x| {
                let a = s.arguments(x).unwrap()[0];
                format!("{:?}", p.ast.exprs.get(slip_kiln::kiln::ast::ExprId::new(a.0)).kind())
            })
            .collect();
        assert_eq!(kinds, ["DeclRefExpr", "DeclRefExpr", "CXXMemberCallExpr"]);
        let c: Vec<_> = calls_of(s, one(s, "j")).into_iter().map(|x| args(s, x)).collect();
        assert_eq!(c, vec![some(&["2"]), some(&["s"])]);
    });
}

const MACROS: &str = "\
int d(int);
int e(int, int);
#define MAX(a, b) ((a) > (b) ? (a) : (b))
#define WRAP(x) (x)
#define ID(x) x
int g(int u, int v) { return d(MAX(u, v)) + WRAP(e(u, 5)) + ID(d(7)); }
";

/// Rule 5 with macros: an argument that is a macro invocation has the
/// invocation for extent (tier 0); a call and its arguments inside a
/// macro argument have the bytes of the argument (tier 1); a call that is
/// the whole expansion of an invocation has the invocation (§5.4 rule 6).
#[test]
fn s6_7_rule5_arguments_and_macros() {
    with(&[("main.cpp", MACROS)], |_, s| {
        let ds = calls_of(s, one(s, "d"));
        assert_eq!(args(s, ds[0]), some(&["MAX(u, v)"]));
        let a = s.arguments(ds[0]).unwrap()[0];
        assert_eq!(s.tiers(range(s, a).unwrap()), Some([true, false]));
        let es = calls_of(s, one(s, "e"));
        assert_eq!(ext(s, es[0]).as_deref(), Some("e(u, 5)"));
        assert_eq!(s.tiers(range(s, es[0]).unwrap()), Some([false, true]));
        assert_eq!(args(s, es[0]), some(&["u", "5"]));
        for a in s.arguments(es[0]).unwrap() {
            assert_eq!(s.tiers(range(s, a).unwrap()), Some([false, true]));
        }
        // `ID(d(7))`: the whole expansion is the call; its range is the
        // invocation, at tier 0; its argument is in the argument.
        assert_eq!(ext(s, ds[1]).as_deref(), Some("ID(d(7))"));
        assert_eq!(s.tiers(range(s, ds[1]).unwrap()), Some([true, false]));
        assert_eq!(args(s, ds[1]), some(&["7"]));
    });
}

// ---------------------------------------------------------------------------
// Test 4: a call in a template is returned once.

#[test]
fn s6_7_rule2_a_call_in_a_template_is_returned_once() {
    let src = "\
int f(int);
template <class T> int t(T x) { return f(1) + f(sizeof(T)); }
template <class T> struct K { int m() { return f(2); } };
int a = t(1) + t('c') + t(2.0);
int b = K<int>().m() + K<char>().m();
template int t<long>(long);
template struct K<short>;
struct E { int k = f(3); bool operator==(const E&) const = default; E() = default; };
bool c = E() == E();
";
    with(&[("main.cpp", src)], |_, s| {
        assert_eq!(calls(s, "f"), some(&["f(1)", "f(sizeof(T))", "f(2)", "f(3)"]));
        assert_eq!(call_count(s), 4 + 5);
    });
}

/// An explicit specialisation is written: its calls are returned.
#[test]
fn s6_7_rule2_an_explicit_specialisation_is_written() {
    let src = "\
int f(int);
template <class T> int t(T) { return f(1); }
template <> int t(char) { return f(2); }
int a = t(1) + t('c');
";
    with(&[("main.cpp", src)], |_, s| {
        assert_eq!(calls(s, "f"), some(&["f(1)", "f(2)"]));
    });
}

// ---------------------------------------------------------------------------
// Test 5: where calls are.

#[test]
fn s6_7_calls_in_lambdas_initialisers_and_constraints() {
    let src = "\
constexpr int f(int x) { return x; }
int l = [] { return f(1); }();
struct D { int m = f(2); int n : 4 = f(8); };
int da(int a = f(3));
static_assert(f(4) == 4, \"\");
template <class T> concept C = requires { f(5); };
template <class T> requires (f(6) == 6) int r(T) { return 0; }
struct E { int k; E() : k(f(7)) {} };
int w = [x = f(9)](int y = f(10)) { return x + y; }();
";
    with(&[("main.cpp", src)], |_, s| {
        let want =
            ["f(1)", "f(2)", "f(8)", "f(3)", "f(4)", "f(5)", "f(6)", "f(7)", "f(9)", "f(10)"];
        assert_eq!(calls(s, "f"), some(&want));
    });
}

/// Calls in statements of every kind, and in a local class: each once,
/// in unit order; none of the range-for's own calls.
#[test]
fn s6_7_calls_in_statements() {
    let src = "\
int f(int);
struct R { int* begin(); int* end(); };
int g(R r) {
  if (int a = f(1)) { f(2); } else f(3);
  switch (f(4)) { case 1: f(5); default: f(6); }
  while (f(7)) {}
  do { f(8); } while (f(9));
  for (int i = f(10); f(11); f(12)) f(13);
  for (int x : r) f(x);
  struct L { int m() { return f(14); } };
  try { f(15); } catch (...) { f(16); }
  int y = ({ f(17); });
  return f(18);
}
";
    with(&[("main.cpp", src)], |_, s| {
        let want: Vec<String> = (1..=13)
            .map(|k| format!("f({k})"))
            .chain(["f(x)".to_owned()])
            .chain((14..=18).map(|k| format!("f({k})")))
            .collect();
        let want: Vec<&str> = want.iter().map(String::as_str).collect();
        assert_eq!(calls(s, "f"), some(&want));
        assert!(calls(s, "R::begin").is_empty());
        assert!(calls(s, "R::end").is_empty());
    });
}

/// A coroutine's calls are the ones written: the operand of `co_await`
/// and the calls in it, not the awaiter's and the promise's calls Kiln
/// makes.
#[test]
fn s6_7_calls_in_a_coroutine() {
    let src = "\
namespace std {
template <class R, class... A> struct coroutine_traits { using promise_type = typename R::promise_type; };
template <class P = void> struct coroutine_handle {
  static coroutine_handle from_address(void*) noexcept; void* address() const noexcept;
};
template <> struct coroutine_handle<void> {
  template <class P> coroutine_handle(coroutine_handle<P>) noexcept;
  static coroutine_handle from_address(void*) noexcept; void* address() const noexcept;
};
}
struct suspend {
  bool await_ready() noexcept; void await_suspend(std::coroutine_handle<>) noexcept;
  void await_resume() noexcept;
};
struct task {
  struct promise_type {
    task get_return_object(); suspend initial_suspend() noexcept; suspend final_suspend() noexcept;
    void return_void(); void unhandled_exception();
  };
};
int f(int);
suspend aw(int);
task co() { co_await aw(f(1)); co_return; }
";
    with(&[("main.cpp", src)], |_, s| {
        assert_eq!(calls(s, "aw"), some(&["aw(f(1))"]));
        assert_eq!(calls(s, "f"), some(&["f(1)"]));
        assert!(calls(s, "suspend::await_ready").is_empty());
        assert!(calls(s, "task::promise_type::initial_suspend").is_empty());
        assert_eq!(s.calls_without_reference(), 0);
        assert_eq!(call_count(s), 2);
    });
}

/// The holder of a call is the declaration at namespace or class scope
/// whose body or initialiser holds it; a local class's and a lambda's
/// calls are their enclosing function's.
#[test]
fn holder_is_the_unit_that_holds_the_call() {
    // `holder` is `pub(crate)`: its rule is tested through the census of
    // `calls_to` here and by WP-095 through `unit_keys`. The walk reaches
    // each call from exactly one declaration (a call reached twice would
    // be returned twice).
    let src = "\
int f(int);
struct A { int m() { struct L { int n() { return f(1); } }; return [] { return f(2); }(); } };
int v = f(3);
";
    with(&[("main.cpp", src)], |_, s| {
        assert_eq!(calls(s, "f"), some(&["f(1)", "f(2)", "f(3)"]));
    });
}

// ---------------------------------------------------------------------------
// Test 6: extents (§6.6 rules 3 and 4), kind by kind.

const KINDS: &str = r#"
template <class T> int k(T&&);
struct P { int a, b; };
int kp(P);
namespace N { int v; }
int v, w[4], *q = &v;
long operator""_km(unsigned long long);
struct S {
  int x;
  static int sx;
  int f(int) const;
  S operator+(S) const;
  S operator-() const;
  S operator++(int);
  int operator()(int) const;
  int operator[](int) const;
  S(int = 0);
  bool operator==(int) const;
  int g(S o, S* p, S t) {
    return k(1) + k(1.5) + k('c') + k("s" "t") + k(true) + k(nullptr) + k(__null)
      + k(this) + k(v) + k(N::v) + k(::v) + k(o.x) + k(p->x) + k(x) + k(S::sx)
      + k((v)) + k([] { return 1; }) + kp({1, 2}) + k(static_cast<long>(v))
      + k(sizeof(int)) + k(sizeof v) + k(int(2)) + k(S(1)) + k(S{1}) + k((long)v)
      + k(f(1)) + k(o.f(2)) + k(w[1]) + k(-v) + k(!v) + k(v++) + k(v + 1)
      + k(v += 1) + k(v ? 1 : 2) + k(o + t) + k(-o) + k(o++) + k(o(3)) + k(o[4])
      + k(new int(1)) + k(1_km) + k(noexcept(v));
  }
};
"#;

/// Every kind of the table of `spec/landed/api/WP-086.md` has its exact
/// bytes; three kinds outside the table have none.
#[test]
fn s6_6_rule3_the_extent_of_every_kind_of_the_table() {
    with(&[("main.cpp", KINDS)], |_, s| {
        let got: Vec<Option<String>> =
            calls_of(s, one(s, "k")).into_iter().flat_map(|x| args(s, x)).collect();
        let want: Vec<Option<String>> = [
            "1",
            "1.5",
            "'c'",
            "\"s\" \"t\"",
            "true",
            "nullptr",
            "__null",
            "this",
            "v",
            "N::v",
            "::v",
            "o.x",
            "p->x",
            "x",
            "S::sx",
            "(v)",
            "[] { return 1; }",
            "static_cast<long>(v)",
            "sizeof(int)",
            "sizeof v",
            "int(2)",
            "S(1)",
            "S{1}",
            "(long)v",
            "f(1)",
            "o.f(2)",
            "w[1]",
            "-v",
            "!v",
            "v++",
            "v + 1",
            "v += 1",
            "v ? 1 : 2",
            "o + t",
            "-o",
            "o++",
            "o(3)",
            "o[4]",
        ]
        .iter()
        .map(|t| Some((*t).to_owned()))
        .chain([None, None, None])
        .collect();
        assert_eq!(got, want);
        let kp = calls_of(s, one(s, "kp"));
        assert_eq!(args(s, kp[0]), some(&["{1, 2}"]));
        // The three nulls say which kind stopped the rule.
        let xs = calls_of(s, one(s, "k"));
        let n = xs.len();
        let why: Vec<NoExtent> = xs[n - 3..]
            .iter()
            .map(|&x| s.no_extent(s.arguments(x).unwrap()[0]).expect("null"))
            .collect();
        let kinds: Vec<String> = why.iter().map(|w| format!("{:?}", w.kind)).collect();
        // A user-defined literal is a `CallExpr` in Kiln, whose `)` is the
        // literal, before its callee (the suffix): not a postfix call.
        assert_eq!(kinds, ["CXXNewExpr", "CallExpr", "CXXNoexceptExpr"]);
        assert_eq!(why[1].reason, "not a postfix call");
        assert_eq!(s.calls_without_reference(), 0);
        // Every call of `k` has its extent: `k(` to `)`.
        for x in xs {
            let t = ext(s, x).expect("an extent");
            assert!(t.starts_with("k(") && t.ends_with(')'), "{t}");
        }
    });
}

/// Rule 4: a name with template arguments has no last token Kiln
/// stores, so an argument that ends with one has no extent, while the
/// call it is the callee of has one; an operator function named as a
/// function has no last token either.
#[test]
fn s6_6_rule4_null_where_the_last_token_is_unknown() {
    let src = "\
template <class T> int k(T);
template <class T> int t(T);
struct B { int z; };
struct S : B { int operator+(int) const; static int sx; int x; int m(); S(int); };
int g(S s) { return k(t<int>) + t<long>(1) + k(&S::operator+) + s.operator+(1); }
int S::m() { S o(1); return k(decltype(o)::x) + k(decltype(o)::sx) + k(::S(1)) + k(decltype(x)(2))
  + k(decltype(o)::B::z); }
";
    with(&[("main.cpp", src)], |_, s| {
        // A `decltype` qualifier ends the table's chain; `::S(1)` begins
        // with a `::` the table does not place; a type named by
        // `decltype` is not named by an entry.
        let m: Vec<_> = calls_of(s, one(s, "k"))[2..].iter().flat_map(|&x| args(s, x)).collect();
        assert_eq!(m, vec![None, None, None, None, None]);
        let xs = calls_of(s, one(s, "k"));
        assert_eq!(args(s, xs[0]), vec![None]);
        assert_eq!(calls(s, "t"), some(&["t<long>(1)"]));
        assert_eq!(args(s, xs[1]), vec![None]);
    });
}

/// An overloaded `->`: the member call through it begins at the object
/// (the operator call's first operand) and has its extent; a member
/// access through it ends at the member name, so `p->x` has one too.
#[test]
fn s6_6_rule3_a_call_through_an_overloaded_arrow() {
    let src = "\
struct S { int f(int); int x; };
struct P { S* operator->(); };
int k(int);
int g(P p) { return p->f(1) + k(p->x); }
";
    with(&[("main.cpp", src)], |_, s| {
        assert_eq!(calls(s, "S::f"), some(&["p->f(1)"]));
        let x = calls_of(s, one(s, "k"))[0];
        assert_eq!(args(s, x), some(&["p->x"]));
    });
}

/// Rule 4: an argument whose written tokens Kiln keeps in no node has
/// no extent. `f({x})`, when `x` of the class alone initialises the
/// parameter, is `x` in Kiln's AST: the braces are in no node, and the
/// range of `x` would be too short.
#[test]
fn s6_6_rule4_an_argument_is_all_of_what_is_written() {
    let src = "\
struct O { bool a = false; long b = 0; };
int f(const O&);
int g(O);
int g2(int, O, int);
int h(int);
int use(O x) { return f({x}) + g({x}) + g({x,}) + g2(1, {x}, 2) + f(x) + h(1 /* one */) + h((2)); }
";
    with(&[("main.cpp", src)], |_, s| {
        assert_eq!(calls(s, "f"), some(&["f({x})", "f(x)"]));
        let a: Vec<_> = ["f", "g", "g2", "h"]
            .iter()
            .flat_map(|f| calls_of(s, one(s, f)))
            .map(|x| args(s, x))
            .collect();
        let none = vec![None];
        let want = vec![
            none.clone(),
            some(&["x"]),
            none.clone(),
            none,
            vec![Some("1".to_owned()), None, Some("2".to_owned())],
            some(&["1"]),
            some(&["(2)"]),
        ];
        assert_eq!(a, want);
        let x = calls_of(s, one(s, "g"))[0];
        let why = s.no_extent(s.arguments(x).unwrap()[0]).unwrap();
        assert_eq!(why.reason, "not all of the argument");
    });
}

/// Rule 4: a member variable template named with template arguments
/// (`d.twice<7>`): Kiln records no arguments on the member access, so
/// its last token is not known.
#[test]
fn s6_6_rule4_a_member_variable_template() {
    let src = "\
struct V { template <int N> static constexpr int twice = 2 * N; static constexpr int one = 1; };
int k(int);
int use(V d, V* p) { return k(d.twice<7>) + k(p->twice<2>) + k(d.one) + k(V::twice<3>); }
";
    with(&[("main.cpp", src)], |_, s| {
        let a: Vec<_> = calls_of(s, one(s, "k")).into_iter().flat_map(|x| args(s, x)).collect();
        assert_eq!(a, vec![None, None, Some("d.one".to_owned()), None]);
        let x = calls_of(s, one(s, "k"))[0];
        let why = s.no_extent(s.arguments(x).unwrap()[0]).unwrap();
        assert_eq!(why.reason, "specialisation");
    });
}

/// Rule 4 with a reversed rewritten comparison (WP-084 "must not
/// assume" point 7: its begin and end are out of order): no extent.
#[test]
fn s6_6_rule4_a_rewritten_comparison_has_none() {
    let src = "\
struct S { bool operator==(int) const; };
int k(bool);
int g(S s) { return k(1 == s) + k(s == 1) + k(s != 1); }
";
    with(&[("main.cpp", src)], |_, s| {
        let a: Vec<_> = calls_of(s, one(s, "k")).into_iter().flat_map(|x| args(s, x)).collect();
        assert_eq!(a, vec![None, Some("s == 1".to_owned()), None]);
    });
}

// ---------------------------------------------------------------------------
// Review of WP-086: arguments through macros (F1), callees from macro
// arguments (F2), the implicit object (F3), calls held by types (F4), the
// guards (F5), `template` and destructors (F6).

/// Every reason an argument has none, in order.
fn arg_reasons(s: &KilnSubject<'_>, x: Expression) -> Vec<Option<&'static str>> {
    s.arguments(x).unwrap().into_iter().map(|a| s.no_extent(a).map(|w| w.reason)).collect()
}

/// Rule 4, "all of the argument", is decided in the token stream the
/// parser read: the call's `(`, `,` and `)` around the argument, wherever
/// they were written. A macro body that writes braces or parentheses
/// around a macro argument (`#define M(a) f({a})`) makes the argument
/// longer than the node; at tier 1 the tokens beside the file range are
/// the invocation's, not the call's.
#[test]
fn s6_6_rule4_an_argument_through_a_macro_is_all_of_the_expanded_argument() {
    let src = "\
struct O { bool a = false; long b = 0; };
int f(const O&);
int g(O);
int h(int);
#define M(a) f({a})
#define N(a) g({a})
#define P(a) h((a))
#define Q(a) h(a + 1)
#define R(a) h(a)
#define R2(a) R(a)
int use(O x, int a) { return M(x) + N(x) + g({x}) + P(a) + Q(a) + R(a) + R2(a) + h((a)); }
";
    with(&[("main.cpp", src)], |_, s| {
        let f = calls_of(s, one(s, "f"));
        assert_eq!(f.iter().map(|&x| args(s, x)).collect::<Vec<_>>(), vec![vec![None]]);
        assert_eq!(arg_reasons(s, f[0]), [Some("not all of the argument")]);
        let g: Vec<_> = calls_of(s, one(s, "g")).into_iter().map(|x| args(s, x)).collect();
        assert_eq!(g, vec![vec![None], vec![None]]);
        let h: Vec<_> = calls_of(s, one(s, "h")).into_iter().map(|x| args(s, x)).collect();
        // `P(a)`: the parentheses are the body's; `Q(a)`: `a` is part of
        // the argument; `R(a)` and `R2(a)` (forwarded through two
        // macros): the argument is `a`, at tier 1.
        let want = vec![vec![None], vec![None], some(&["a"]), some(&["a"]), some(&["(a)"])];
        assert_eq!(h, want);
        for x in &calls_of(s, one(s, "h"))[2..4] {
            let a = s.arguments(*x).unwrap()[0];
            assert_eq!(s.tiers(range(s, a).unwrap()), Some([false, true]));
        }
    });
}

/// Rule 1 with the callee's name from a macro argument or a `##` paste and
/// the `)` from the body: each is a call (Kiln numbers an argument's
/// tokens after the body's, so no order of locations decides it). The
/// call's extent is the invocation (tier 0) or null.
#[test]
fn s6_7_rule1_a_callee_from_a_macro_argument() {
    let src = "\
int f(int);
int handle_a();
struct O { int mm(int); };
#define CALL1(fn) fn(1)
#define CALL(fn, x) fn(x)
#define ID(x) x
#define X(n) handle_##n()
#define MEM(o, m) o.m(3)
int use(O o) { return CALL1(f) + CALL(f, 2) + CALL(ID(f), 9) + X(a) + MEM(o, mm); }
";
    with(&[("main.cpp", src)], |_, s| {
        assert_eq!(calls(s, "f"), some(&["CALL1(f)", "CALL(f, 2)", "CALL(ID(f), 9)"]));
        let a: Vec<_> = calls_of(s, one(s, "f")).into_iter().map(|x| args(s, x)).collect();
        assert_eq!(a, vec![vec![None], some(&["2"]), some(&["9"])]);
        assert_eq!(calls(s, "handle_a"), some(&["X(a)"]));
        assert_eq!(calls(s, "O::mm"), some(&["MEM(o, mm)"]));
        let c = s.extent_census();
        assert_eq!((c.calls, c.without_reference, c.not_written, c.visited), (5, 0, 0, 5));
    });
}

/// §6.7: every call expression the walk visits is returned or counted;
/// no test of position drops one. A user-defined literal and the call of
/// a conversion function Kiln makes are not written `callee ( … )`
/// (counted as not written); a destructor called by name has no table
/// entry naming its callee (counted without a reference; decided in
/// `spec/landed/api/WP-086.md`).
#[test]
fn s6_7_every_call_visited_is_returned_or_counted() {
    let src = "\
int f(int);
struct T { operator int() const; };
struct D { ~D(); };
unsigned long long operator\"\"_k(unsigned long long);
int use(T t, D* p) { int a = t; p->~D(); return f(a) + int(1_k); }
";
    with(&[("main.cpp", src)], |_, s| {
        let c = s.extent_census();
        assert_eq!(calls(s, "f"), some(&["f(a)"]));
        assert_eq!((c.calls, c.not_written, c.without_reference), (1, 2, 1));
        assert_eq!(c.visited, c.calls + c.not_written + c.without_reference);
        assert_eq!(s.calls_without_reference(), 1);
    });
}

/// Rule 5: a function with an explicit object parameter called with the
/// implicit object (`f(2)` in a member) has Kiln's object as its first
/// argument: it is not written, so it is no argument.
#[test]
fn s6_7_rule5_an_implicit_object_is_no_argument() {
    let src = "\
struct S { int f(this S self, int); int g(this const S&, int, int); int h() const; };
int S::h() const { return f(2) + g(3, 4); }
";
    with(&[("main.cpp", src)], |_, s| {
        let f = calls_of(s, one(s, "S::f"));
        assert_eq!(ext(s, f[0]).as_deref(), Some("f(2)"));
        assert_eq!(args(s, f[0]), some(&["2"]));
        let g = calls_of(s, one(s, "S::g"));
        assert_eq!(args(s, g[0]), some(&["3", "4"]));
    });
}

/// §6.7 rule 3 in types: a call in an expression a type holds is a call
/// of the unit. Each form, and how the walk reaches it.
macro_rules! call_in_type {
    ($name:ident, $how:literal, $src:literal) => {
        #[doc = $how]
        #[test]
        fn $name() {
            let src = concat!(
                "constexpr int f(int x) { return x; }\n",
                "template <int N> struct A { static constexpr int v = N; };\n",
                $src,
                "\n"
            );
            with(&[("main.cpp", src)], |_, s| {
                assert_eq!(calls(s, "f"), some(&["f(1)"]));
                let c = s.extent_census();
                assert_eq!((c.calls, c.visited), (1, 1));
            });
        }
    };
}

call_in_type!(
    s6_7_rule3_in_a_non_type_parameter_default,
    "The template head's parameters: their defaults.",
    "template <int N = f(1)> struct T {};"
);
call_in_type!(
    s6_7_rule3_in_a_type_parameter_default,
    "A type parameter's default is a type: its template arguments.",
    "template <class T = A<f(1)>> struct T2 {};"
);
call_in_type!(
    s6_7_rule3_in_decltype,
    "The declared type: `decltype`'s expression.",
    "decltype(f(1)) d = 0;"
);
call_in_type!(
    s6_7_rule3_in_a_template_argument,
    "The declared type: a specialisation's arguments.",
    "A<f(1)> av;"
);
call_in_type!(
    s6_7_rule3_in_a_parameter_type,
    "The function's type: its parameters' types.",
    "void k(A<f(1)>);"
);
call_in_type!(
    s6_7_rule3_in_noexcept,
    "The function's type: its exception specification.",
    "void h() noexcept(f(1) == 1);"
);
call_in_type!(
    s6_7_rule3_in_alignas_of_a_class,
    "The declaration's `alignas` attribute: its expression.",
    "struct alignas(f(1)) Q {};"
);
call_in_type!(
    s6_7_rule3_in_alignas_of_a_type,
    "The declaration's `alignas` attribute: its type.",
    "alignas(A<f(1)>) char c;"
);
call_in_type!(
    s6_7_rule3_in_a_base_specifier,
    "The class's bases: each base's type.",
    "struct B : A<f(1)> {};"
);
call_in_type!(
    s6_7_rule3_in_an_alias_target,
    "An alias's type is its own typedef type: its underlying type.",
    "using AT = A<f(1)>;"
);
call_in_type!(
    s6_7_rule3_in_an_alias_template_target,
    "An alias template's pattern, then its underlying type.",
    "template <class T> using AA = A<f(1) + sizeof(T)>;"
);
call_in_type!(
    s6_7_rule3_in_a_typedef,
    "A typedef's type is its own typedef type: its underlying type.",
    "typedef A<f(1)> TT;"
);
call_in_type!(
    s6_7_rule3_in_an_enum_base,
    "A fixed underlying type as written.",
    "enum E : decltype(f(1)) { e0 };"
);
call_in_type!(
    s6_7_rule3_in_explicit,
    "`explicit(…)`: Kiln's record of the expression.",
    "struct C { explicit(f(1) == 1) C(int); };"
);
call_in_type!(
    s6_7_rule3_in_a_conversion_type,
    "A conversion function's type: its return type.",
    "struct C { operator A<f(1)>() const; };"
);
call_in_type!(
    s6_7_rule3_in_a_type_requirement,
    "A requires-expression's type requirement.",
    "template <class T> concept K = requires { typename A<f(1)>; };"
);
call_in_type!(s6_7_rule3_in_new, "A `new`'s allocated type.", "void g() { delete new A<f(1)>; }");
call_in_type!(
    s6_7_rule3_in_a_temporary_type,
    "`T()`: the expression's type.",
    "int g() { return decltype(A<f(1)>())::v; }"
);
call_in_type!(
    s6_7_rule3_in_a_local_decltype,
    "A local variable's declared type.",
    "int g() { decltype(f(1)) b = 0; return b; }"
);
call_in_type!(
    s6_7_rule3_in_sizeof_a_type,
    "`sizeof`'s type operand.",
    "int g() { return sizeof(A<f(1)>); }"
);
call_in_type!(
    s6_7_rule3_in_a_cast_type,
    "A cast's written type.",
    "int g() { return static_cast<A<f(1)>*>(nullptr) != nullptr; }"
);
call_in_type!(
    s6_7_rule3_in_a_qualifier,
    "A name's qualifier: its type.",
    "int g() { return A<f(1)>::v; }"
);
call_in_type!(
    s6_7_rule3_in_a_name_template_argument,
    "A name's template arguments.",
    "template <int N> constexpr int vt = N;\nint g() { return vt<f(1)>; }"
);
call_in_type!(
    s6_7_rule3_in_assume,
    "A statement's `assume` attribute: its expression.",
    "void g(int x) { [[assume(f(1) == x)]]; }"
);
call_in_type!(
    s6_7_rule3_in_enable_if,
    "A function's `enable_if` attribute: its condition.",
    "int g(int x) __attribute__((enable_if(f(1) == 1, \"\")));"
);
call_in_type!(
    s6_7_rule3_in_a_lambda_return_type,
    "A lambda's call operator's type.",
    "auto lam = []() -> decltype(f(1)) { return 0; };"
);

/// §6.7 rule 3, a hole of version 0: an array bound written in a
/// declarator. Kiln keeps the value of a constant bound but not the
/// expression (`size_expr` is `None`), so the call is not in the tree.
#[test]
#[ignore = "pending Kiln: no expression for a constant array bound (docs/wp/WP-086.md \"For Kiln\")"]
fn s6_7_rule3_pending_in_an_array_bound() {
    // The reviewer's lines.
    let src = "\
constexpr int f(int x) { return x; }
int arr[f(2)];
int body() { int a[f(11)]; return sizeof(int[f(13)]) + a[0]; }
";
    with(&[("main.cpp", src)], |_, s| {
        assert_eq!(calls(s, "f"), some(&["f(2)", "f(11)", "f(13)"]));
    });
}

/// §6.6 rule 3 with the guard on a qualified name's first token: Kiln's
/// begin of `Q::f` is `f` when a member calls a static overload through
/// its class (`For Kiln` 1), so the first token is not known and the
/// call has no extent; never `f(*this, 1)`.
#[test]
fn s6_6_rule3_a_qualified_name_whose_begin_is_its_name() {
    let src = "\
struct Q;
int f(const Q&, int);
struct Q {
  static int f(const Q&, int);
  int f(int) const;
  int g() const { return Q::f(*this, 1) + ::Q::f(*this, 2) + ::f(*this, 3); }
};
";
    with(&[("main.cpp", src)], |_, s| {
        let c = s.extent_census();
        assert_eq!(c.calls, 3);
        let why: Vec<_> = c.calls_null.iter().map(|(w, n)| (w.reason, *n)).collect();
        assert_eq!(why, [("qualifier", 2)]);
        assert_eq!(calls(s, "::f"), some(&["::f(*this, 3)"]));
    });
}

/// §6.6 rule 3 for `T<…>(x)`: the type's name is a template's, written
/// first; `::T<…>(x)` begins at `::`, where no entry starts: null.
#[test]
fn s6_6_rule3_a_construction_named_by_a_template() {
    let src = "\
template <class T> struct W { W(T); };
int k(W<int>);
int use() { return k(W<int>(1)) + k(::W<int>(2)) + k(W<int>{3}); }
";
    with(&[("main.cpp", src)], |_, s| {
        let a: Vec<_> = calls_of(s, one(s, "k")).into_iter().flat_map(|x| args(s, x)).collect();
        assert_eq!(a, vec![Some("W<int>(1)".to_owned()), None, Some("W<int>{3}".to_owned())]);
    });
}

/// Rule 1 with a member named after `template`, with and without a
/// qualifier: calls, found through the table's template-name entry.
#[test]
fn s6_7_rule1_a_member_named_after_template() {
    let src = "\
struct D { template <class T> int t(); };
struct E : D {};
int use(D d, E* p) { return d.template t<int>() + p->D::template t<long>() + d.t<char>(); }
";
    with(&[("main.cpp", src)], |_, s| {
        let want = ["d.template t<int>()", "p->D::template t<long>()", "d.t<char>()"];
        assert_eq!(calls(s, "D::t"), some(&want));
        assert_eq!(s.calls_without_reference(), 0);
    });
}

/// Rule 1 and a destructor called by name (`p->~D()`): Kiln's table
/// names `D`, not the callee's location, so it is no call of version 0
/// and is counted without a reference. A pseudo-destructor call
/// (`ip->~I()`, `I` a scalar type) calls no function: neither.
#[test]
fn s6_7_rule1_a_destructor_call_is_counted() {
    let src = "\
struct D { ~D(); };
using I = int;
void use(D* p, I* ip) { p->~D(); ip->~I(); }
";
    with(&[("main.cpp", src)], |_, s| {
        let c = s.extent_census();
        assert_eq!((c.calls, c.without_reference, c.not_written, c.visited), (0, 1, 0, 1));
    });
}

// ---------------------------------------------------------------------------
// Depth: the walk and the extent rule keep their own stacks.

/// A 10,000-deep `a + a + … + a` as an argument: its extent is the whole
/// sum, its first and last token found by loops.
#[test]
fn depth_a_sum_ten_thousand_deep() {
    let n = 10_000;
    let sum = vec!["a"; n].join("+");
    let src = format!("int f(int);\nint g(int a) {{ return f({sum}); }}\n");
    with(&[("main.cpp", src.as_str())], |_, s| {
        let xs = calls_of(s, one(s, "f"));
        assert_eq!(args(s, xs[0]), vec![Some(sum.clone())]);
    });
}

/// A 10,000-deep nest of parentheses as an argument, and calls nested as
/// arguments of calls: every call is found once, with its extent.
#[test]
fn depth_parentheses_and_calls_ten_thousand_deep() {
    let n = 10_000;
    let parens = format!("{}a{}", "(".repeat(n), ")".repeat(n));
    let nest = format!("{}a{}", "f(".repeat(n), ")".repeat(n));
    let src = format!("int f(int);\nint g(int a) {{ return f({parens}) + {nest}; }}\n");
    with(&[("main.cpp", src.as_str())], |_, s| {
        let xs = calls_of(s, one(s, "f"));
        assert_eq!(xs.len(), n + 1);
        assert_eq!(args(s, xs[0]), vec![Some(parens.clone())]);
        assert_eq!(ext(s, xs[1]), Some(nest.clone()));
        assert_eq!(ext(s, xs[n]).as_deref(), Some("f(a)"));
    });
}

// ---------------------------------------------------------------------------
// What a consumer of `slip_kiln::ranges` must not assume
// (`spec/landed/api/WP-084.md`), points 1, 2, 3, 6 and 7; points 4 and 5
// are sentences of `spec/landed/api/WP-086.md`.

/// Point 1: implicit nodes have ranges in `ranges`; here they have none
/// of their own. Kiln's calls (a conversion function's, a range-for's
/// `begin` and `end`) are no calls, and an implicit `this` is not where a
/// member call begins.
#[test]
fn wp084_point1_implicit_nodes_are_no_calls_and_have_no_extent() {
    let src = "\
struct T { operator int() const; };
struct R { int* begin(); int* end(); };
int i(int);
struct M { int m(int); int k() { return m(1); } };
int g(T t, R r) { for (int x : r) i(x); return i(t); }
";
    with(&[("main.cpp", src)], |_, s| {
        assert!(calls(s, "R::begin").is_empty() && calls(s, "R::end").is_empty());
        assert_eq!(s.calls_without_reference(), 0);
        assert_eq!(calls(s, "i"), some(&["i(x)", "i(t)"]));
        assert_eq!(calls(s, "M::m"), some(&["m(1)"]));
        assert_eq!(call_count(s), 3);
    });
}

/// Point 2: a header entered twice gives one range for the calls of both
/// entries; each entry's call is a call of its own, and the range is
/// reached at tier 0 by both.
#[test]
fn wp084_point2_a_header_entered_twice() {
    let h = "int y = f(1);\n";
    let main =
        "int f(int);\nnamespace a {\n#include \"h.h\"\n}\nnamespace b {\n#include \"h.h\"\n}\n";
    with(&[("main.cpp", main), ("h.h", h)], |_, s| {
        let xs = calls_of(s, one(s, "f"));
        assert_eq!(xs.len(), 2);
        assert_ne!(xs[0], xs[1]);
        assert_eq!(range(s, xs[0]), range(s, xs[1]));
        assert_eq!(ext(s, xs[0]).as_deref(), Some("f(1)"));
        assert_eq!(s.tiers(range(s, xs[0]).unwrap()), Some([true, false]));
    });
}

/// Point 3: a tier 1 range stands for every use of the argument: a call
/// in an argument used twice is two calls with one range.
#[test]
fn wp084_point3_an_argument_used_twice() {
    let src = "\
int f(int);
int g(int);
#define TWICE(x) (f(x) + f(x))
int y = TWICE(g(1));
";
    with(&[("main.cpp", src)], |_, s| {
        let xs = calls_of(s, one(s, "g"));
        assert_eq!(xs.len(), 2);
        assert_eq!(ext(s, xs[0]).as_deref(), Some("g(1)"));
        assert_eq!(range(s, xs[0]), range(s, xs[1]));
        assert_eq!(s.tiers(range(s, xs[0]).unwrap()), Some([false, true]));
        // `f(x)`'s tokens are the replacement list's: no extent, and the
        // place is the invocation.
        let fs = calls_of(s, one(s, "f"));
        assert_eq!(ext(s, fs[0]), None);
        let PlaceOf::Nearest(r) = s.place(Value::Expression(fs[0])) else { panic!("nearest") };
        assert_eq!(s.text(r), b"TWICE(g(1))");
        // Its argument `x` came from the argument: tier 1.
        assert_eq!(args(s, fs[0]), some(&["g(1)"]));
    });
}

/// Point 6: an extent is offsets in Kiln's text, the file's bytes only
/// when Kiln changed nothing; a file that is not valid UTF-8 gives no
/// extent (`spec/edits.md` §2.3 `file_recoded`). Through a real parse of a
/// tree on disk.
#[test]
fn wp084_point6_a_recoded_file_gives_no_extent() {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("wp086-recoded");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("main.cpp"), b"int f(int);\n// \xff\nint y = f(1);\n").unwrap();
    std::fs::write(dir.join("ok.cpp"), b"int f(int);\n// ok\nint y = f(1);\n").unwrap();
    for (file, want) in [("main.cpp", None), ("ok.cpp", Some("f(1)".to_owned()))] {
        let got = on_disk(&dir, file, |s| {
            let xs = calls_of(s, one(s, "f"));
            assert_eq!(xs.len(), 1);
            ext(s, xs[0])
        });
        assert_eq!(got, want, "{file}");
    }
}

/// Point 7: begin and end out of order (a reversed rewritten comparison)
/// give no range in `ranges`; the extent of the written form is none
/// either (test `s6_6_rule4_a_rewritten_comparison_has_none`), never one
/// computed from the semantic form.
#[test]
fn wp084_point7_out_of_order_has_no_extent() {
    let src = "\
struct S { bool operator==(int) const; };
int k(bool);
int g(S s) { return k(1 == s); }
";
    with(&[("main.cpp", src)], |_, s| {
        let xs = calls_of(s, one(s, "k"));
        let a = s.arguments(xs[0]).unwrap()[0];
        assert_eq!(ext(s, a), None);
        assert_eq!(format!("{:?}", s.no_extent(a).unwrap().kind), "CXXRewrittenBinaryOperator");
        assert_eq!(ext(s, xs[0]).as_deref(), Some("k(1 == s)"));
    });
}

/// Parses `file` in `dir` as a unit of a run (with the tables) and calls
/// `f` with the subject over it.
fn on_disk<R: Send>(
    dir: &std::path::Path,
    file: &str,
    f: impl FnOnce(&KilnSubject<'_>) -> R + Send,
) -> R {
    use slip_model::{Command, Digest, FileRef, Language, Observer, Origin, Phase};
    let argv = ["clang++", "-std=c++20", "-c", file];
    let observer = Observer {
        id: Digest([5; 32]),
        command: Command {
            directory: dir.to_str().unwrap().as_bytes().to_vec(),
            argv: argv.iter().map(|a| a.as_bytes().to_vec()).collect(),
            env: Vec::new(),
        },
        language: Language::Cxx,
        main: FileRef { path: Vec::new(), size: 0, content: Digest([0; 32]) },
        cost_hint: 1,
        origins: vec![Origin { configuration: 0, entry: 0 }],
    };
    let mut options = slip_kiln::ParseOptions::default();
    options.written = true;
    slip_kiln::parse_observer_with(&observer, Phase::Map, 0, options, |unit| {
        let owned = |_: &[u8]| true;
        f(&KilnSubject::new(unit, &owned))
    })
    .expect("the tree parses")
    .1
    .expect("the closure ran")
}

// ---------------------------------------------------------------------------
// §7.4 rule 3 and §5.4: place and tiers.

/// The place of an expression with an extent is its extent; of one whose
/// tokens a macro's replacement list wrote, the invocation; of one with
/// no extent and no macro, unnamed.
#[test]
fn s7_4_rule3_the_place_of_an_expression() {
    let src = "\
int f(int);
#define CALL f(2)
int a = f(1) + CALL + f(new int(3) != nullptr);
";
    with(&[("main.cpp", src)], |_, s| {
        let xs = calls_of(s, one(s, "f"));
        let place = |x| s.place(Value::Expression(x));
        let PlaceOf::Own(r) = place(xs[0]) else { panic!("own") };
        assert_eq!(s.text(r), b"f(1)");
        // `CALL` is the whole expansion: the call's extent is the
        // invocation, its own place.
        assert_eq!(ext(s, xs[1]).as_deref(), Some("CALL"));
        // Its argument `2` is of no tier: its place is the invocation.
        let two = s.arguments(xs[1]).unwrap()[0];
        assert_eq!(ext(s, two), None);
        let PlaceOf::Nearest(r) = place(two) else { panic!("nearest") };
        assert_eq!(s.text(r), b"CALL");
        let a = s.arguments(xs[2]).unwrap()[0];
        assert_eq!(s.place(Value::Expression(a)), PlaceOf::Unnamed);
        assert!(s.display(Value::Expression(xs[0])).starts_with("a call at main.cpp:3:"));
    });
}

/// §5.4: `tiers` answers for exactly the extents given out; a range
/// inside one, or one of no live expression, is `None`.
#[test]
fn s5_4_tiers_only_for_extents_given() {
    let src = "int f(int, int);\nint a = f(1, 22);\n";
    with(&[("main.cpp", src)], |_, s| {
        let x = calls_of(s, one(s, "f"))[0];
        let r = range(s, x).unwrap();
        assert_eq!(s.tiers(r), Some([true, false]));
        assert_eq!(s.tiers(Range { lo: r.lo + 1, ..r }), None);
        assert_eq!(s.tiers(Range { hi: r.hi - 1, ..r }), None);
        for a in s.arguments(x).unwrap() {
            assert_eq!(s.tiers(range(s, a).unwrap()), Some([true, false]));
        }
        // `22`'s first byte: no expression has it for extent.
        let b = range(s, s.arguments(x).unwrap()[1]).unwrap();
        assert_eq!(s.tiers(Range { hi: b.lo + 1, ..b }), None);
    });
}

/// Only calls and their arguments are live expressions; the trait's
/// numbers of other expressions are not.
#[test]
fn s5_1_only_calls_and_arguments_are_live() {
    let src = "int f(int);\nint a = f(1 + 2) * 3;\n";
    with(&[("main.cpp", src)], |p, s| {
        let x = calls_of(s, one(s, "f"))[0];
        let a = s.arguments(x).unwrap()[0];
        let live: Vec<u32> = (1..=p.ast.exprs.len() as u32)
            .filter(|&n| s.is_live(Value::Expression(Expression(n))))
            .collect();
        let mut want = vec![x.0, a.0];
        want.sort_unstable();
        assert_eq!(live, want);
        assert!(!s.is_live(Value::Expression(Expression(0))));
        assert_eq!(s.callee(a), None);
        assert_eq!(s.arguments(a), None);
        assert_eq!(s.callee_reference(a), None);
        assert_eq!(s.kind_bits(Value::Expression(a)), wire::KIND_EXPRESSION);
    });
}

// ---------------------------------------------------------------------------
// Item 7: the differential against clang 19's `-ast-dump=json`.
//
// Every snippet of the digest's pair tests, both sides, is parsed by Kiln
// (through the subject) and by clang 19. A call is keyed by the file and
// offset of its `)`: the one Kiln stores, and the end of the range of
// clang's `CallExpr` or `CXXMemberCallExpr` whose callee (after implicit
// casts and parentheses) is a `DeclRefExpr` or a `MemberExpr`. A call
// clang has not is counted and printed, not an error: clang 19 skips the
// code under `__has_builtin` of a clang 22 builtin Kiln has, and its dump
// leaves out the elements of an initializer list with an array filler.
// Every non-null extent
// of a call and of each of its arguments must be the range clang gives
// the same node. A range clang gives through a macro is not compared.

/// The digest's pair-test roots (`crates/slip_digest/tests/`).
const PAIR_ROOTS: &[&str] = &["differ", "equal", "demand", "indeterminate"];

/// The clang the differential runs.
const CLANG: &str = "clang++-19";

/// One side of a pair test (as `crates/slip_kiln/tests/ranges.rs`).
#[derive(Clone, Debug)]
struct Side {
    files: Vec<(String, String)>,
    std: String,
    args: Vec<String>,
    target: Option<String>,
}

/// Every directory under `root` holding a `t.cpp`, an `a.cpp` or a
/// `b.cpp`, sorted.
fn pair_dirs(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        let mut mains = false;
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if matches!(e.file_name().to_str(), Some("t.cpp" | "a.cpp" | "b.cpp")) {
                mains = true;
            }
        }
        if mains {
            out.push(d);
        }
    }
    out.sort();
    out
}

/// The two sides of a pair test as the digest's runner builds them
/// (`crates/slip_digest/tests/common/mod.rs`): `side.h` for a `t.cpp`,
/// every other file of the directory, the support files it lacks, and the
/// header lines of the main file. The main file is `main.cpp`.
fn pair_sides(dir: &std::path::Path, support: &[(String, String)]) -> Vec<Side> {
    use std::collections::BTreeMap;
    let mut all: BTreeMap<String, String> = BTreeMap::new();
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        if e.path().is_file()
            && let (Some(n), Ok(t)) =
                (e.file_name().to_str().map(str::to_owned), std::fs::read_to_string(e.path()))
        {
            all.insert(n, t);
        }
    }
    let mut sides = Vec::new();
    for side in ["a", "b"] {
        let mut files = Vec::new();
        let main = if let Some(t) = all.get("t.cpp") {
            t.clone()
        } else if let Some(m) = all.get(&format!("{side}.cpp")) {
            m.clone()
        } else {
            continue;
        };
        files.push(("main.cpp".to_owned(), main.clone()));
        if all.contains_key("t.cpp") {
            files
                .push(("side.h".to_owned(), if side == "a" { "" } else { "#define B 1\n" }.into()));
        }
        for (n, t) in &all {
            if !matches!(n.as_str(), "t.cpp" | "a.cpp" | "b.cpp" | "expect.txt") {
                files.push((n.clone(), t.clone()));
            }
        }
        for (n, t) in support {
            if !all.contains_key(n) {
                files.push((n.clone(), t.clone()));
            }
        }
        let mut s = Side { files, std: "c++20".to_owned(), args: Vec::new(), target: None };
        for line in main.lines() {
            let line = line.trim_end();
            if line == "#include \"side.h\"" {
                continue;
            }
            let Some((key, value)) = line.strip_prefix("// ").and_then(|r| r.split_once(": "))
            else {
                break;
            };
            match key {
                "std" => value.clone_into(&mut s.std),
                "args" => {
                    s.args =
                        value.split(' ').filter(|a| !a.is_empty()).map(str::to_owned).collect();
                }
                "target" => s.target = Some(value.to_owned()),
                "errors" => {}
                _ => break,
            }
        }
        sides.push(s);
    }
    sides
}

/// The number of threads: `SLIP_TEST_THREADS`, else at most 4.
fn test_threads() -> usize {
    match std::env::var("SLIP_TEST_THREADS") {
        Ok(v) => v.parse::<usize>().expect("SLIP_TEST_THREADS: a positive number").max(1),
        Err(_) => std::thread::available_parallelism().map_or(1, std::num::NonZero::get).min(4),
    }
}

/// A JSON value, objects in document order.
enum J {
    Obj(Vec<(String, J)>),
    Arr(Vec<J>),
    Str(String),
    Num(i64),
    Bool(bool),
    Other,
}

impl J {
    fn get(&self, key: &str) -> Option<&J> {
        match self {
            J::Obj(kv) => kv.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
}

/// A JSON reader for clang's dump (recursive: run on a thread of
/// `slip_digest::STACK_SIZE`).
struct Reader<'a> {
    b: &'a [u8],
    i: usize,
}

impl Reader<'_> {
    fn ws(&mut self) {
        while self.b.get(self.i).is_some_and(u8::is_ascii_whitespace) {
            self.i += 1;
        }
    }

    fn value(&mut self) -> J {
        self.ws();
        match self.b[self.i] {
            b'{' => {
                self.i += 1;
                let mut kv = Vec::new();
                loop {
                    self.ws();
                    if self.b[self.i] == b'}' {
                        self.i += 1;
                        break;
                    }
                    if self.b[self.i] == b',' {
                        self.i += 1;
                        self.ws();
                    }
                    let k = self.string();
                    self.ws();
                    assert_eq!(self.b[self.i], b':');
                    self.i += 1;
                    kv.push((k, self.value()));
                }
                J::Obj(kv)
            }
            b'[' => {
                self.i += 1;
                let mut v = Vec::new();
                loop {
                    self.ws();
                    match self.b[self.i] {
                        b']' => {
                            self.i += 1;
                            break;
                        }
                        b',' => self.i += 1,
                        _ => v.push(self.value()),
                    }
                }
                J::Arr(v)
            }
            b'"' => J::Str(self.string()),
            _ => {
                let start = self.i;
                while !matches!(self.b[self.i], b',' | b'}' | b']')
                    && !self.b[self.i].is_ascii_whitespace()
                {
                    self.i += 1;
                }
                match &self.b[start..self.i] {
                    b"true" => J::Bool(true),
                    b"false" => J::Bool(false),
                    t => std::str::from_utf8(t)
                        .ok()
                        .and_then(|t| t.parse().ok())
                        .map_or(J::Other, J::Num),
                }
            }
        }
    }

    fn string(&mut self) -> String {
        assert_eq!(self.b[self.i], b'"');
        self.i += 1;
        let mut out = Vec::new();
        loop {
            match self.b[self.i] {
                b'"' => {
                    self.i += 1;
                    break;
                }
                b'\\' => {
                    let c = self.b[self.i + 1];
                    self.i += 2;
                    match c {
                        b'n' => out.push(b'\n'),
                        b't' => out.push(b'\t'),
                        b'r' => out.push(b'\r'),
                        b'b' => out.push(8),
                        b'f' => out.push(12),
                        b'u' => {
                            let h = std::str::from_utf8(&self.b[self.i..self.i + 4]).unwrap();
                            let c = char::from_u32(u32::from_str_radix(h, 16).unwrap())
                                .unwrap_or('\u{fffd}');
                            out.extend_from_slice(c.to_string().as_bytes());
                            self.i += 4;
                        }
                        c => out.push(c),
                    }
                }
                c => {
                    out.push(c);
                    self.i += 1;
                }
            }
        }
        String::from_utf8_lossy(&out).into_owned()
    }
}

/// A location of clang's dump: a file, a byte offset and the length of
/// the token there; or one through a macro (where its token was spelled,
/// and the start of the outermost invocation, each as a file location);
/// or none.
#[derive(Clone, Debug, PartialEq, Eq)]
enum At {
    File(String, u32, u32),
    Macro(Option<(String, u32, u32)>, Option<(String, u32, u32)>),
    None,
}

impl At {
    fn file(self) -> Option<(String, u32, u32)> {
        match self {
            At::File(f, o, l) => Some((f, o, l)),
            _ => None,
        }
    }

    /// Where the token was spelled: its file location, or where a macro's
    /// token was written.
    fn spelled(&self) -> Option<(&str, u32, u32)> {
        match self {
            At::File(f, o, l) | At::Macro(Some((f, o, l)), _) => Some((f, *o, *l)),
            _ => None,
        }
    }

    /// Its file location, or the start of the outermost invocation.
    fn outer(&self) -> Option<(&str, u32, u32)> {
        match self {
            At::File(f, o, l) | At::Macro(_, Some((f, o, l))) => Some((f, *o, *l)),
            _ => None,
        }
    }
}

/// A node of clang's dump: its kind, range, children, and what the
/// reverse direction needs: an implicit cast's kind, the kind of the
/// declaration a name refers to, a member's name, whether a declaration
/// is implicit or explicitly defaulted.
struct Node {
    kind: String,
    begin: At,
    end: At,
    inner: Vec<Node>,
    cast: String,
    decl_kind: String,
    name: String,
    implicit: bool,
    defaulted: bool,
    id: String,
    member: String,
}

/// A path as a snippet path: `.` and `..` resolved by their spelling.
fn snippet_path(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for c in path.split('/') {
        match c {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            c => parts.push(c),
        }
    }
    parts.join("/")
}

/// The dump prints a location's file only when it differs from the last
/// one printed, in document order (`JSONNodeDumper::writeBareSourceLocation`;
/// `includedFrom` is not a location and does not count).
struct Dump {
    file: Option<String>,
}

impl Dump {
    fn bare(&mut self, j: &J) -> At {
        let J::Obj(kv) = j else { return At::None };
        if j.get("spellingLoc").is_some() || j.get("expansionLoc").is_some() {
            let (mut spelled, mut outer) = (None, None);
            for (k, v) in kv {
                match k.as_str() {
                    "spellingLoc" => spelled = self.bare(v).file(),
                    "expansionLoc" => outer = self.bare(v).file(),
                    _ => {}
                }
            }
            return At::Macro(spelled, outer);
        }
        let (mut off, mut len) = (None, None);
        for (k, v) in kv {
            match (k.as_str(), v) {
                ("offset", J::Num(n)) => off = u32::try_from(*n).ok(),
                ("tokLen", J::Num(n)) => len = u32::try_from(*n).ok(),
                ("file", J::Str(f)) => self.file = Some(snippet_path(f)),
                _ => {}
            }
        }
        match (off, len, &self.file) {
            (Some(o), Some(l), Some(f)) => At::File(f.clone(), o, l),
            _ => At::None,
        }
    }

    /// Every location under `j` that is not a node's, for the file state.
    fn scan(&mut self, j: &J) {
        match j {
            J::Obj(_) if j.get("offset").is_some() || j.get("spellingLoc").is_some() => {
                self.bare(j);
            }
            J::Obj(kv) => kv.iter().for_each(|(_, v)| self.scan(v)),
            J::Arr(v) => v.iter().for_each(|v| self.scan(v)),
            _ => {}
        }
    }

    fn node(&mut self, j: &J) -> Node {
        let mut n = Node {
            kind: String::new(),
            begin: At::None,
            end: At::None,
            inner: Vec::new(),
            cast: String::new(),
            decl_kind: String::new(),
            name: String::new(),
            implicit: false,
            defaulted: false,
            id: String::new(),
            member: String::new(),
        };
        let J::Obj(kv) = j else { return n };
        for (k, v) in kv {
            match (k.as_str(), v) {
                ("kind", J::Str(s)) => n.kind.clone_from(s),
                ("range", J::Obj(r)) => {
                    for (k, v) in r {
                        match k.as_str() {
                            "begin" => n.begin = self.bare(v),
                            "end" => n.end = self.bare(v),
                            _ => self.scan(v),
                        }
                    }
                }
                ("inner", J::Arr(v)) => n.inner = v.iter().map(|c| self.node(c)).collect(),
                ("castKind", J::Str(s)) => n.cast.clone_from(s),
                ("name", J::Str(s)) => n.name.clone_from(s),
                ("id", J::Str(s)) => n.id.clone_from(s),
                ("referencedMemberDecl", J::Str(s)) => n.member.clone_from(s),
                ("isImplicit", J::Bool(b)) => n.implicit = *b,
                ("explicitlyDefaulted", J::Str(_)) => n.defaulted = true,
                ("referencedDecl", v) => {
                    if let Some(J::Str(s)) = v.get("kind") {
                        n.decl_kind.clone_from(s);
                    }
                    self.scan(v);
                }
                _ => self.scan(v),
            }
        }
        n
    }
}

/// A range as (file, first byte, one past the last), when its two ends
/// are in one file.
type ClangRange = Option<(String, u32, u32)>;

/// Clang's range of a node three ways: in a file (both ends written in
/// one file, outside any macro); spelled (where its first and last
/// tokens were written, in one file, macro bodies included); and outer
/// (from the start of the outermost invocation of its first token to the
/// end of the token, or of the macro's name, where its last token was
/// expanded: the dump gives no invocation's end).
#[derive(Clone, Debug)]
struct ClangSpan {
    file: ClangRange,
    spelled: ClangRange,
    outer: ClangRange,
}

fn clang_span(n: &Node) -> ClangSpan {
    let join = |a: Option<(&str, u32, u32)>, b: Option<(&str, u32, u32)>| match (a, b) {
        (Some((f, lo, _)), Some((g, e, len))) if f == g => Some((f.to_owned(), lo, e + len)),
        _ => None,
    };
    let file = match (&n.begin, &n.end) {
        (At::File(f, lo, _), At::File(g, e, len)) if f == g => Some((f.clone(), *lo, e + len)),
        _ => None,
    };
    ClangSpan {
        file,
        spelled: join(n.begin.spelled(), n.end.spelled()),
        outer: join(n.begin.outer(), n.end.outer()),
    }
}

/// Clang's calls of a unit by their `)`: the call's range and its
/// arguments' ranges.
type ClangCalls = std::collections::HashMap<Close, Vec<(ClangSpan, Vec<ClangSpan>)>>;

/// Where a call's `)` is, for the reverse direction: written in a file,
/// or produced by a macro (where it was spelled, and the start of the
/// outermost invocation).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Close {
    File(String, u32),
    Macro((String, u32), (String, u32)),
}

/// Clang 19 begins a call with an explicit object (`b.xobj()`) at its
/// callee (`CallExpr::getBeginLoc`; clang 20 begins it at the object):
/// the call's first token is the object's. Corrected bottom-up, so that
/// such a call is right also where it is an argument or an operand.
fn begin_explicit_object_calls(n: &mut Node) {
    for c in &mut n.inner {
        begin_explicit_object_calls(c);
    }
    if n.kind != "CallExpr" || n.inner.len() < 2 {
        return;
    }
    let mut callee = &n.inner[0];
    while matches!(callee.kind.as_str(), "ImplicitCastExpr" | "ParenExpr")
        && callee.inner.len() == 1
    {
        callee = &callee.inner[0];
    }
    let object = first_at(&n.inner[1]).clone();
    if let (At::File(f, at, _), At::File(g, lo, _)) = (&callee.begin, &object)
        && f == g
        && lo < at
        && matches!(callee.kind.as_str(), "DeclRefExpr" | "MemberExpr")
    {
        n.begin = object;
    }
}

/// Where clang begins `n`: its begin, or, for a node clang gives none
/// (the implicit `*p` of an explicit object called through `p->`), its
/// first child's. A default argument's children are written elsewhere.
fn first_at(n: &Node) -> &At {
    match (&n.begin, n.inner.first()) {
        (At::None, Some(c)) if n.kind != "CXXDefaultArgExpr" => first_at(c),
        (b, _) => b,
    }
}

/// The callee of call `n` without parentheses and the decay of a
/// function to a pointer, when it is a name (`DeclRefExpr`) or a member
/// access (`MemberExpr`) of a function: not a variable of function
/// pointer type (an lvalue-to-rvalue conversion), not a reference to a
/// function (a name of a variable).
fn clang_callee<'n>(n: &'n Node, kinds: &Kinds<'_>) -> Option<&'n Node> {
    let mut callee = n.inner.first()?;
    loop {
        match callee.kind.as_str() {
            "ParenExpr" if callee.inner.len() == 1 => {}
            "ImplicitCastExpr"
                if callee.inner.len() == 1
                    && matches!(
                        callee.cast.as_str(),
                        "FunctionToPointerDecay" | "BuiltinFnToFnPtr"
                    ) => {}
            _ => break,
        }
        callee = &callee.inner[0];
    }
    let function = match callee.kind.as_str() {
        "DeclRefExpr" => {
            callee.decl_kind.ends_with("FunctionDecl")
                || matches!(
                    callee.decl_kind.as_str(),
                    "CXXMethodDecl"
                        | "CXXConversionDecl"
                        | "CXXDestructorDecl"
                        | "CXXConstructorDecl"
                )
        }
        // A member function (a member of the dump whose kind is not
        // known is taken for one); not a data member of a reference to a
        // function, or of a class type with `operator()` in a pattern.
        "MemberExpr" => kinds.get(callee.member.as_str()).is_none_or(|k| {
            matches!(*k, "CXXMethodDecl" | "CXXConversionDecl" | "CXXDestructorDecl")
        }),
        _ => false,
    };
    function.then_some(callee)
}

/// The kind of each declaration of the dump, by id.
type Kinds<'a> = std::collections::HashMap<&'a str, &'a str>;

/// The ids of the dump's declarations: their kinds, and the ones
/// dumped as an instantiation (after a template's pattern), which the
/// dump may repeat outside the template (a variable template's).
fn clang_decls<'a>(n: &'a Node, kinds: &mut Kinds<'a>, instances: &mut HashSet<&'a str>) {
    if !n.id.is_empty() && n.kind.ends_with("Decl") {
        kinds.insert(&n.id, &n.kind);
    }
    let pattern = template_pattern(n);
    for (i, c) in n.inner.iter().enumerate() {
        if pattern.is_some_and(|p| i > p) && !c.id.is_empty() {
            instances.insert(&c.id);
        }
        clang_decls(c, kinds, instances);
    }
}

/// A template's children: its parameters (and requires-clause), the
/// pattern, then its instantiations (`ASTNodeTraverser::dumpTemplateDecl`):
/// the index of the pattern.
fn template_pattern(n: &Node) -> Option<usize> {
    if !matches!(n.kind.as_str(), "FunctionTemplateDecl" | "ClassTemplateDecl" | "VarTemplateDecl")
    {
        return None;
    }
    n.inner.iter().position(|c| {
        c.kind.ends_with("Decl")
            && !matches!(
                c.kind.as_str(),
                "TemplateTypeParmDecl" | "NonTypeTemplateParmDecl" | "TemplateTemplateParmDecl"
            )
    })
}

/// What `clang_calls` reads besides the tree.
struct ClangContext<'a> {
    texts: &'a Texts<'a>,
    kinds: Kinds<'a>,
    instances: HashSet<&'a str>,
}

/// What the reverse direction keeps of a clang call: what it is (the
/// call's kind, the callee's, and where it is), keyed by its `)`.
type ClangSites = std::collections::HashMap<Close, String>;

/// The text of each file of a side, by snippet path.
type Texts<'a> = std::collections::HashMap<String, &'a [u8]>;

/// The calls of the dump under `n`. `within` says where `n` is: written
/// (empty), or in an instantiation, an implicit declaration, a defaulted
/// function (the places §6.7 rule 2 leaves out). `converted`: `n` is the
/// operand of a user-defined conversion (the conversion function's call
/// is Kiln's and clang's, not written).
fn clang_calls(
    n: &Node,
    within: &'static str,
    converted: bool,
    cx: &ClangContext<'_>,
    out: &mut ClangCalls,
    sites: &mut ClangSites,
) {
    if matches!(n.kind.as_str(), "CallExpr" | "CXXMemberCallExpr")
        && !converted
        && let Some(callee) = clang_callee(n, &cx.kinds)
    {
        let closes =
            |f: &str, o: u32| cx.texts.get(f).and_then(|t| t.get(o as usize)) == Some(&b')');
        let key = match &n.end {
            At::File(f, o, _) => Some(Close::File(f.clone(), *o)),
            At::Macro(Some((f, o, _)), Some((g, x, _))) => {
                Some(Close::Macro((f.clone(), *o), (g.clone(), *x)))
            }
            _ => None,
        };
        if let Some(key) = key {
            // An explicit object argument precedes the callee; an
            // implicit one (made from an implicit `this`) has no place.
            let object = |a: &&Node| match (first_at(a).spelled(), callee.begin.spelled()) {
                (Some((g, lo, _)), Some((h, at, _))) => g == h && lo < at,
                (None, _) => matches!(first_at(a), At::None),
                _ => false,
            };
            let written: Vec<&Node> =
                n.inner[1..].iter().filter(|a| a.kind != "CXXDefaultArgExpr").collect();
            let call = clang_span(n);
            let args = written.into_iter().filter(|a| !object(a)).map(clang_span).collect();
            out.entry(key.clone()).or_default().push((call, args));
            let written_close = match &key {
                Close::File(f, o) | Close::Macro((f, o), _) => closes(f, *o),
            };
            if written_close {
                let what = if callee.name.starts_with('~') { " (a destructor)" } else { "" };
                let through =
                    if matches!(key, Close::Macro(..)) { ", `)` from a macro" } else { "" };
                let kind = format!("{} of a {}{what}{through}{within}", n.kind, callee.kind);
                // A call dumped twice (a lambda's body, under its class
                // and under the expression) is written if one of them is.
                let e = sites.entry(key).or_insert_with(|| kind.clone());
                if within.is_empty() {
                    *e = kind;
                }
            }
        }
    }
    let here = if !within.is_empty() {
        within
    } else if cx.instances.contains(n.id.as_str()) {
        " in an instantiation"
    } else if n.implicit && n.kind.ends_with("Decl") {
        " in an implicit declaration"
    } else if n.defaulted {
        " in a defaulted function"
    } else {
        ""
    };
    let pattern = template_pattern(n);
    for (i, c) in n.inner.iter().enumerate() {
        let w = if here.is_empty() && pattern.is_some_and(|p| i > p) {
            " in an instantiation"
        } else {
            here
        };
        clang_calls(c, w, n.cast == "UserDefinedConversion", cx, out, sites);
    }
}

/// What the differential counted.
#[derive(Default)]
struct Diff {
    units: u64,
    kiln_errors: u64,
    clang_errors: u64,
    calls: u64,
    closed_by_macro: u64,
    unmatched: u64,
    compared: u64,
    compared_through_macro: u64,
    null: u64,
    clang_macro: u64,
    differences: Vec<String>,
    unmatched_at: Vec<String>,
    /// The reverse direction: clang's calls whose `)` is written, and
    /// those the subject has no call for, by kind.
    clang_calls: u64,
    missing: std::collections::BTreeMap<String, u64>,
    missing_at: Vec<String>,
}

impl Diff {
    fn add(&mut self, o: Diff) {
        self.units += o.units;
        self.kiln_errors += o.kiln_errors;
        self.clang_errors += o.clang_errors;
        self.calls += o.calls;
        self.closed_by_macro += o.closed_by_macro;
        self.unmatched += o.unmatched;
        self.compared += o.compared;
        self.compared_through_macro += o.compared_through_macro;
        self.null += o.null;
        self.clang_macro += o.clang_macro;
        self.differences.extend(o.differences);
        self.unmatched_at.extend(o.unmatched_at);
        self.clang_calls += o.clang_calls;
        for (k, n) in o.missing {
            *self.missing.entry(k).or_default() += n;
        }
        self.missing_at.extend(o.missing_at);
    }
}

/// Clang's dump of a side, or `None` when clang reports an error.
fn clang_dump(side: &Side, dir: &std::path::Path) -> Option<Vec<u8>> {
    for (p, t) in &side.files {
        let path = dir.join(p);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, t).unwrap();
    }
    let target = side.target.as_deref().unwrap_or("x86_64-unknown-linux-gnu");
    let out = std::process::Command::new(CLANG)
        .current_dir(dir)
        .arg(format!("-std={}", side.std))
        .arg(format!("--target={target}"))
        .args(&side.args)
        .args(["-nostdinc", "-nostdinc++", "-I.", "-w", "-fsyntax-only"])
        .args(["-Xclang", "-ast-dump=json", "main.cpp"])
        .output()
        .unwrap();
    std::fs::remove_dir_all(dir).unwrap();
    out.status.success().then_some(out.stdout)
}

/// The differential of one side.
fn differential(side: &Side, dir: &std::path::Path, label: &str) -> Diff {
    let mut d = Diff { units: 1, ..Diff::default() };
    let Some(json) = clang_dump(side, dir) else {
        d.clang_errors = 1;
        return d;
    };
    let texts: Texts<'_> =
        side.files.iter().map(|(p, t)| (snippet_path(p), t.as_bytes())).collect();
    let mut clang = ClangCalls::new();
    let mut sites = ClangSites::new();
    let root = Reader { b: &json, i: 0 }.value();
    let mut tree = Dump { file: None }.node(&root);
    begin_explicit_object_calls(&mut tree);
    let mut cx = ClangContext { texts: &texts, kinds: Kinds::new(), instances: HashSet::new() };
    clang_decls(&tree, &mut cx.kinds, &mut cx.instances);
    clang_calls(&tree, "", false, &cx, &mut clang, &mut sites);
    let files: Vec<(&str, &str)> =
        side.files.iter().map(|(p, t)| (p.as_str(), t.as_str())).collect();
    let snippet = Snippet { files: &files, main: "main.cpp", include: &[""], std: &side.std };
    let args: Vec<&str> = side.args.iter().map(String::as_str).collect();
    let extra = Extra { args: &args, target: side.target.as_deref() };
    let parsed = parse_with_tables(&snippet, &extra, |p| {
        if p.errors > 0 {
            d.kiln_errors = 1;
            return;
        }
        let owned = |_: &[u8]| true;
        let s = KilnSubject::from_snippet(p, &owned);
        let ours = |r: Option<Range>| {
            r.map(|r| {
                let path = String::from_utf8_lossy(s.file_path(r.file).unwrap()).into_owned();
                (path, r.lo, r.hi, s.tiers(r).unwrap_or([false, false]))
            })
        };
        let text = |p: &[u8]| String::from_utf8_lossy(p).into_owned();
        let mut ours_closed = HashSet::new();
        for site in s.call_sites() {
            let x = site.call;
            d.calls += 1;
            let key = match (site.close, site.close_in_macro) {
                (Some((path, at)), _) => Close::File(text(path), at),
                (None, Some((spelled, outer))) => {
                    d.closed_by_macro += 1;
                    Close::Macro((text(spelled.0), spelled.1), (text(outer.0), outer.1))
                }
                (None, None) => {
                    d.closed_by_macro += 1;
                    continue;
                }
            };
            ours_closed.insert(key.clone());
            let Some(cands) = clang.get(&key) else {
                d.unmatched += 1;
                d.unmatched_at.push(format!("{label}: {key:?}"));
                continue;
            };
            let args = s.arguments(x).unwrap();
            if !cands.iter().any(|(_, a)| a.len() == args.len()) {
                d.differences.push(format!(
                    "{label}: {key:?}: {} arguments, clang {:?}",
                    args.len(),
                    cands.iter().map(|(_, a)| a.len()).collect::<Vec<_>>()
                ));
                continue;
            }
            // An extent is clang's range of the node in a file. Where
            // clang's range comes through a macro: a range of tier 1 (the
            // tokens of a macro argument) is where clang's first and last
            // tokens were spelled; a range of tier 0 (an invocation) begins
            // where clang's outermost invocation begins and covers its
            // last token's.
            let mut check = |mine: Option<(String, u32, u32, [bool; 2])>,
                             theirs: Vec<&ClangSpan>,
                             what: &str| {
                let Some((f, lo, hi, [t0, t1])) = mine else {
                    d.null += 1;
                    return;
                };
                let m = Some((f.clone(), lo, hi));
                if theirs.iter().any(|t| t.file == m) {
                    d.compared += 1;
                    return;
                }
                if theirs.iter().any(|t| t.file.is_some()) {
                    d.differences.push(format!("{label}: {what} {m:?}, clang {theirs:?}"));
                    return;
                }
                let spelled = t1 && theirs.iter().any(|t| t.spelled == m);
                let outer = t0
                    && theirs.iter().any(|t| {
                        t.outer.as_ref().is_some_and(|(g, a, b)| *g == f && *a == lo && *b <= hi)
                    });
                if spelled || outer {
                    d.compared_through_macro += 1;
                } else if theirs.iter().all(|t| t.spelled.is_none() && t.outer.is_none()) {
                    d.clang_macro += 1;
                } else {
                    d.differences.push(format!(
                        "{label}: {what} {m:?} (tiers {t0} {t1}), clang through a macro {theirs:?}"
                    ));
                }
            };
            check(ours(range(&s, x)), cands.iter().map(|(c, _)| c).collect(), "call");
            for (i, &a) in args.iter().enumerate() {
                let theirs = cands.iter().filter_map(|(_, v)| v.get(i)).collect();
                check(ours(range(&s, a)), theirs, &format!("argument {i} of {key:?}"));
            }
        }
        // The reverse direction: clang's calls the subject has not.
        for (close, kind) in &sites {
            d.clang_calls += 1;
            if !ours_closed.contains(close) {
                *d.missing.entry(kind.clone()).or_default() += 1;
                d.missing_at.push(format!("{label}: {kind}: {close:?}"));
            }
        }
    });
    if parsed.is_err() {
        d.kiln_errors = 1;
    }
    d
}

/// Kiln's own C++ tests as sides: every `*.cpp` directly under `dir`, as
/// `main.cpp`, under `std`, with the headers of `dir/Inputs`.
fn kiln_sides(dir: &std::path::Path, std: &str) -> Vec<(String, Vec<Side>)> {
    let read = |d: &std::path::Path, ext: &str| -> Vec<(String, String)> {
        let mut v: Vec<(String, String)> = std::fs::read_dir(d)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == ext))
            .filter_map(|e| {
                Some((e.file_name().to_str()?.to_owned(), std::fs::read_to_string(e.path()).ok()?))
            })
            .collect();
        v.sort();
        v
    };
    let inputs: Vec<(String, String)> = read(&dir.join("Inputs"), "h")
        .into_iter()
        .map(|(n, t)| (format!("Inputs/{n}"), t))
        .collect();
    read(dir, "cpp")
        .into_iter()
        // A test Kiln marks as one clang 19 hangs or crashes on.
        .filter(|(_, t)| !t.contains("//@ skip(clang)"))
        .map(|(n, t)| {
            let mut files = vec![("main.cpp".to_owned(), t)];
            files.extend(inputs.iter().cloned());
            let side = Side { files, std: std.to_owned(), args: Vec::new(), target: None };
            (format!("{}/{n}", dir.display()), vec![side])
        })
        .collect()
}

/// Item 7: the extents of calls and arguments against clang 19, over
/// every pair-test snippet of the digest (both sides) and every test of
/// Kiln's `tests/codegen` (C++26); with `SLIP_WP086_CONFORMANCE=1`, also
/// every test of Kiln's `tests/conformance`. A missing clang is a
/// visible skip, and an error under `SLIP_REQUIRE_ALL=1`.
#[test]
fn item7_differential_against_clang() {
    let version = std::process::Command::new(CLANG).arg("--version").output();
    let Ok(version) = version.map(|o| String::from_utf8_lossy(&o.stdout).into_owned()) else {
        let reason = format!("no {CLANG} on PATH");
        assert!(
            std::env::var_os("SLIP_REQUIRE_ALL").is_none_or(|v| v != "1"),
            "SLIP_REQUIRE_ALL=1 and {reason}"
        );
        eprintln!("SKIP subject_calls.rs item7_differential_against_clang: {reason}");
        return;
    };
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let tests = root.join("../slip_digest/tests");
    let mut support: Vec<(String, String)> = Vec::new();
    for e in std::fs::read_dir(tests.join("support")).into_iter().flatten().flatten() {
        if let (Some(n), Ok(t)) =
            (e.file_name().to_str().map(str::to_owned), std::fs::read_to_string(e.path()))
        {
            support.push((n, t));
        }
    }
    support.sort();
    let dirs: Vec<_> = PAIR_ROOTS.iter().flat_map(|r| pair_dirs(&tests.join(r))).collect();
    assert!(dirs.len() > 1000, "found only {} pair-test directories", dirs.len());
    let mut jobs: Vec<(String, Vec<Side>)> = dirs
        .iter()
        .map(|d| {
            let label = d.strip_prefix(&tests).unwrap_or(d).display().to_string();
            (label, pair_sides(d, &support))
        })
        .collect();
    let kiln = root.join("../../kiln/tests");
    let codegen = kiln_sides(&kiln.join("codegen"), "c++26");
    assert!(codegen.len() > 500, "found only {} tests in Kiln's tests/codegen", codegen.len());
    jobs.extend(codegen);
    if std::env::var_os("SLIP_WP086_CONFORMANCE").is_some_and(|v| v == "1") {
        let mut sub: Vec<_> = std::fs::read_dir(kiln.join("conformance"))
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        sub.sort();
        for d in sub {
            jobs.extend(kiln_sides(&d, "c++26"));
        }
    }
    let scratch = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("wp086-differential");
    let next = std::sync::atomic::AtomicUsize::new(0);
    let diffs: Vec<Diff> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..test_threads())
            .map(|w| {
                let (jobs, scratch, next) = (&jobs, &scratch, &next);
                std::thread::Builder::new()
                    .stack_size(slip_digest::STACK_SIZE)
                    .spawn_scoped(scope, move || {
                        let mut d = Diff::default();
                        loop {
                            let k = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            let Some((label, sides)) = jobs.get(k) else { break };
                            for (n, side) in sides.iter().enumerate() {
                                let at = scratch.join(format!("{w}"));
                                d.add(differential(side, &at, &format!("{label} side {n}")));
                            }
                        }
                        d
                    })
                    .unwrap()
            })
            .collect();
        workers.into_iter().map(|w| w.join().unwrap()).collect()
    });
    let mut d = Diff::default();
    for x in diffs {
        d.add(x);
    }
    d.differences.sort();
    d.unmatched_at.sort();
    d.missing_at.sort();
    eprintln!(
        "item 7 against {}: {} tests, {} units ({} with a Kiln error, {} with a clang \
         error); {} calls ({} closed by a macro's `)`, {} with no clang call); extents: {} equal, \
         {} equal through a macro, {} null, {} clang's through a macro not comparable, {} different",
        version.lines().next().unwrap_or(CLANG),
        jobs.len(),
        d.units,
        d.kiln_errors,
        d.clang_errors,
        d.calls,
        d.closed_by_macro,
        d.unmatched,
        d.compared,
        d.compared_through_macro,
        d.null,
        d.clang_macro,
        d.differences.len()
    );
    for u in d.unmatched_at.iter().take(100) {
        eprintln!("no clang call: {u}");
    }
    eprintln!(
        "item 7, reverse: {} clang calls whose `)` is written, {} the subject has not, by kind:",
        d.clang_calls,
        d.missing.values().sum::<u64>()
    );
    for (kind, n) in &d.missing {
        eprintln!("  {n:>6}  {kind}");
    }
    let shown: usize =
        std::env::var("SLIP_WP086_SHOW").ok().and_then(|v| v.parse().ok()).unwrap_or(100);
    for u in d.missing_at.iter().filter(|u| !u.contains(" in an ")).take(shown) {
        eprintln!("no subject call: {u}");
    }
    assert!(d.compared > 1000, "only {} extents compared", d.compared);
    assert!(d.differences.is_empty(), "{}", d.differences.join("\n"));
}

// ---------------------------------------------------------------------------
// Measurement (`docs/wp/WP-086.md`): null extents on three LLVM units, by
// the kind of expression where the rule stopped. Not part of the gate.

/// The census of `$SLIP_CORPUS_DIR/llvm` units `SLIP_WP086_UNITS` (paths
/// under `src/`, separated by `,`; by default a small, a typical and a
/// large one). Prints the numbers.
#[test]
#[ignore = "a measurement: run by hand on the corpus"]
fn measure_llvm_null_extents() {
    use slip_model::{Command, Digest, FileRef, Language, Observer, Origin, Phase};
    let corpus =
        std::path::PathBuf::from(std::env::var("SLIP_CORPUS_DIR").expect("SLIP_CORPUS_DIR"))
            .join("llvm");
    let db = std::fs::read(corpus.join("build/compile_commands.json")).expect("the database");
    let db = slip_model::json::parse(&db).expect("the database is JSON");
    let units = std::env::var("SLIP_WP086_UNITS").unwrap_or_else(|_| {
        "llvm/lib/Support/StringRef.cpp,llvm/lib/IR/Instructions.cpp,clang/lib/Sema/SemaExpr.cpp"
            .to_owned()
    });
    for unit in units.split(',') {
        let file = corpus.join("src").join(unit);
        let entry = db
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e.get("file").and_then(|f| f.as_str()) == file.to_str())
            .unwrap_or_else(|| panic!("{unit}: not in the database"));
        let command = entry.get("command").and_then(|c| c.as_str()).expect("a command");
        assert!(!command.contains(['"', '\'', '\\']), "{unit}: a command needing a shell");
        let observer = Observer {
            id: Digest([9; 32]),
            command: Command {
                directory: entry
                    .get("directory")
                    .and_then(|d| d.as_str())
                    .unwrap()
                    .as_bytes()
                    .to_vec(),
                argv: command.split_ascii_whitespace().map(|a| a.as_bytes().to_vec()).collect(),
                env: Vec::new(),
            },
            language: Language::Cxx,
            main: FileRef { path: Vec::new(), size: 0, content: Digest([0; 32]) },
            cost_hint: 1,
            origins: vec![Origin { configuration: 0, entry: 0 }],
        };
        let mut options = slip_kiln::ParseOptions::default();
        options.written = true;
        let t0 = std::time::Instant::now();
        let (shard, run) = slip_kiln::parse_observer_with(&observer, Phase::Map, 0, options, |u| {
            let owned = |_: &[u8]| true;
            let s = KilnSubject::new(u, &owned);
            let started = std::time::Instant::now();
            let census = s.extent_census();
            let elapsed = started.elapsed();
            // `SLIP_WP086_SAMPLE=n`: where the first n calls of each cause
            // of a null extent are.
            let sample: usize =
                std::env::var("SLIP_WP086_SAMPLE").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
            let mut shown = std::collections::BTreeMap::<String, usize>::new();
            for x in s.call_sites().into_iter().map(|c| c.call) {
                if let Some(w) = s.no_extent(x) {
                    let cause = format!("{:?} {}", w.kind, w.reason);
                    let seen = shown.entry(cause.clone()).or_default();
                    if *seen < sample {
                        *seen += 1;
                        println!("  sample {cause}: {}", s.display(Value::Expression(x)));
                    }
                }
            }
            (census, elapsed)
        })
        .expect("the unit parses");
        println!("{unit}: outcome {:?}", shard.outcome);
        let (census, census_time) = run.expect("the closure ran");
        println!(
            "{unit}: {} calls, {} null; {} arguments, {} null; {} without a reference; \
             census {census_time:?}, all {:?}",
            census.calls,
            census.calls_null.values().sum::<u64>(),
            census.arguments,
            census.arguments_null.values().sum::<u64>(),
            census.without_reference,
            t0.elapsed()
        );
        for (what, map) in [("call", &census.calls_null), ("argument", &census.arguments_null)] {
            for (w, n) in map {
                println!("  {what}: {:?} {}: {n}", w.kind, w.reason);
            }
        }
    }
}
