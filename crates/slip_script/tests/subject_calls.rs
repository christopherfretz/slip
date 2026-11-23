//! `KilnSubject` for calls and the extents of expressions
//! (`docs/wp/WP-086.md`): `spec/api.md` §6.6 rules 3 and 4, §6.7, §7.4
//! rule 3, on snippets parsed with Kiln's written-reference table,
//! calling the trait directly. Each test names its section and rule, or
//! the item of the package's test list.

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
int g(S s, T t, short c) { return h(1) + h(s) + i(t) + i(c) + j(2) + j(s); }
";
    with(&[("main.cpp", src)], |_, s| {
        let a: Vec<_> = calls_of(s, one(s, "h")).into_iter().map(|x| args(s, x)).collect();
        assert_eq!(a, vec![some(&["1"]), some(&["s"])]);
        let b: Vec<_> = calls_of(s, one(s, "i")).into_iter().map(|x| args(s, x)).collect();
        assert_eq!(b, vec![some(&["t"]), some(&["c"])]);
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
";
    with(&[("main.cpp", src)], |_, s| {
        assert_eq!(calls(s, "f"), some(&["f(1)", "f(sizeof(T))", "f(2)"]));
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
        let want = [
            "f(1)", "f(2)", "f(8)", "f(3)", "f(4)", "f(5)", "f(6)", "f(7)", "f(9)", "f(10)",
        ];
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
            "1", "1.5", "'c'", "\"s\" \"t\"", "true", "nullptr", "__null", "this", "v", "N::v",
            "::v", "o.x", "p->x", "x", "S::sx", "(v)", "[] { return 1; }",
            "static_cast<long>(v)", "sizeof(int)", "sizeof v", "int(2)", "S(1)", "S{1}",
            "(long)v", "f(1)", "o.f(2)", "w[1]", "-v", "!v", "v++", "v + 1", "v += 1",
            "v ? 1 : 2", "o + t", "-o", "o++", "o(3)", "o[4]",
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
struct S { int operator+(int) const; };
int g(S s) { return k(t<int>) + t<long>(1) + k(&S::operator+) + s.operator+(1); }
";
    with(&[("main.cpp", src)], |_, s| {
        let xs = calls_of(s, one(s, "k"));
        assert_eq!(args(s, xs[0]), vec![None]);
        assert_eq!(calls(s, "t"), some(&["t<long>(1)"]));
        assert_eq!(args(s, xs[1]), vec![None]);
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
