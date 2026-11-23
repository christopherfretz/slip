//! `KilnSubject`'s keys (`docs/wp/WP-095.md`): `spec/api.md` §7.5 rules 1
//! to 3 over Kiln, on snippets parsed with Kiln's tables. The keys a
//! question gives are named by the digest's own names of its units
//! (`slip_digest::Output::names`), so each expectation reads as the
//! units the digest lists. Each test names its section and rule, or the
//! item of the package's test list.

use std::collections::{BTreeMap, BTreeSet};

use slip_kiln::snippet::{Extra, Parsed, Snippet, parse_with_tables};
use slip_model::Digest;
use slip_model::keys::RuleTable;
use slip_script::host::parse_qualified_name;
use slip_script::kiln_subject::{KilnSubject, Why};
use slip_script::subject::{Declaration, Entity, Expression, Reference, Subject, Value};
use slip_script::wire::Errc;

// ---------------------------------------------------------------------------
// Harness.

/// The digest of a snippet: every unit's key and digest, and the name of
/// each unit, from one walk on a thread of the walk's stack.
struct Digested {
    names: BTreeMap<Digest, String>,
    units: BTreeMap<Digest, Digest>,
}

fn digested(p: &Parsed<'_>) -> Digested {
    let paths: Vec<Vec<u8>> =
        p.sm.file_ids()
            .map(|f| p.path(f).map(|s| s.as_bytes().to_vec()).unwrap_or_default())
            .collect();
    let (ast, sm) = (p.ast, p.sm);
    std::thread::scope(|s| {
        std::thread::Builder::new()
            .stack_size(slip_digest::STACK_SIZE)
            .spawn_scoped(s, || {
                let path = |f: slip_kiln::kiln::source::FileId| {
                    paths.get(f.index()).cloned().unwrap_or_default()
                };
                let rules = RuleTable::empty();
                let input = slip_digest::Input { ast, sm, subject_path: &path };
                let options = slip_digest::Options {
                    rules: &rules,
                    read_only: slip_digest::ReadOnly::EVERY_FILE,
                    detail: slip_digest::Detail::All,
                    names: true,
                    rule_use: false,
                };
                let out = slip_digest::digest(&input, &options);
                let mut units = BTreeMap::new();
                for g in &out.digests.groups {
                    for u in g.units.iter().flatten() {
                        units.insert(u.key, u.digest);
                    }
                }
                Digested { names: out.names.into_iter().collect(), units }
            })
            .expect("a thread for the walk")
            .join()
            .expect("the walk")
    })
}

/// Parses `src` with the tables and calls `f` with the subject, inside
/// its `with_keys`, and the snippet's digest.
fn with<R>(src: &str, f: impl FnOnce(&KilnSubject<'_>, &Digested) -> R) -> R {
    let files = [("main.cpp", src)];
    let snippet = Snippet { files: &files, main: "main.cpp", include: &[], std: "c++23" };
    parse_with_tables(&snippet, &Extra::default(), |p| {
        assert_eq!(p.errors, 0, "{}", p.diagnostics);
        let d = digested(p);
        let owned = |_: &[u8]| true;
        let s = KilnSubject::from_snippet(p, &owned).expect("a unit with tables");
        s.with_keys(|s| {
            // Test 5 on every snippet of this file.
            assert_eq!(s.entity_disagreements(), Some(Vec::new()), "WP-083's rule (test 5)");
            // The walk and the table never give one entry two units.
            let mut r = 0;
            while s.is_live(Value::Reference(Reference(r))) {
                let k = s.reference_unit_keys(Reference(r));
                assert_ne!(
                    k,
                    Err(Why::TwoUnits),
                    "reference {r}: {:?}",
                    s.reference_holder_units(Reference(r))
                );
                r += 1;
            }
            f(s, &d)
        })
    })
    .expect("the snippet parses")
}

fn one(s: &KilnSubject<'_>, name: &str) -> Entity {
    let (global, components) = parse_qualified_name(name.as_bytes()).expect("a qualified name");
    let found = s.lookup(global, &components);
    assert_eq!(found.len(), 1, "{name}: {found:?}");
    found[0]
}

/// The keys as the digest names them, the unit's own first, the derived
/// ones after it in the order of their names.
fn named(d: &Digested, keys: &[Digest]) -> Vec<String> {
    let mut out: Vec<String> =
        keys.iter().map(|k| d.names.get(k).cloned().unwrap_or_else(|| format!("{k:?}"))).collect();
    if out.len() > 1 {
        out[1..].sort();
    }
    out
}

/// The calls of `callee`, in unit order.
fn calls(s: &KilnSubject<'_>, callee: &str) -> Vec<Expression> {
    s.calls_to(one(s, callee)).expect("a function")
}

/// The unit keys of `v`, named; `Err` with the subject's reason.
fn keys_of(s: &KilnSubject<'_>, d: &Digested, v: Value) -> Result<Vec<String>, Why> {
    let keys = match v {
        Value::Reference(r) => s.reference_unit_keys(r),
        Value::Expression(x) => s.expression_unit_keys(x),
        _ => unreachable!(),
    };
    // The trait answers the same, `unsupported` for every reason.
    assert_eq!(s.unit_keys(v), keys.clone().map_err(|_| Errc::Unsupported));
    if let Ok(k) = &keys {
        let set: BTreeSet<&Digest> = k.iter().collect();
        assert_eq!(set.len(), k.len(), "no key twice");
        assert!(k.iter().all(|k| d.units.contains_key(k)), "every key is a unit the digest lists");
    }
    keys.map(|k| named(d, &k))
}

/// Every live reference whose referent is `e`, by the text of its name.
fn refs_to(s: &KilnSubject<'_>, e: Entity) -> Vec<Reference> {
    s.references_to(e)
}

fn strs(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| (*x).to_owned()).collect()
}

// ---------------------------------------------------------------------------
// Test 1: a call, and its callee's reference, in each holder.

const T1: &str = "\
int f(int);
int g() { return f(1); }
int h() { return [] { return f(2); }(); }
int v = f(3);
struct D { int k = f(4); };
template <class T> struct X { int m() { return f(5); } };
int u = X<int>().m() + X<long>().m();
auto l = [] { return f(6); };
";

#[test]
fn s7_5_rule1_test1_the_keys_of_each_holder() {
    with(T1, |s, d| {
        let xs = calls(s, "f");
        assert_eq!(xs.len(), 6);
        let want: [&[&str]; 6] = [
            &["g()"],
            &["h()"],
            &["v"],
            &["D::k"],
            &[
                "X::m()",
                "X<int>::m()",
                "X<int>::m()::<definition>",
                "X<long>::m()",
                "X<long>::m()::<definition>",
            ],
            &["l"],
        ];
        for (i, (&x, want)) in xs.iter().zip(want).enumerate() {
            let got = keys_of(s, d, Value::Expression(x)).expect("keys");
            assert_eq!(got, strs(want), "call {i}");
            // The callee's reference is held by the same unit.
            let r = s.callee_reference(x).expect("a reference");
            assert_eq!(keys_of(s, d, Value::Reference(r)), Ok(strs(want)), "reference {i}");
            // So is the argument.
            for a in s.arguments(x).expect("arguments") {
                assert_eq!(keys_of(s, d, Value::Expression(a)), Ok(strs(want)), "argument {i}");
            }
        }
    });
}

// Test 1, the two instantiations: each is derived from the member as
// written, and neither is its unit.
#[test]
fn s7_5_rule1_test1_instantiations_are_derived_units() {
    with(T1, |s, d| {
        let x = calls(s, "f")[4];
        let keys = s.expression_unit_keys(x).expect("keys");
        let first = named(d, &keys[..1]);
        assert_eq!(first, strs(&["X::m()"]), "the unit is the member as written");
        let mut rest = named(d, &keys[1..]);
        rest.sort();
        assert_eq!(
            rest,
            strs(&[
                "X<int>::m()",
                "X<int>::m()::<definition>",
                "X<long>::m()",
                "X<long>::m()::<definition>"
            ])
        );
    });
}

// ---------------------------------------------------------------------------
// Test 2: a reference outside every body.

#[test]
fn s7_5_rule1_test2_a_parameter_type_and_a_base_specifier() {
    let src = "\
struct B {};
struct S {};
void p(S s);
void q(int, S) {}
struct C : B {};
template <class T> struct Y : B {};
struct E : Y<S> {};
S r();
S w;
";
    with(src, |s, d| {
        let held: Vec<Result<Vec<String>, Why>> = refs_to(s, one(s, "S"))
            .into_iter()
            .map(|r| keys_of(s, d, Value::Reference(r)))
            .collect();
        assert_eq!(
            held,
            vec![
                Ok(strs(&["p(S)"])),
                Ok(strs(&["q(int, S)"])),
                Ok(strs(&["E"])),
                Ok(strs(&["r()"])),
                Ok(strs(&["w"])),
            ]
        );
        let bases: Vec<Result<Vec<String>, Why>> = refs_to(s, one(s, "B"))
            .into_iter()
            .map(|r| keys_of(s, d, Value::Reference(r)))
            .collect();
        // `Y`'s unit, and its instantiation `Y<S>` derived from it.
        assert_eq!(bases, vec![Ok(strs(&["C"])), Ok(strs(&["Y", "Y<S>", "Y<S>::<definition>"]))]);
    });
}

// The holders of a lambda's references, of a local class's, of a local
// variable's type, of a default argument and of a default member
// initialiser's type.
#[test]
fn s7_5_rule1_references_in_local_declarations() {
    let src = "\
struct S { static int z; };
int f(int);
int a(int x = f(S::z)) { struct L { S n() { return {}; } }; S loc; return [](S) { return 1; }(loc); }
struct M { int k = sizeof(S); };
auto gl = [](S) { return 0; };
";
    with(src, |s, d| {
        let held: Vec<Result<Vec<String>, Why>> = refs_to(s, one(s, "S"))
            .into_iter()
            .map(|r| keys_of(s, d, Value::Reference(r)))
            .collect();
        assert_eq!(
            held,
            vec![
                Ok(strs(&["a(int)"])),
                Ok(strs(&["a(int)"])),
                Ok(strs(&["a(int)"])),
                Ok(strs(&["a(int)"])),
                Ok(strs(&["M::k"])),
                Ok(strs(&["gl"])),
            ]
        );
    });
}

// ---------------------------------------------------------------------------
// Test 3: each `unsupported`.

#[test]
fn s7_5_rule2_test3_no_holder() {
    // Kiln attributes the names of an alias, an enumerator, a template
    // head and a `static_assert` to the namespace or the class around
    // them (Kiln `spec/ast.md` §18.3): no holder. A name in the
    // decl-specifiers of two declarators is held by both.
    let src = "\
struct S {};
using A = S;
enum E { e = sizeof(S) };
template <class T = S> void t() {}
struct C { using I = S; };
S a, b;
";
    with(src, |s, d| {
        for r in refs_to(s, one(s, "S")) {
            assert_eq!(keys_of(s, d, Value::Reference(r)), Err(Why::NoHolder));
        }
    });
}

#[test]
fn s7_5_rule2_test3_content_keyed_and_indeterminate() {
    let src = "\
constexpr int f(int x) { return x; }
static_assert(f(1) == 1);
void i(decltype([] {}) x) { f(2); }
";
    with(src, |s, d| {
        let xs = calls(s, "f");
        assert_eq!(
            keys_of(s, d, Value::Expression(xs[0])),
            Err(Why::Key(slip_digest::query::NoKey::ContentKeyed))
        );
        assert!(
            matches!(
                keys_of(s, d, Value::Expression(xs[1])),
                Err(Why::Key(slip_digest::query::NoKey::Indeterminate(_)))
            ),
            "{:?}",
            keys_of(s, d, Value::Expression(xs[1]))
        );
    });
}

#[test]
fn s7_5_rule2_test3_not_a_reference_or_an_expression() {
    with(T1, |s, _| {
        let g = one(s, "g");
        assert_eq!(s.unit_keys(Value::Entity(g)), Err(Errc::Unsupported));
        let decl = s.declarations(g).expect("declarations")[0];
        assert_eq!(s.unit_keys(Value::Declaration(decl)), Err(Errc::Unsupported));
        // Not live.
        assert_eq!(s.unit_keys(Value::Expression(Expression(u32::MAX))), Err(Errc::Unsupported));
        assert_eq!(s.unit_keys(Value::Reference(Reference(u32::MAX))), Err(Errc::Unsupported));
        let _ = Declaration(0);
    });
}

#[test]
fn s7_5_rule3_test3_rename_rules() {
    let src = "\
namespace n { int f(int p) { int local = p; return local; } }
namespace { int y; }
int zi;
";
    with(src, |s, _| {
        // A unit keyed by name: the rule.
        let f = one(s, "n::f");
        let rule = s.rename_rule(f, b"g").expect("a rule");
        assert_eq!(rule.name, slip_model::keys::Name::Identifier(b"g".to_vec()));
        assert_eq!(rule.to_name, slip_model::keys::Name::Identifier(b"f".to_vec()));
        // A parameter (no unit), an explicit
        // specialisation (its key follows its template's).
        let p = s.parameters(Value::Entity(f)).expect("parameters")[0];
        assert_eq!(s.rename_rule(p, b"q"), Err(Errc::Unsupported));
        // The unnamed namespace: a key, but no identifier to rename.
        let unnamed = s.parent(one(s, "y")).expect("the unnamed namespace");
        assert!(s.entity_key(unnamed).is_some());
        assert_eq!(s.rename_rule(unnamed, b"w"), Err(Errc::Unsupported));
        assert!(s.rename_rule(one(s, "zi"), b"zj").is_ok());
        // The keys of spelled entities: a namespace member has one, a
        // parameter has none.
        assert!(s.entity_key(f).is_some());
        assert_eq!(s.entity_key(p), None);
    });
}

// ---------------------------------------------------------------------------
// Test 4: end to end. The keys declared for a call cover every unit a
// change of that call changes.

fn digest_of(src: &str) -> Digested {
    let files = [("main.cpp", src)];
    let snippet = Snippet { files: &files, main: "main.cpp", include: &[], std: "c++23" };
    parse_with_tables(&snippet, &Extra::default(), |p| {
        assert_eq!(p.errors, 0, "{}", p.diagnostics);
        digested(p)
    })
    .expect("the snippet parses")
}

#[test]
fn s7_5_test4_the_keys_that_differ_are_declared() {
    for (i, at) in ["f(1)", "f(2)", "f(3)", "f(4)", "f(5)", "f(6)"].iter().enumerate() {
        let declared: BTreeSet<Digest> = with(T1, |s, _| {
            let x = calls(s, "f")[i];
            assert_eq!(s.text(s.extent(Value::Expression(x)).unwrap().unwrap()), at.as_bytes());
            s.unit_keys(Value::Expression(x)).expect("keys").into_iter().collect()
        });
        let before = digest_of(T1);
        let changed = T1.replacen(at, &format!("f({})", 40 + i), 1);
        let after = digest_of(&changed);
        let differ: BTreeSet<Digest> = before
            .units
            .iter()
            .filter(|(k, v)| after.units.get(k) != Some(v))
            .map(|(k, _)| *k)
            .chain(after.units.keys().filter(|k| !before.units.contains_key(k)).copied())
            .collect();
        assert!(!differ.is_empty(), "{at}: the change changes a unit");
        let undeclared: Vec<String> = differ
            .difference(&declared)
            .map(|k| before.names.get(k).cloned().unwrap_or_default())
            .collect();
        assert!(undeclared.is_empty(), "{at}: changed but not declared: {undeclared:?}");
    }
}

// ---------------------------------------------------------------------------
// Item 4: `Keys` lives as long as the server; a question outside
// `with_keys` gets the same answer from a server of its own.

#[test]
fn item4_outside_with_keys_the_answer_is_the_same() {
    let files = [("main.cpp", T1)];
    let snippet = Snippet { files: &files, main: "main.cpp", include: &[], std: "c++23" };
    parse_with_tables(&snippet, &Extra::default(), |p| {
        let owned = |_: &[u8]| true;
        let s = KilnSubject::from_snippet(p, &owned).expect("a unit with tables");
        let x = calls(&s, "f")[4];
        let outside = s.unit_keys(Value::Expression(x));
        let inside = s.with_keys(|s| {
            let a = s.unit_keys(Value::Expression(x));
            // Nested: the one server.
            let b = s.with_keys(|s| s.unit_keys(Value::Expression(x)));
            assert_eq!(a, b);
            a
        });
        assert!(outside.is_ok());
        assert_eq!(outside, inside);
    })
    .expect("the snippet parses");
}

// ---------------------------------------------------------------------------
// One written name, several entries of Kiln's table: the holder is that
// of every entry.

/// As [`with`], over several files.
fn with_files<R>(files: &[(&str, &str)], f: impl FnOnce(&KilnSubject<'_>, &Digested) -> R) -> R {
    let snippet = Snippet { files, main: files[0].0, include: &[""], std: "c++23" };
    parse_with_tables(&snippet, &Extra::default(), |p| {
        assert_eq!(p.errors, 0, "{}", p.diagnostics);
        let d = digested(p);
        let owned = |_: &[u8]| true;
        let s = KilnSubject::from_snippet(p, &owned).expect("a unit with tables");
        s.with_keys(|s| {
            assert_eq!(s.entity_disagreements(), Some(Vec::new()), "WP-083's rule (test 5)");
            f(s, &d)
        })
    })
    .expect("the snippet parses")
}

#[test]
fn s7_5_rule1_a_macro_argument_used_twice_has_one_holder() {
    let src = "\
struct S { static int z; };
#define TWICE(x) ((x) + (x))
int v = TWICE(S::z);
";
    with(src, |s, d| {
        let refs = refs_to(s, one(s, "S"));
        assert_eq!(refs.len(), 1, "one written name");
        assert_eq!(keys_of(s, d, Value::Reference(refs[0])), Ok(strs(&["v"])));
    });
}

#[test]
fn s7_5_rule2_a_header_entered_in_two_holders_has_no_unit() {
    let files = [
        (
            "main.cpp",
            "int f(int);\nnamespace a {\n#include \"h.h\"\n}\nnamespace b {\n#include \"h.h\"\n}\n",
        ),
        ("h.h", "inline int g() { return f(1); }\n"),
    ];
    with_files(&files, |s, d| {
        let refs = refs_to(s, one(s, "f"));
        assert_eq!(refs.len(), 1, "one written name: {refs:?}");
        assert_eq!(keys_of(s, d, Value::Reference(refs[0])), Err(Why::TwoUnits));
        // Each call is an expression of its own, with its own holder.
        let held: Vec<Result<Vec<String>, Why>> =
            calls(s, "f").into_iter().map(|x| keys_of(s, d, Value::Expression(x))).collect();
        assert_eq!(held, vec![Ok(strs(&["a::g()"])), Ok(strs(&["b::g()"]))]);
    });
}

// The walk's holder where the table gives only a context: an
// enumerator's value, a template head's default argument.
#[test]
fn s7_5_rule1_the_walk_holds_what_the_table_does_not() {
    let src = "\
constexpr int f(int x) { return x; }
enum E { q = f(1) };
template <int N = f(2)> int t() { return N; }
";
    with(src, |s, d| {
        let held: Vec<Result<Vec<String>, Why>> = refs_to(s, one(s, "f"))
            .into_iter()
            .map(|r| keys_of(s, d, Value::Reference(r)))
            .collect();
        assert_eq!(held, vec![Ok(strs(&["q"])), Ok(strs(&["template <int> int t()"]))]);
    });
}
