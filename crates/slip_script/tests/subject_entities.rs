//! `KilnSubject` for entities, lookup and structure (`docs/wp/WP-083.md`):
//! `spec/api.md` §1 (unit order), §5.1, §5.5, §6.1 to §6.4 and §7.2 rule
//! 2, on snippets (`slip_kiln::snippet`), calling the trait directly. Each
//! test names its section and rule.
//!
//! A snippet puts each declaration a test names on its own line, and the
//! tests say which entity they mean by the place of its first
//! declaration (`file:line`).

use std::time::Instant;

use slip_kiln::kiln::ast::decl::{DeclFlags, DeclKind, TemplLinkKind};
use slip_kiln::kiln::ast::names::DeclNameView;
use slip_kiln::kiln::ast::templates::SpecKind;
use slip_kiln::kiln::ast::{Ast, DeclId};
use slip_kiln::snippet::{Extra, Parsed, Snippet, parse_with_tables};
use slip_model::hash::content_digest;
use slip_script::fake::FakeOut;
use slip_script::host::{Host, Limits, parse_qualified_name};
use slip_script::kiln_subject::KilnSubject;
use slip_script::subject::{
    Declaration, Entity, File, PlaceOf, Range, Subject, Unqualified, Value,
};
use slip_script::wire::{self, Errc};

// ---------------------------------------------------------------------------
// Harness.

/// Parses `files` (the first is the main file) and calls `f` with the
/// subject over it. A file whose path starts with `own/` is owned.
fn with<R>(files: &[(&str, &str)], f: impl FnOnce(&Parsed<'_>, &KilnSubject<'_>) -> R) -> R {
    let snippet = Snippet { files, main: files[0].0, include: &["lib"], std: "c++23" };
    parse_with_tables(&snippet, &Extra::default(), |p| {
        assert_eq!(p.errors, 0, "{}", p.diagnostics);
        let owned = |path: &[u8]| path.starts_with(b"own/");
        let s = KilnSubject::from_snippet(p, &owned).expect("a unit with tables");
        f(p, &s)
    })
    .expect("the snippet parses")
}

/// Options of a real parse with Kiln's two tables, which the subject needs.
fn written_options() -> slip_kiln::ParseOptions {
    let mut o = slip_kiln::ParseOptions::default();
    o.written = true;
    o
}

fn look(s: &KilnSubject<'_>, name: &str) -> Vec<Entity> {
    let (global, components) = parse_qualified_name(name.as_bytes()).expect("a qualified name");
    let found = s.lookup(global, &components);
    for &e in &found {
        assert!(s.is_live(Value::Entity(e)), "{name}: a live entity");
    }
    found
}

fn one(s: &KilnSubject<'_>, name: &str) -> Entity {
    let found = look(s, name);
    assert_eq!(found.len(), 1, "{name}: {found:?}");
    found[0]
}

/// `file:line` of a declaration.
fn place_of(p: &Parsed<'_>, d: Declaration) -> String {
    let loc = p.ast.decls.get(DeclId::new(d.0)).loc();
    let lc = p.sm.line_col(loc).expect("a location");
    format!("{}:{}", p.path(lc.file).unwrap_or("?"), lc.line)
}

/// `file:line` of each declaration of `e`.
fn decls_at(p: &Parsed<'_>, s: &KilnSubject<'_>, e: Entity) -> Vec<String> {
    s.declarations(e).expect("declarations").into_iter().map(|d| place_of(p, d)).collect()
}

/// `file:line` of the first declaration of each entity.
fn at(p: &Parsed<'_>, s: &KilnSubject<'_>, es: &[Entity]) -> Vec<String> {
    es.iter()
        .map(|&e| decls_at(p, s, e).into_iter().next().unwrap_or_else(|| "-".to_owned()))
        .collect()
}

fn found_at(p: &Parsed<'_>, s: &KilnSubject<'_>, name: &str) -> Vec<String> {
    at(p, s, &look(s, name))
}

fn kinds(s: &KilnSubject<'_>, e: Entity) -> u64 {
    s.kind_bits(Value::Entity(e))
}

fn is(s: &KilnSubject<'_>, e: Entity, kind: u64) -> bool {
    kinds(s, e) & kind != 0
}

fn ident(s: &KilnSubject<'_>, v: Value) -> String {
    String::from_utf8(s.identifier(v).expect("an identifier")).unwrap()
}

fn name_of(ast: &Ast, d: DeclId) -> String {
    match ast.names.view(ast.decls.get(d).name()) {
        DeclNameView::Identifier(s) => ast.symbols().as_str(s).to_owned(),
        _ => String::new(),
    }
}

/// The declarations of the AST of kind `kind` named `name` that `pred`
/// accepts, as the subject numbers their entities. For entities `lookup`
/// does not find (specialisations, implicit members).
fn find(
    p: &Parsed<'_>,
    kind: DeclKind,
    name: &str,
    pred: impl Fn(&Ast, DeclId) -> bool,
) -> Vec<DeclId> {
    p.ast
        .decls
        .ids()
        .filter(|&d| {
            p.ast.decls.get(d).kind() == kind && name_of(p.ast, d) == name && pred(p.ast, d)
        })
        .collect()
}

fn spec_kind(ast: &Ast, d: DeclId) -> Option<SpecKind> {
    ast.spec_info(d).map(|(_, i)| i.kind)
}

// ---------------------------------------------------------------------------
// §6.1: lookup.

const L: &str = "namespace ns {
  void f(int);
  inline namespace v1 { int g; }
  namespace { int h; }
  enum E { A, B };
  enum class SE { K };
  struct S {
    union { int u1; float u2; };
    friend void fr(S);
    void m();
    using T = int;
  };
  template <class T> struct C { void cm(); };
  template <> struct C<int> { void spec(); };
  void f(char);
}
namespace ns { void f(double); }
void ns::f(int) {}
namespace other { using ns::f; using namespace ns; }
namespace al = ns;
using ST = ns::S;
struct D : ns::S {};
void body() { struct L {}; int x; }
extern \"C\" { void cfun(); }
namespace ns { namespace inner { int deep; } }
";

/// §6.1 rule 3: each transparent scope, an inline namespace, an unnamed
/// namespace, an unscoped enumeration and an anonymous union, and the
/// transparent context of a linkage specification.
#[test]
fn s6_1_rule3_transparent_scopes() {
    with(&[("main.cpp", L)], |p, s| {
        assert_eq!(found_at(p, s, "ns::g"), ["main.cpp:3"], "inline namespace");
        assert_eq!(found_at(p, s, "ns::v1::g"), ["main.cpp:3"], "inline namespace, named");
        assert_eq!(found_at(p, s, "ns::v1"), ["main.cpp:3"], "an inline namespace is found");
        assert_eq!(found_at(p, s, "ns::h"), ["main.cpp:4"], "unnamed namespace");
        assert_eq!(found_at(p, s, "ns::A"), ["main.cpp:5"], "unscoped enumeration");
        assert_eq!(found_at(p, s, "ns::E::A"), ["main.cpp:5"], "an enumeration is a scope");
        assert_eq!(found_at(p, s, "ns::S::u1"), ["main.cpp:8"], "anonymous union");
        assert_eq!(found_at(p, s, "ns::S::u2"), ["main.cpp:8"], "anonymous union");
        assert_eq!(found_at(p, s, "cfun"), ["main.cpp:24"], "extern \"C\" {{ }}");
    });
}

/// §6.1 rules 3 and 4: what is not transparent: a scoped enumeration, a
/// named namespace, a class's anonymous union seen from the namespace.
#[test]
fn s6_1_rule3_not_transparent() {
    with(&[("main.cpp", L)], |p, s| {
        assert!(look(s, "ns::K").is_empty(), "a scoped enumeration");
        assert_eq!(found_at(p, s, "ns::SE::K"), ["main.cpp:6"]);
        assert!(look(s, "ns::deep").is_empty(), "a named namespace");
        assert_eq!(found_at(p, s, "ns::inner::deep"), ["main.cpp:25"]);
        assert!(look(s, "ns::u1").is_empty(), "the anonymous union is in the class");
        assert!(look(s, "g").is_empty(), "transparency is one scope up, not to the root");
    });
}

/// §6.1 rule 5, not followed: using-declarations, using-directives,
/// namespace aliases, type aliases as scopes, base classes.
#[test]
fn s6_1_rule5_not_followed() {
    with(&[("main.cpp", L)], |p, s| {
        let fs = look(s, "ns::f");
        // The using-declaration is an entity declared `f` in `other`; its
        // target is not what is found.
        let u = look(s, "other::f");
        assert_eq!(at(p, s, &u), ["main.cpp:19"]);
        assert!(u.iter().all(|e| !fs.contains(e)), "the using-declaration is not followed");
        assert_eq!(kinds(s, u[0]), wire::KIND_ENTITY, "a using-declaration has no kind of §6.2");
        assert!(look(s, "other::g").is_empty(), "using-directive");
        assert!(look(s, "other::S").is_empty(), "using-directive");
        assert_eq!(found_at(p, s, "al"), ["main.cpp:20"], "an alias is found");
        assert!(look(s, "al::f").is_empty(), "namespace alias");
        let st = one(s, "ST");
        assert!(is(s, st, wire::KIND_TYPE));
        assert!(look(s, "ST::m").is_empty(), "type alias as a scope");
        assert!(look(s, "D::m").is_empty(), "base class");
        assert!(look(s, "ns::S::T::x").is_empty(), "member type alias as a scope");
        assert_eq!(found_at(p, s, "ns::S::T"), ["main.cpp:11"]);
    });
}

/// §6.1 rule 5, not found: specialisations, template parameters, entities
/// declared in a function body, entities with no written declaration (the
/// injected-class-name). Rule 4: a class template's scope is its primary
/// template's definition.
#[test]
fn s6_1_rule5_not_found() {
    with(&[("main.cpp", L)], |p, s| {
        let c = look(s, "ns::C");
        assert_eq!(at(p, s, &c), ["main.cpp:13"], "the explicit specialisation is not found");
        assert!(is(s, c[0], wire::KIND_TEMPLATE));
        assert_eq!(found_at(p, s, "ns::C::cm"), ["main.cpp:13"], "rule 4");
        assert!(look(s, "ns::C::spec").is_empty(), "a member of the specialisation");
        assert!(look(s, "ns::C::T").is_empty(), "a template parameter");
        assert!(look(s, "body::L").is_empty(), "in a function body");
        assert!(look(s, "body::x").is_empty(), "in a function body");
        assert!(look(s, "L").is_empty(), "in a function body");
        assert!(look(s, "x").is_empty(), "in a function body");
        assert!(look(s, "ns::S::S").is_empty(), "the injected-class-name is implicit");
    });
}

/// §6.1 rule 6 and the head: each function of an overload set is one
/// entity, in unit order of first declarations; a redeclaration in
/// another body and an out-of-line definition add none.
#[test]
fn s6_1_rule6_overloads_in_unit_order() {
    with(&[("main.cpp", L)], |p, s| {
        let fs = look(s, "ns::f");
        assert_eq!(at(p, s, &fs), ["main.cpp:2", "main.cpp:15", "main.cpp:17"]);
        assert_eq!(decls_at(p, s, fs[0]), ["main.cpp:2", "main.cpp:18"]);
        for &f in &fs {
            assert!(is(s, f, wire::KIND_FUNCTION));
        }
    });
}

/// §6.1 rule 8 (WP-132): `lookup(scope, identifier)` answers as the last
/// component of a qualified name that reached `scope`: transparency,
/// overloads in unit order, a class, an enumeration, a class template, an
/// unnamed namespace, a typedef name for linkage purposes; `None` for an
/// entity that is no scope; the empty list for an unknown identifier.
#[test]
fn s6_1_rule8_lookup_in_a_scope() {
    let src = "namespace e { typedef struct { int tdm; } TD; typedef int TI; }\n";
    with(&[("main.cpp", L)], |p, s| {
        let in_ = |scope: Entity, id: &str| {
            let found = s.lookup_in(scope, id.as_bytes()).expect("a scope");
            at(p, s, &found)
        };
        let ns = one(s, "ns");
        assert_eq!(in_(ns, "f"), ["main.cpp:2", "main.cpp:15", "main.cpp:17"]);
        assert_eq!(in_(ns, "g"), ["main.cpp:3"], "through the inline namespace");
        assert_eq!(in_(ns, "h"), ["main.cpp:4"], "through the unnamed namespace");
        assert!(in_(ns, "deep").is_empty(), "a named namespace is not transparent");
        assert!(in_(ns, "no_such_name").is_empty());
        let unnamed = s.parent(one(s, "ns::h")).expect("a parent");
        assert_eq!(in_(unnamed, "h"), ["main.cpp:4"], "a scope with no qualified name");
        assert_eq!(in_(one(s, "ns::S"), "u1"), ["main.cpp:8"]);
        assert_eq!(in_(one(s, "ns::E"), "A"), ["main.cpp:5"]);
        assert_eq!(in_(one(s, "ns::SE"), "K"), ["main.cpp:6"]);
        assert_eq!(in_(one(s, "ns::C"), "cm"), ["main.cpp:13"]);
        assert!(in_(one(s, "ns::C"), "spec").is_empty(), "a member of the specialisation");
        // An explicit specialisation: its own members, not the template's.
        let explicit = |ast: &Ast, d| spec_kind(ast, d) == Some(SpecKind::ExplicitSpecialization);
        let mut spec = find(p, DeclKind::ClassTemplateSpecialization, "C", explicit);
        spec.extend(find(p, DeclKind::CXXRecord, "C", explicit));
        assert!(!spec.is_empty(), "the explicit specialisation C<int>");
        let spec = s.entity_number(spec[0]);
        assert_eq!(in_(spec, "spec"), ["main.cpp:14"]);
        assert!(in_(spec, "cm").is_empty());
        let global = s.parent(ns).expect("the global namespace");
        assert_eq!(in_(global, "ns"), ["main.cpp:1"]);
        for not_a_scope in [one(s, "ns::v1::g"), look(s, "ns::f")[0], one(s, "ST")] {
            assert_eq!(s.lookup_in(not_a_scope, b"m"), None, "{not_a_scope:?}");
        }
    });
    with(&[("main.cpp", src)], |p, s| {
        let td = s.lookup_in(one(s, "e::TD"), b"tdm").expect("a typedef for linkage");
        assert_eq!(at(p, s, &td), ["main.cpp:1"]);
        assert_eq!(s.lookup_in(one(s, "e::TI"), b"tdm"), None, "another typedef");
    });
}

/// §6.1 rule 8 (WP-132 ruling on item 4): a specialisation as a scope has
/// the members written in the definition it comes from, and the entities
/// found are the written ones. An instantiation of the primary template,
/// of a partial specialisation (the partial's members, not the
/// primary's), a member class of an instantiated class template, and an
/// explicit instantiation definition, each reached as `parent_of` the
/// referent of a call in `use`; a partial specialisation as a scope: its
/// own members. A specialisation named and never instantiated, and a
/// class declared and not defined: nothing.
#[test]
fn s6_1_rule8_lookup_in_an_instantiation() {
    let src = "template <class T> struct W {
  void f();
  void g();
};
template <class T> struct W<T*> {
  void f();
  void g();
};
template <class T> struct O {
  struct I {
    void f();
    void g();
  };
};
template <class T> struct X {
  void f();
  void g();
};
template struct X<char>;
template <class T> struct N {
  void g();
};
struct D;
void use(W<int> w, W<int*> p, O<int>::I i, X<char> x, N<int>* n, D* d) {
  w.f(); p.f(); i.f(); x.f();
}
";
    with(&[("main.cpp", src)], |p, s| {
        let in_ = |scope: Entity, id: &str| s.lookup_in(scope, id.as_bytes()).expect("a scope");
        // The scope of the one call of `f` (an origin) in `use`.
        let scope_of_call = |f: Entity| {
            let refs = s.references_to(f);
            assert_eq!(refs.len(), 1, "{f:?}: {refs:?}");
            s.parent(s.referent(refs[0])).expect("a parent")
        };
        let partial =
            find(p, DeclKind::CXXRecord, "W", |ast, d| ast.partial_spec_info(d).is_some());
        let partial = s.entity_number(partial[0]);
        for (template, f, line) in [
            (one(s, "W"), one(s, "W::f"), "main.cpp:3"),
            (partial, in_(partial, "f")[0], "main.cpp:7"),
            (one(s, "O::I"), one(s, "O::I::f"), "main.cpp:12"),
            (one(s, "X"), one(s, "X::f"), "main.cpp:17"),
        ] {
            let inst = scope_of_call(f);
            assert_ne!(inst, template, "{line}: the scope is the instantiation");
            let g = in_(inst, "g");
            assert_eq!(at(p, s, &g), [line]);
            assert_eq!(g, in_(template, "g"), "{line}: the written member");
            assert_eq!(in_(inst, "f"), in_(template, "f"), "{line}");
            assert!(in_(inst, "no_such_name").is_empty());
        }
        // The partial's members are its own, not the primary's.
        assert_ne!(in_(partial, "g"), in_(one(s, "W"), "g"));
        // N<int> is named (a pointer) and never instantiated; D is declared.
        let named = find(p, DeclKind::ClassTemplateSpecialization, "N", |_, _| true)
            .into_iter()
            .chain(find(p, DeclKind::CXXRecord, "N", |ast, d| ast.spec_info(d).is_some()))
            .collect::<Vec<_>>();
        assert!(!named.is_empty(), "Kiln makes the specialisation N<int>");
        assert!(in_(s.entity_number(named[0]), "g").is_empty(), "never instantiated");
        assert!(in_(one(s, "D"), "g").is_empty(), "declared, not defined");
        assert_eq!(at(p, s, &in_(one(s, "N"), "g")), ["main.cpp:21"]);
    });
}

/// §6.1 head, §1: a name declared in two headers and in the main file is
/// in the order of inclusion (a header's declarations before and after a
/// nested include), not in the order of the files' buffers.
#[test]
fn s6_1_s1_order_follows_inclusion() {
    let main = "namespace n { void h(long); }
#include \"b.h\"
#include \"a.h\"
namespace n { void h(double); }
";
    let a = "namespace n { void h(int); }
#include \"c.h\"
namespace n { void h(unsigned); }
";
    let b = "namespace n { void h(char); }\n";
    let c = "namespace n { void h(short); }\n";
    with(&[("main.cpp", main), ("a.h", a), ("b.h", b), ("c.h", c)], |p, s| {
        assert_eq!(
            found_at(p, s, "n::h"),
            ["main.cpp:1", "b.h:1", "a.h:1", "c.h:1", "a.h:3", "main.cpp:4"]
        );
        // §6.4 rule 3: the bodies of `n` in the same order.
        assert_eq!(
            decls_at(p, s, one(s, "n")),
            ["main.cpp:1", "b.h:1", "a.h:1", "c.h:1", "a.h:3", "main.cpp:4"]
        );
    });
}

/// §6.1 rule 2: `::` names the global namespace; a leading `::` changes
/// nothing (the walk starts there anyway).
#[test]
fn s6_1_rule2_global() {
    with(&[("main.cpp", L)], |_, s| {
        let g = one(s, "::");
        assert!(is(s, g, wire::KIND_NAMESPACE));
        assert_eq!(s.parent(g), None, "§6.4 rule 1");
        assert_eq!(ident(s, Value::Entity(g)), "");
        assert_eq!(s.declarations(g), Ok(vec![]), "no written declaration");
        assert_eq!(look(s, "::ns"), look(s, "ns"));
        assert_eq!(look(s, "::ns::f"), look(s, "ns::f"));
        assert_eq!(s.parent(one(s, "ns")), Some(g));
    });
}

/// §6.1 rule 7: a name nothing matches is empty, whether or not the unit
/// spells it somewhere.
#[test]
fn s6_1_rule7_absent() {
    with(&[("main.cpp", L)], |_, s| {
        assert!(look(s, "ns::nope").is_empty());
        assert!(look(s, "never_written_anywhere").is_empty());
        assert!(look(s, "f").is_empty(), "spelled in the unit, not declared globally");
        assert!(look(s, "ns::f::x").is_empty(), "a function is no scope");
        assert!(look(s, "ns::v1::v1").is_empty());
    });
}

/// §6.1 rule 3 with [namespace.memdef]/3: a friend declared in a class is
/// a member of the innermost enclosing namespace, not of the class.
#[test]
fn s6_1_friend_is_a_namespace_member() {
    with(&[("main.cpp", L)], |p, s| {
        assert_eq!(found_at(p, s, "ns::fr"), ["main.cpp:9"]);
        assert!(look(s, "ns::S::fr").is_empty());
    });
}

/// §6.1, §5.3 rule 2: through the host, the answer is checked (live, no
/// duplicate) and encoded; a repeated call gives the same handles.
#[test]
fn s6_1_through_the_host() {
    with(&[("main.cpp", L)], |_, s| {
        let mut h = Host::new(s, content_digest(b"t"), Limits::default());
        let mut call = |name: &str| {
            let mut out = FakeOut::with_capacity(64, 0);
            let word = h.call(wire::OP_LOOKUP, &[], name.as_bytes(), &mut out).unwrap();
            assert_eq!(wire::status(word), 0, "{name}");
            out.ints
        };
        let first = call("ns::f");
        assert_eq!(first.len(), 3);
        assert_eq!(call("ns::f"), first);
        assert!(call("ns::absent").is_empty());
    });
}

// ---------------------------------------------------------------------------
// §6.2: sorts and kinds.

const K: &str = "namespace kn {
struct Cls { int dm; static int sdm; void meth(); };
enum En { E1 };
typedef int Td;
using Al = int;
void fn(int p, ...);
template <class T> void ft(T);
template <class T> struct CT {};
int var;
template <class T> concept Co = true;
template <class T> using AT = T;
template <class T> T vt = T();
struct Fwd;
struct Fwd {};
extern int ev;
int ev = 1;
int Cls::sdm = 2;
void fn(int p, ...) {}
}
";

/// §6.2 rule 3: each kind predicate true and false, by kind bit.
#[test]
fn s6_2_rule3_kinds() {
    with(&[("main.cpp", K)], |_, s| {
        let kn = one(s, "kn");
        let fn_ = one(s, "kn::fn");
        let p = s.parameters(Value::Entity(fn_)).unwrap();
        assert_eq!(p.len(), 1, "§6.4 rule 2: an ellipsis is not a parameter");
        let rows: &[(Entity, u64, &str)] = &[
            (one(s, "::"), wire::KIND_NAMESPACE, "global"),
            (kn, wire::KIND_NAMESPACE, "namespace"),
            (one(s, "kn::Cls"), wire::KIND_TYPE, "class"),
            (one(s, "kn::En"), wire::KIND_TYPE, "enumeration"),
            (one(s, "kn::Td"), wire::KIND_TYPE, "typedef"),
            (one(s, "kn::Al"), wire::KIND_TYPE, "alias-declaration"),
            (fn_, wire::KIND_FUNCTION, "function"),
            (one(s, "kn::Cls::meth"), wire::KIND_FUNCTION, "member function"),
            (one(s, "kn::ft"), wire::KIND_TEMPLATE, "function template"),
            (one(s, "kn::CT"), wire::KIND_TEMPLATE, "class template"),
            (one(s, "kn::AT"), wire::KIND_TEMPLATE, "alias template"),
            (one(s, "kn::vt"), wire::KIND_TEMPLATE, "variable template"),
            (one(s, "kn::var"), wire::KIND_VARIABLE, "variable"),
            (one(s, "kn::Cls::dm"), wire::KIND_VARIABLE, "data member"),
            (one(s, "kn::Cls::sdm"), wire::KIND_VARIABLE, "static data member"),
            (p[0], wire::KIND_PARAMETER, "parameter"),
        ];
        for &(e, kind, what) in rows {
            assert_eq!(
                kinds(s, e),
                wire::KIND_ENTITY | kind,
                "{what}: exactly its kind (a function template is not a function, \
                 a parameter not a variable, a class template not a type)"
            );
        }
    });
}

/// §6.2 rule 3: an entity that satisfies none: an enumerator, a concept.
#[test]
fn s6_2_rule3_none() {
    with(&[("main.cpp", K)], |_, s| {
        assert_eq!(kinds(s, one(s, "kn::E1")), wire::KIND_ENTITY, "enumerator");
        assert_eq!(kinds(s, one(s, "kn::En::E1")), wire::KIND_ENTITY, "enumerator");
        assert_eq!(kinds(s, one(s, "kn::Co")), wire::KIND_ENTITY, "concept");
    });
}

/// §6.2 rules 2 and 4: a declaration has the declaration sort only, and
/// the definition bit exactly when it is a definition ([basic.def]).
#[test]
fn s6_2_rule4_definition() {
    with(&[("main.cpp", K)], |p, s| {
        let defs = |name: &str| -> Vec<(String, bool)> {
            s.declarations(one(s, name))
                .unwrap()
                .into_iter()
                .map(|d| {
                    let bits = s.kind_bits(Value::Declaration(d));
                    assert_eq!(bits & !wire::KIND_DEFINITION, wire::KIND_DECLARATION, "{name}");
                    (place_of(p, d), bits & wire::KIND_DEFINITION != 0)
                })
                .collect()
        };
        let y = |l: u32| (format!("main.cpp:{l}"), true);
        let n = |l: u32| (format!("main.cpp:{l}"), false);
        assert_eq!(defs("kn::Fwd"), [n(13), y(14)], "a class");
        assert_eq!(defs("kn::ev"), [n(15), y(16)], "extern and not");
        assert_eq!(defs("kn::Cls::sdm"), [n(2), y(17)], "a static data member");
        assert_eq!(defs("kn::fn"), [n(6), y(18)], "a function");
        assert_eq!(defs("kn::var"), [y(9)], "a variable");
        assert_eq!(defs("kn::Td"), [n(4)], "a typedef");
        assert_eq!(defs("kn::Al"), [n(5)], "an alias-declaration");
        assert_eq!(defs("kn::ft"), [n(7)], "a function template without a body");
        assert_eq!(defs("kn::CT"), [y(8)], "a class template");
        assert_eq!(defs("kn::Co"), [y(10)], "a concept");
        assert_eq!(defs("kn::Cls::dm"), [y(2)], "a data member");
        assert_eq!(defs("kn::E1"), [y(3)], "an enumerator");
        assert_eq!(defs("kn")[0], y(1), "a namespace body");
    });
}

/// §6.2 rules 1 and 2: every value the subject gives has its one sort.
#[test]
fn s6_2_rule2_one_sort() {
    with(&[("main.cpp", K)], |_, s| {
        let e = one(s, "kn::fn");
        assert_eq!(kinds(s, e) & 0xf, wire::KIND_ENTITY);
        for d in s.declarations(e).unwrap() {
            assert_eq!(s.kind_bits(Value::Declaration(d)) & 0xf, wire::KIND_DECLARATION);
            assert!(s.is_live(Value::Declaration(d)));
        }
    });
}

// ---------------------------------------------------------------------------
// §6.3, §6.4: names and structure.

const S: &str = "namespace sn {
void rd(int a);
void rd(int b);
void rd(int c) {}
struct P { P(); P& operator=(const P&); int x; friend void fr2(); };
void fr2();
template <class T> void tp(T t, int u);
enum class Sc { V };
namespace { int anon; }
struct I { int z; };
}
namespace sn { int more; }
namespace sn { }
void use() { sn::I i; sn::I j = i; (void)j; }
";

/// §6.4 rule 3, §5.1: redeclarations are one entity with several
/// declarations, in unit order; `entity_of` each is the entity.
#[test]
fn s6_4_rule3_redeclarations() {
    with(&[("main.cpp", S)], |p, s| {
        let rd = one(s, "sn::rd");
        assert_eq!(decls_at(p, s, rd), ["main.cpp:2", "main.cpp:3", "main.cpp:4"]);
        for d in s.declarations(rd).unwrap() {
            assert_eq!(s.entity_of(d), rd, "§6.4 rule 4");
            assert_eq!(ident(s, Value::Declaration(d)), "rd", "§6.3 rule 1");
            assert_ne!(Value::Declaration(d), Value::Entity(rd), "§5.1 rule 2");
        }
    });
}

/// §6.4 rule 3: a namespace's declarations are its bodies.
#[test]
fn s6_4_rule3_namespace_bodies() {
    with(&[("main.cpp", S)], |p, s| {
        let sn = one(s, "sn");
        assert_eq!(decls_at(p, s, sn), ["main.cpp:1", "main.cpp:12", "main.cpp:13"]);
        for d in s.declarations(sn).unwrap() {
            assert_eq!(s.entity_of(d), sn);
        }
        assert_eq!(found_at(p, s, "sn::more"), ["main.cpp:12"]);
    });
}

/// §6.4 rules 1 and 2: parameters, in order, of an entity (its first
/// declaration's) and of one declaration; a parameter's parent is its
/// function (for a function template, the template); `declarations_of` a
/// parameter is `unsupported`; a non-function has none.
#[test]
fn s6_4_rule2_parameters() {
    with(&[("main.cpp", S)], |_, s| {
        let rd = one(s, "sn::rd");
        let ps = s.parameters(Value::Entity(rd)).unwrap();
        let names = |ps: &[Entity]| -> Vec<String> {
            ps.iter().map(|&x| ident(s, Value::Entity(x))).collect()
        };
        assert_eq!(names(&ps), ["a"], "the first declaration's");
        let ds = s.declarations(rd).unwrap();
        let last = s.parameters(Value::Declaration(ds[2])).unwrap();
        assert_eq!(names(&last), ["c"], "the declaration's own");
        assert_ne!(ps, last, "each declaration's parameters are its own entities");
        for &x in ps.iter().chain(&last) {
            assert_eq!(s.parent(x), Some(rd), "rule 1");
            assert!(is(s, x, wire::KIND_PARAMETER));
            assert_eq!(s.declarations(x), Err(Errc::Unsupported), "rule 3");
        }
        let tp = one(s, "sn::tp");
        let tps = s.parameters(Value::Entity(tp)).unwrap();
        assert_eq!(names(&tps), ["t", "u"], "a function template");
        assert_eq!(s.parent(tps[0]), Some(tp));
        let td = s.declarations(tp).unwrap();
        assert_eq!(s.parameters(Value::Declaration(td[0])).unwrap(), tps);
        assert_eq!(s.parameters(Value::Entity(one(s, "sn::Sc"))), None, "not a function");
        assert_eq!(s.parameters(Value::Entity(one(s, "sn"))), None, "not a function");
    });
}

/// §6.4 rule 1: parents.
#[test]
fn s6_4_rule1_parents() {
    with(&[("main.cpp", S), ("other.h", "")], |_, s| {
        let sn = one(s, "sn");
        assert_eq!(s.parent(one(s, "sn::Sc::V")), Some(one(s, "sn::Sc")), "an enumerator");
        assert_eq!(s.parent(one(s, "sn::P::x")), Some(one(s, "sn::P")), "a member");
        let anon = s.parent(one(s, "sn::anon")).unwrap();
        assert!(is(s, anon, wire::KIND_NAMESPACE), "an unnamed namespace is a parent");
        assert_eq!(ident(s, Value::Entity(anon)), "", "§6.3 rule 1: unnamed");
        assert_eq!(s.parent(anon), Some(sn));
        assert_eq!(s.parent(sn), Some(one(s, "::")));
    });
    with(&[("main.cpp", L)], |_, s| {
        let v1 = one(s, "ns::v1");
        assert_eq!(s.parent(one(s, "ns::g")), Some(v1), "an inline namespace is a parent");
        let c = one(s, "ns::C");
        assert_eq!(s.parent(one(s, "ns::C::cm")), Some(c), "a member of a class template");
        let u = s.parent(one(s, "ns::S::u1")).unwrap();
        assert!(is(s, u, wire::KIND_TYPE), "the anonymous union");
        assert_eq!(s.parent(u), Some(one(s, "ns::S")));
    });
}

/// §6.4 rule 1 (landed decision 11): a template parameter's parent is its
/// template, not the scope Kiln gives it as context.
#[test]
fn s6_4_rule1_template_parameter_parent() {
    with(&[("main.cpp", O)], |p, s| {
        let tf = one(s, "tf");
        let c = one(s, "C");
        let ct = one(s, "C::ct");
        let parent = |name: &str| -> Vec<Option<Entity>> {
            find(p, DeclKind::TemplateTypeParm, name, |ast, d| {
                !ast.decls.get(d).has(DeclFlags::IMPLICIT)
            })
            .into_iter()
            .map(|d| s.entity_number(d))
            .filter(|&e| s.is_live(Value::Entity(e)))
            .map(|e| s.parent(e))
            .collect()
        };
        let t = parent("T");
        assert!(t.contains(&Some(tf)) && t.contains(&Some(c)), "{t:?}");
        assert!(parent("U").contains(&Some(ct)));
    });
}

/// §6.4 rules 1 and 3 with a friend declaration: one entity declared in
/// the class and in the namespace; its parent is the namespace.
#[test]
fn s6_4_friend_declaration() {
    with(&[("main.cpp", S)], |p, s| {
        let fr = one(s, "sn::fr2");
        assert_eq!(decls_at(p, s, fr), ["main.cpp:5", "main.cpp:6"]);
        assert_eq!(s.parent(fr), Some(one(s, "sn")));
    });
}

/// §6.4 rule 3: an implicit member has no declaration; it is an entity
/// all the same, with no place (§7.4 rule 3) and no spelling (§7.1 rule 4).
#[test]
fn s6_4_rule3_implicit_member() {
    with(&[("main.cpp", S)], |p, s| {
        let implicit = find(p, DeclKind::CXXConstructor, "", |ast, d| {
            ast.decls.get(d).has(DeclFlags::IMPLICIT)
                && ast.decls.get(d).ctx().is_some_and(|c| name_of(ast, c) == "I")
        });
        assert!(!implicit.is_empty(), "the snippet makes I's implicit constructors");
        for d in implicit {
            let e = s.entity_number(d);
            assert!(s.is_live(Value::Entity(e)));
            assert_eq!(s.declarations(e), Ok(vec![]));
            assert!(!s.is_live(Value::Declaration(Declaration(d.index()))));
            assert_eq!(s.place(Value::Entity(e)), PlaceOf::Nowhere);
            assert_eq!(s.qualified(e), None);
            assert_eq!(s.unqualified(e), None, "§7.1 rule 4: never spelled");
            assert!(is(s, e, wire::KIND_FUNCTION));
            assert_eq!(s.parent(e), Some(one(s, "sn::I")));
        }
    });
}

/// §6.3 rule 1: identifiers, and the empty identifier of a name that is
/// not one (a constructor, an operator, an unnamed entity).
#[test]
fn s6_3_rule1_identifiers() {
    with(&[("main.cpp", S)], |p, s| {
        assert_eq!(ident(s, Value::Entity(one(s, "sn::rd"))), "rd");
        assert_eq!(ident(s, Value::Entity(one(s, "::"))), "");
        let ctor = find(p, DeclKind::CXXConstructor, "", |ast, d| {
            !ast.decls.get(d).has(DeclFlags::IMPLICIT)
                && ast.decls.get(d).ctx().is_some_and(|c| name_of(ast, c) == "P")
        });
        assert_eq!(ctor.len(), 1);
        let ctor = s.entity_number(ctor[0]);
        assert_eq!(ident(s, Value::Entity(ctor)), "", "a constructor");
        let op = find(p, DeclKind::CXXMethod, "", |ast, d| {
            ast.decls.get(d).ctx().is_some_and(|c| name_of(ast, c) == "P")
                && !ast.decls.get(d).has(DeclFlags::IMPLICIT)
        });
        assert_eq!(op.len(), 1, "operator=");
        assert_eq!(ident(s, Value::Entity(s.entity_number(op[0]))), "", "an operator");
    });
}

/// §6.3 rule 2: a description for every live value, naming the entity.
#[test]
fn s6_3_rule2_display() {
    with(&[("main.cpp", S)], |_, s| {
        let rd = one(s, "sn::rd");
        assert!(s.display(Value::Entity(rd)).contains("rd"));
        let d = s.declarations(rd).unwrap()[0];
        assert!(s.display(Value::Declaration(d)).contains("rd"));
        assert!(s.display(Value::Declaration(d)).contains("main.cpp:2"));
        assert!(!s.display(Value::Entity(one(s, "::"))).is_empty());
        let anon = s.parent(one(s, "sn::anon")).unwrap();
        assert!(!s.display(Value::Entity(anon)).is_empty());
    });
}

// ---------------------------------------------------------------------------
// §5.5: origins.

const O: &str = "template <class T> void tf(T) {}
template <class T> struct C {
  void cm() {}
  template <class U> void ct(U) {}
  int x;
};
template <> void tf<char>(char) {}
void use() { tf(1); tf('c'); C<int> c; c.cm(); c.ct(2.0); (void)c.x; }
";

/// §5.5: the origin of a function template specialisation is the template.
#[test]
fn s5_5_origin_of_function_template_specialisation() {
    with(&[("main.cpp", O)], |p, s| {
        let tf = one(s, "tf");
        assert_eq!(s.origin(tf), tf, "a written entity is its own origin");
        let ps = find(p, DeclKind::Function, "tf", |ast, d| {
            spec_kind(ast, d) == Some(SpecKind::ImplicitInstantiation)
        });
        assert_eq!(ps.len(), 1, "tf<int>");
        let e = s.entity_number(ps[0]);
        assert!(s.is_live(Value::Entity(e)));
        assert_eq!(s.declarations(e), Ok(vec![]), "no written declaration");
        assert_eq!(s.origin(e), tf);
        assert!(is(s, e, wire::KIND_FUNCTION), "a specialisation is a function");
        assert_eq!(s.qualified(e), None, "§7.1 rule 4: a specialisation");
        assert_eq!(s.unqualified(e), Some(Unqualified::Specialisation));
    });
}

/// §5.5: an explicit specialisation is written, with its declaration (and
/// not the one Kiln made of it before reading it). Its origin is its
/// template (`slip_kiln::written::origin`, WP-085 fix round, ruling 5).
#[test]
fn s5_5_explicit_specialisation_is_written() {
    with(&[("main.cpp", O)], |p, s| {
        let ps = find(p, DeclKind::Function, "tf", |ast, d| {
            spec_kind(ast, d) == Some(SpecKind::ExplicitSpecialization)
        });
        assert!(!ps.is_empty());
        let e = s.entity_number(ps[0]);
        assert_eq!(decls_at(p, s, e), ["main.cpp:7"]);
        assert_eq!(s.origin(e), one(s, "tf"));
        assert!(look(s, "tf").iter().all(|&x| x != e), "§6.1 rule 5");
    });
}

/// §5.5: the origin of a member of a class template instantiation is the
/// member of the template; of the instantiation, the template.
#[test]
fn s5_5_origin_of_member_of_instantiation() {
    with(&[("main.cpp", O)], |p, s| {
        let c = one(s, "C");
        let ci = find(p, DeclKind::CXXRecord, "C", |ast, d| {
            spec_kind(ast, d) == Some(SpecKind::ImplicitInstantiation)
        });
        assert_eq!(ci.len(), 1, "C<int>");
        assert_eq!(s.origin(s.entity_number(ci[0])), c);
        let cm = find(p, DeclKind::CXXMethod, "cm", |ast, d| {
            matches!(ast.templ_link(d).get(), Some((TemplLinkKind::MemberOfSpecialization, _)))
        });
        assert_eq!(cm.len(), 1, "C<int>::cm");
        let e = s.entity_number(cm[0]);
        assert_eq!(s.origin(e), one(s, "C::cm"));
        assert_eq!(s.parent(e), Some(s.entity_number(ci[0])));
        assert_eq!(s.qualified(e), None, "§7.1 rule 4: a member of a specialisation");
        assert_eq!(s.unqualified(e), Some(Unqualified::MemberOfSpecialisation));
        let x = find(p, DeclKind::Field, "x", |ast, d| {
            ast.decls.get(d).ctx().is_some_and(|k| ci.contains(&k))
        });
        assert_eq!(x.len(), 1, "C<int>::x");
        assert_eq!(s.origin(s.entity_number(x[0])), one(s, "C::x"));
    });
}

/// §5.5: the origin of a member specialisation of a member template
/// (`C<int>::ct<double>`) is the member template of the class template.
#[test]
fn s5_5_origin_of_member_template_specialisation() {
    with(&[("main.cpp", O)], |p, s| {
        let spec = find(p, DeclKind::CXXMethod, "ct", |ast, d| {
            spec_kind(ast, d) == Some(SpecKind::ImplicitInstantiation)
        });
        assert_eq!(spec.len(), 1, "C<int>::ct<double>");
        let e = s.entity_number(spec[0]);
        assert_eq!(s.declarations(e), Ok(vec![]));
        assert_eq!(s.origin(e), one(s, "C::ct"));
        assert!(is(s, one(s, "C::ct"), wire::KIND_TEMPLATE));
    });
}

// ---------------------------------------------------------------------------
// §7.2 rule 2: the identifiers of a fully qualified spelling.

#[test]
fn s7_2_rule2_qualified() {
    with(&[("main.cpp", L)], |_, s| {
        let q = |name: &str| {
            s.qualified(one(s, name))
                .map(|v| v.into_iter().map(|x| String::from_utf8(x).unwrap()).collect::<Vec<_>>())
        };
        assert_eq!(q("ns::v1::g").unwrap(), ["ns", "v1", "g"], "inline namespaces are named");
        assert_eq!(q("ns::h").unwrap(), ["ns", "h"], "unnamed namespaces are skipped");
        assert_eq!(q("ns::S::m").unwrap(), ["ns", "S", "m"]);
        assert_eq!(q("ns::E::A").unwrap(), ["ns", "E", "A"]);
        assert_eq!(q("ns::C").unwrap(), ["ns", "C"], "a template, without arguments");
        assert_eq!(q("ns::C::cm"), None, "§7.1 rule 4: a member of a class template");
        assert_eq!(q("ns::S::u1"), None, "§7.1 rule 4: declared in an unnamed class");
        assert_eq!(q("::"), None, "no identifier");
        let p = s.parameters(Value::Entity(look(s, "ns::f")[0])).unwrap();
        assert_eq!(s.qualified(p[0]), None, "a parameter");
        // §7.1 rule 4 (WP-137): why there is none, when it is spelled.
        let u = |name: &str| s.unqualified(one(s, name));
        assert_eq!(u("ns::C::cm"), Some(Unqualified::MemberOfClassTemplate));
        assert_eq!(u("ns::S::u1"), Some(Unqualified::InUnnamedClass));
        assert_eq!(u("::"), None, "no identifier: never spelled");
        assert_eq!(s.unqualified(p[0]), None, "a parameter: never spelled");
        assert_eq!(u("ns::S::m"), None, "a qualified name");
    });
}

// ---------------------------------------------------------------------------
// Files.

/// §6.6 rules 5 and 6 (what this package gives of them): a file's bytes,
/// length, ownership and `FileRef`; a buffer that is no file is not live.
#[test]
fn files() {
    let main = "#include <x.h>\nint m;\n";
    with(&[("own/m.cpp", main), ("lib/x.h", "int x;\n")], |_, s| {
        let files: Vec<File> = (1..64).map(File).filter(|&f| s.file_len(f).is_some()).collect();
        let path = |f: File| String::from_utf8(s.file_ref(f).path).unwrap();
        let m = *files.iter().find(|&&f| path(f) == "own/m.cpp").expect("the main file");
        let x = *files.iter().find(|&&f| path(f) == "lib/x.h").expect("the header");
        assert_eq!(files.len(), 2, "the predefines are no file of the subject");
        assert_eq!(s.file_len(m), Some(main.len() as u32));
        assert_eq!(s.text(Range { file: m, lo: 15, hi: 21 }), b"int m;");
        assert!(s.owned(m));
        assert!(!s.owned(x));
        let r = s.file_ref(x);
        assert_eq!((r.size, r.content), (7, content_digest(b"int x;\n")));
        assert_eq!(s.file_len(File(0)), None);
        assert_eq!(s.file_len(File(u32::MAX)), None);
    });
}

// ---------------------------------------------------------------------------
// Determinism and liveness.

/// §2.7: the same snippet parsed twice gives the same sequences, and a
/// repeated query in one subject gives the same answer.
#[test]
fn determinism() {
    let names = ["ns::f", "ns::g", "ns::S::u1", "ns::C::cm", "ns::fr", "other::f", "cfun", "::"];
    let run = || {
        with(&[("main.cpp", L)], |_, s| {
            let mut out = Vec::new();
            for n in names {
                let es = look(s, n);
                assert_eq!(look(s, n), es, "{n}: the same answer twice");
                for &e in &es {
                    out.push(format!(
                        "{n} {e:?} {:?} {:?} {:?} {:#x}",
                        s.declarations(e),
                        s.parent(e),
                        s.parameters(Value::Entity(e)),
                        kinds(s, e)
                    ));
                }
            }
            out
        })
    };
    assert_eq!(run(), run());
}

/// §5.2: numbers that are no value are not live.
#[test]
fn liveness() {
    with(&[("main.cpp", L)], |p, s| {
        assert!(!s.is_live(Value::Entity(Entity(0))));
        assert!(!s.is_live(Value::Entity(Entity(u32::MAX))));
        assert!(!s.is_live(Value::Declaration(Declaration(0))));
        // A declaration that is not the entity is no entity number; a
        // pattern is no declaration (its template is).
        let f = look(s, "ns::f")[0];
        let ds = s.declarations(f).unwrap();
        assert!(!s.is_live(Value::Entity(Entity(ds[1].0))), "the definition is not the entity");
        let c = one(s, "ns::C");
        let pattern = p.ast.template_info(DeclId::new(c.0)).and_then(|t| t.pattern).unwrap();
        assert!(!s.is_live(Value::Declaration(Declaration(pattern.index()))));
        assert!(!s.is_live(Value::Entity(Entity(pattern.index()))));
        assert_eq!(s.entity_number(pattern), c, "a pattern's entity is its template");
    });
}

/// The subject is `Sync` (a run's handler, `slip_script::Host`, holds it
/// by reference and must be `Send` at this Kiln pin), so a reference to
/// it is `Send`. Checked at compile time.
#[test]
fn subject_is_sync() {
    fn assert_sync<T: Sync>() {}
    fn assert_send<T: Send>() {}
    assert_sync::<KilnSubject<'_>>();
    assert_send::<&KilnSubject<'_>>();
}

// ---------------------------------------------------------------------------
// The review of WP-083: each finding's regression test.

/// The entity of the declaration of kind `kind` named `name` in context
/// `ctx` (by its context's name and spec kind), for the tests below.
fn member_of(
    p: &Parsed<'_>,
    s: &KilnSubject<'_>,
    kind: DeclKind,
    name: &str,
    ctx: DeclId,
) -> Entity {
    let ds = find(p, kind, name, |ast, d| ast.decls.get(d).ctx() == Some(ctx));
    assert_eq!(ds.len(), 1, "{name} in {ctx:?}: {ds:?}");
    s.entity_number(ds[0])
}

const XI: &str = "template <class T> struct C { void m() {} int d; };
template struct C<long>;
extern template struct C<short>;
template <class T> void ft(T) {}
template void ft<int>(int);
void use(C<short>& c) { c.m(); }
";

/// F1, §5.5, landed decision 2: an explicit instantiation (a definition,
/// `extern template`) and its members are no written declarations; their
/// origins are the template and its members.
#[test]
fn review_f1_explicit_instantiations_are_not_written() {
    with(&[("main.cpp", XI)], |p, s| {
        let c = one(s, "C");
        let cm = one(s, "C::m");
        let cd = one(s, "C::d");
        for kind in
            [SpecKind::ExplicitInstantiationDefinition, SpecKind::ExplicitInstantiationDeclaration]
        {
            let ci = find(p, DeclKind::CXXRecord, "C", |ast, d| spec_kind(ast, d) == Some(kind));
            assert!(!ci.is_empty(), "{kind:?}");
            for &d in &ci {
                assert!(!s.is_live(Value::Declaration(Declaration(d.index()))), "{kind:?}");
            }
            let e = s.entity_number(ci[0]);
            assert_eq!(s.declarations(e), Ok(vec![]), "{kind:?}");
            assert_eq!(s.origin(e), c, "{kind:?}");
            let ctx = DeclId::new(e.0);
            let m = member_of(p, s, DeclKind::CXXMethod, "m", ctx);
            assert_eq!(s.declarations(m), Ok(vec![]), "{kind:?} m");
            assert_eq!(s.origin(m), cm, "{kind:?}: what references_to(C::m) matches by");
            let d = member_of(p, s, DeclKind::Field, "d", ctx);
            assert_eq!(s.declarations(d), Ok(vec![]));
            assert_eq!(s.origin(d), cd);
        }
        let fi = find(p, DeclKind::Function, "ft", |ast, d| {
            spec_kind(ast, d) == Some(SpecKind::ExplicitInstantiationDefinition)
        });
        assert!(!fi.is_empty());
        let e = s.entity_number(fi[0]);
        assert_eq!(s.declarations(e), Ok(vec![]));
        assert_eq!(s.origin(e), one(s, "ft"));
        assert_eq!(look(s, "C"), [c]);
        assert_eq!(look(s, "ft").len(), 1);
    });
}

const PS: &str = "namespace o {
template <class T> struct C { int pm0; };
template <class T> struct C<T*> { int pm; };
template <class T> T vt = T();
template <class T> T* vt<T*> = nullptr;
}
";

/// F2, §6.1 rules 4 and 5, landed decision 8: a partial specialisation is
/// not found, nor its members, and it is a template.
#[test]
fn review_f2_partial_specialisations() {
    with(&[("main.cpp", PS)], |p, s| {
        let c = look(s, "o::C");
        assert_eq!(at(p, s, &c), ["main.cpp:2"]);
        assert_eq!(found_at(p, s, "o::vt"), ["main.cpp:4"]);
        assert!(look(s, "o::C::pm").is_empty());
        assert_eq!(found_at(p, s, "o::C::pm0"), ["main.cpp:2"]);
        let pc = find(p, DeclKind::CXXRecord, "C", |ast, d| ast.partial_spec_info(d).is_some());
        let pv = find(p, DeclKind::Var, "vt", |ast, d| ast.partial_spec_info(d).is_some());
        assert_eq!((pc.len(), pv.len()), (1, 1), "Kiln's kinds for partial specialisations");
        for d in [pc[0], pv[0]] {
            let e = s.entity_number(d);
            assert_eq!(kinds(s, e), wire::KIND_ENTITY | wire::KIND_TEMPLATE);
            assert_eq!(s.qualified(e), None);
        }
    });
}

/// F3, §6.4 rules 2 and 3: the declaration Kiln makes by deduction for an
/// explicit specialisation of a member template is not written; the
/// entity's declarations are the one written, and its parameters that
/// one's.
#[test]
fn review_f3_member_template_explicit_specialisation() {
    let src = "struct A { template <class T> void mt(T); };
template <> void A::mt<int>(int x) {}
";
    with(&[("main.cpp", src)], |p, s| {
        let ds = find(p, DeclKind::CXXMethod, "mt", |ast, d| {
            spec_kind(ast, d) == Some(SpecKind::ExplicitSpecialization)
        });
        assert!(!ds.is_empty());
        let e = s.entity_number(ds[0]);
        assert_eq!(decls_at(p, s, e), ["main.cpp:2"]);
        let ps = s.parameters(Value::Entity(e)).unwrap();
        assert_eq!(ps.iter().map(|&x| ident(s, Value::Entity(x))).collect::<Vec<_>>(), ["x"]);
        assert_eq!(s.origin(e), one(s, "A::mt"), "its origin is the member template");
        assert_eq!(found_at(p, s, "A::mt"), ["main.cpp:1"]);
    });
}

/// F4, §1: a forced include (`-include x.h`) is read before the main
/// file, so its declarations come first in unit order. Through a real
/// parse of a tree on disk.
#[test]
fn review_f4_forced_include_orders_first() {
    use slip_model::{Command, Digest, FileRef, Language, Observer, Origin, Phase};
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("wp083-forced-include");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("main.cpp"), "namespace q { void h(int); }\n").unwrap();
    std::fs::write(dir.join("x.h"), "namespace q { void h(char); }\n").unwrap();
    let argv = ["clang++", "-std=c++20", "-include", "x.h", "-c", "main.cpp"];
    let observer = Observer {
        id: Digest([3; 32]),
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
    let order =
        slip_kiln::parse_observer_with(&observer, Phase::Map, 0, written_options(), |unit| {
            let owned = |_: &[u8]| true;
            let s = KilnSubject::new(unit, &owned).expect("a unit with tables");
            let hs = s.lookup(false, &[b"q", b"h"]);
            hs.iter()
                .map(|&e| {
                    let d = s.declarations(e).unwrap()[0];
                    let text = s.display(Value::Declaration(d));
                    if text.contains("x.h") { "x.h" } else { "main.cpp" }
                })
                .collect::<Vec<_>>()
        })
        .expect("the tree parses")
        .1
        .expect("the closure ran");
    assert_eq!(order, ["x.h", "main.cpp"]);
}

/// F5, §7.1 rule 4, §7.2 rule 2: no qualified spelling for a hidden
/// friend or a class only a friend declares; an unnamed enumeration is
/// left out of a spelling.
#[test]
fn review_f5_qualified() {
    let src = "namespace n {
struct A { friend void hf(A); friend void decl_too(A); friend struct FS; };
void decl_too(A);
}
namespace e {
enum { Anon1 };
struct H { enum { HA }; };
}
";
    with(&[("main.cpp", src)], |_, s| {
        let q = |name: &str| {
            s.qualified(one(s, name))
                .map(|v| v.into_iter().map(|x| String::from_utf8(x).unwrap()).collect::<Vec<_>>())
        };
        assert_eq!(q("n::hf"), None, "a hidden friend");
        assert_eq!(q("n::FS"), None, "a class only a friend declaration declares");
        assert_eq!(q("n::decl_too").unwrap(), ["n", "decl_too"], "declared at namespace scope too");
        assert_eq!(q("e::Anon1").unwrap(), ["e", "Anon1"]);
        assert_eq!(q("e::H::HA").unwrap(), ["e", "H", "HA"]);
    });
}

/// F6, §6.1 rule 3 with [namespace.memdef]/3: the friends of a class
/// defined outside its namespace's body, and of a nested class defined
/// so, are members of the namespace.
#[test]
fn review_f6_friends_of_classes_defined_elsewhere() {
    let src = "namespace n { struct Out; struct Nest { struct In; }; }
struct n::Out { friend void outf(Out); };
struct n::Nest::In { friend void inf(In); };
namespace t2 { struct O { struct I { friend void nf(I); }; }; }
";
    with(&[("main.cpp", src)], |p, s| {
        assert_eq!(found_at(p, s, "n::outf"), ["main.cpp:2"]);
        assert_eq!(found_at(p, s, "n::inf"), ["main.cpp:3"]);
        assert!(look(s, "outf").is_empty() && look(s, "n::Out::outf").is_empty());
        assert_eq!(s.parent(one(s, "n::inf")), Some(one(s, "n")));
        assert_eq!(found_at(p, s, "t2::nf"), ["main.cpp:4"], "a friend of a nested class (M2)");
    });
}

/// F7, §6.1 rule 3 as amended: an anonymous struct is transparent like an
/// anonymous union, and a typedef name for linkage purposes names its
/// unnamed class as a scope; no other typedef does.
#[test]
fn review_f7_anonymous_struct_and_linkage_typedef() {
    let src = "namespace e {
struct H {
  union { int hu; struct { int hs; }; };
  struct { int as2; };
};
typedef struct { int tdm; } TD;
typedef H TI;
}
";
    with(&[("main.cpp", src)], |p, s| {
        assert_eq!(found_at(p, s, "e::H::hu"), ["main.cpp:3"]);
        assert_eq!(found_at(p, s, "e::H::hs"), ["main.cpp:3"]);
        assert_eq!(found_at(p, s, "e::H::as2"), ["main.cpp:4"]);
        let td = one(s, "e::TD");
        assert!(is(s, td, wire::KIND_TYPE));
        assert_eq!(ident(s, Value::Entity(td)), "TD", "still the typedef");
        assert_eq!(found_at(p, s, "e::TD::tdm"), ["main.cpp:6"]);
        assert!(look(s, "e::TI::hu").is_empty(), "another typedef is not followed");
    });
}

/// F8: the members of an implicit builtin record are not written.
#[test]
fn review_f8_builtin_record_members() {
    let src = "void f(int n, ...) { __builtin_va_list ap; (void)ap; }\n";
    with(&[("main.cpp", src)], |p, s| {
        let rec = find(p, DeclKind::CXXRecord, "__va_list_tag", |_, _| true);
        assert!(!rec.is_empty(), "the target declares __va_list_tag");
        let fields: Vec<DeclId> = p
            .ast
            .decls
            .ids()
            .filter(|&d| p.ast.decls.get(d).ctx().is_some_and(|c| rec.contains(&c)))
            .collect();
        assert!(!fields.is_empty());
        for d in fields {
            assert!(!s.is_live(Value::Declaration(Declaration(d.index()))), "{d:?}");
            assert_eq!(s.declarations(s.entity_number(d)), Ok(vec![]));
        }
    });
}

/// The surviving mutants of the review: M1 (transparency at depth), M3 (a
/// name in a scope and in its inline namespace), M4 and M5 (a class or
/// class template defined after a forward declaration).
#[test]
fn review_lookup_scopes() {
    let src =
        "namespace t1 { inline namespace a { namespace { inline namespace b { int deep; } } } }
namespace t3 { void f(int); inline namespace v { void f(char); } }
template <class T> struct FT;
template <class T> struct FT { int m4; };
struct Fw;
struct Fw { int m5; };
";
    with(&[("main.cpp", src)], |p, s| {
        assert_eq!(found_at(p, s, "t1::deep"), ["main.cpp:1"], "M1");
        assert_eq!(found_at(p, s, "t1::a::deep"), ["main.cpp:1"]);
        assert_eq!(found_at(p, s, "t3::f"), ["main.cpp:2", "main.cpp:2"], "M3");
        assert_eq!(found_at(p, s, "FT::m4"), ["main.cpp:4"], "M4");
        assert_eq!(found_at(p, s, "Fw::m5"), ["main.cpp:6"], "M5");
    });
}

/// The surviving mutants of the review: M8 (a global template's
/// parameter is written), M9 (block-scope declarations and the members of
/// a local class are written), M11 (an indirect field is a variable),
/// M20 (a member specialisation is not found under its template).
#[test]
fn review_written_and_kinds() {
    let src = "template <class T> void g8(T);
void lf() { int lx; struct LC { int lcm; }; (void)lx; }
struct U { union { int u1; }; };
template <class T> struct MS { void cm(); };
template <> void MS<int>::cm() {}
";
    with(&[("main.cpp", src)], |p, s| {
        let t = find(p, DeclKind::TemplateTypeParm, "T", |ast, d| {
            ast.decls
                .get(d)
                .ctx()
                .is_some_and(|c| ast.decls.get(c).kind() == DeclKind::TranslationUnit)
                && !ast.decls.get(d).has(DeclFlags::IMPLICIT)
        });
        assert!(!t.is_empty());
        let t = s.entity_number(t[0]);
        assert_eq!(decls_at(p, s, t), ["main.cpp:1"], "M8");
        for (kind, name, line) in [
            (DeclKind::Var, "lx", "main.cpp:2"),
            (DeclKind::CXXRecord, "LC", "main.cpp:2"),
            (DeclKind::Field, "lcm", "main.cpp:2"),
        ] {
            let d = find(p, kind, name, |_, _| true);
            assert_eq!(d.len(), 1, "{name}");
            assert_eq!(decls_at(p, s, s.entity_number(d[0])), [line], "M9: {name}");
        }
        let ind = find(p, DeclKind::IndirectField, "u1", |_, _| true);
        assert!(!ind.is_empty());
        assert_eq!(
            kinds(s, s.entity_number(ind[0])),
            wire::KIND_ENTITY | wire::KIND_VARIABLE,
            "M11"
        );
        assert_eq!(found_at(p, s, "MS::cm"), ["main.cpp:4"], "M20");
    });
}

/// The surviving mutant M18 (§5.5): the specialisation of a template that
/// is declared and never defined comes from the template although nothing
/// was instantiated.
#[test]
fn review_origin_of_specialisation_of_undefined_template() {
    let src = "template <class T> void td(T);\nvoid use() { td(1); }\n";
    with(&[("main.cpp", src)], |p, s| {
        let ds = find(p, DeclKind::Function, "td", |ast, d| ast.spec_info(d).is_some());
        assert!(!ds.is_empty(), "td<int>");
        let e = s.entity_number(ds[0]);
        assert_eq!(s.declarations(e), Ok(vec![]));
        assert_eq!(s.origin(e), one(s, "td"));
    });
}

/// F9 (landed decision 19): a closure's call operator has no written
/// declaration; the lambda-expression is the written thing.
#[test]
fn review_f9_closure_call_operator() {
    let src = "void lam() { auto l = [](int a) { int z = a; return z; }; (void)l(1); }\n";
    with(&[("main.cpp", src)], |p, s| {
        let ops = find(p, DeclKind::CXXMethod, "", |ast, d| {
            ast.decls.get(d).ctx().is_some_and(|c| ast.decls.get(c).has(DeclFlags::IMPLICIT))
        });
        assert!(!ops.is_empty(), "the closure's call operator");
        for d in ops {
            assert_eq!(s.declarations(s.entity_number(d)), Ok(vec![]));
        }
        let z = find(p, DeclKind::Var, "z", |_, _| true);
        assert_eq!(z.len(), 1);
        // The structural rule's gap (landed decision 19) is closed by the
        // extent table (WP-085): `z` is written.
        assert_eq!(decls_at(p, s, s.entity_number(z[0])), ["main.cpp:1"]);
    });
}

/// Second review, item 3: everything a lambda-expression declares climbs to
/// the implicit closure, so the structural rule found no declaration
/// (landed decision 19). WP-085 reads the extent table, which has a row for
/// each, except an init-capture (a Kiln hole, `spec/landed/edits/WP-067.md`,
/// pinned here and in `tests/subject_references.rs`).
#[test]
fn review2_lambda_declares() {
    let src = "struct P { int a, b; };
void lam(int v, P pr) {
  auto c1 = [ic = v] { return ic; };
  auto c2 = [] { struct LL { int lm; }; return LL{1}.lm; };
  auto c3 = [pr] { auto [b1, b2] = pr; return b1 + b2; };
  auto c4 = []<class LT>(LT t) { return t; };
  (void)c1(); (void)c2(); (void)c3(); (void)c4(1);
}
struct N { int n = [] { int nz = 1; return nz; }(); };
int da(int x = [] { int dz = 2; return dz; }());
";
    with(&[("main.cpp", src)], |p, s| {
        let at_of = |kind: DeclKind, name: &str| {
            let d = find(p, kind, name, |_, _| true);
            assert_eq!(d.len(), 1, "{name}: {d:?}");
            decls_at(p, s, s.entity_number(d[0]))
        };
        let none = Vec::<String>::new();
        assert_eq!(at_of(DeclKind::Var, "ic"), none, "init-capture: no row (Kiln hole)");
        assert_eq!(at_of(DeclKind::CXXRecord, "LL"), ["main.cpp:4"], "local class");
        assert_eq!(at_of(DeclKind::Field, "lm"), ["main.cpp:4"], "its member");
        assert_eq!(at_of(DeclKind::Binding, "b1"), ["main.cpp:5"], "binding");
        assert_eq!(at_of(DeclKind::Binding, "b2"), ["main.cpp:5"], "binding");
        assert_eq!(at_of(DeclKind::Var, "nz"), ["main.cpp:9"], "in a member initialiser");
        assert_eq!(at_of(DeclKind::Var, "dz"), ["main.cpp:10"], "in a default argument");
        assert_eq!(at_of(DeclKind::TemplateTypeParm, "LT"), ["main.cpp:6"], "template parameter");
    });
}

/// Second review, mutant Ma: an enum is a local context, so an enumerator of
/// a local enum is written.
#[test]
fn review2_ma_local_enum() {
    with(&[("main.cpp", "void f() { enum LE { lx }; (void)lx; }\n")], |p, s| {
        let d = find(p, DeclKind::EnumConstant, "lx", |_, _| true);
        assert_eq!(decls_at(p, s, s.entity_number(d[0])), ["main.cpp:1"], "Ma");
    });
}

/// Second review, mutant Mf: the template parameters of a member template
/// are written.
#[test]
fn review2_mf_member_template_parameter() {
    with(&[("main.cpp", "struct A { template <class MT> void m(MT); };\n")], |p, s| {
        let d = find(p, DeclKind::TemplateTypeParm, "MT", |_, _| true);
        assert_eq!(decls_at(p, s, s.entity_number(d[0])), ["main.cpp:1"], "Mf");
    });
}

/// Second review, mutant Mg: the class walk of a namespace reaches the
/// pattern of a class template, so its hidden friends are found.
#[test]
fn review2_mg_friend_of_class_template() {
    let src = "namespace w { template <class T> struct W { friend void wf(W) {} }; }\n";
    with(&[("main.cpp", src)], |p, s| {
        assert_eq!(found_at(p, s, "w::wf"), ["main.cpp:1"], "Mg");
    });
}

/// Second review, mutant Mh: only an anonymous class is transparent (§6.1
/// rule 3), not every unnamed one.
#[test]
fn review2_mh_unnamed_class_not_transparent() {
    let src = "struct { int un; } uv;\nnamespace q { struct H { struct { int hn; } hv; }; }\n";
    with(&[("main.cpp", src)], |p, s| {
        assert!(look(s, "un").is_empty(), "Mh");
        assert!(look(s, "q::H::hn").is_empty(), "Mh");
        assert_eq!(found_at(p, s, "uv"), ["main.cpp:1"]);
        assert_eq!(found_at(p, s, "q::H::hv"), ["main.cpp:2"]);
    });
}

/// Second review, mutant Mi: a member of a partial specialisation has no
/// qualified spelling.
#[test]
fn review2_mi_member_of_partial_specialisation() {
    let src = "namespace o { template <class T> struct C { int pm0; };\n\
               template <class T> struct C<T*> { int pm; }; }\n";
    with(&[("main.cpp", src)], |p, s| {
        let d = find(p, DeclKind::Field, "pm", |_, _| true);
        assert_eq!(s.qualified(s.entity_number(d[0])), None, "Mi");
    });
}

/// Second review, mutant Mj: a member of an explicit specialisation has no
/// qualified spelling.
#[test]
fn review2_mj_member_of_explicit_specialisation() {
    let src = "template <class T> struct C { int x; };\ntemplate <> struct C<int> { int sx; };\n";
    with(&[("main.cpp", src)], |p, s| {
        let d = find(p, DeclKind::Field, "sx", |_, _| true);
        assert_eq!(s.qualified(s.entity_number(d[0])), None, "Mj");
    });
}

/// Second review, mutant M24: Kiln parses default member initialisers late,
/// so the class `Q` (line 2) has a higher declaration number than the friend
/// function `Q` (line 3). Lookup's sort by unit order is needed.
#[test]
fn review2_m24_delayed_parse_order() {
    let src = "struct S {\n  int x = sizeof(struct Q*);\n  friend void Q(S);\n};\n";
    with(&[("main.cpp", src)], |p, s| {
        let found = look(s, "Q");
        assert_eq!(found.len(), 2);
        assert!(found[0].0 > found[1].0, "the premise: Kiln numbers line 2 after line 3");
        assert_eq!(at(p, s, &found), ["main.cpp:2", "main.cpp:3"], "M24");
    });
}

/// The surviving mutant M10: `export` is seen through.
#[test]
fn review_export_is_seen_through() {
    let src = "export module m;\nexport namespace xm { int xv; }\nexport { int xw; }\n";
    with(&[("main.cpp", src)], |p, s| {
        assert_eq!(found_at(p, s, "xm::xv"), ["main.cpp:2"]);
        assert_eq!(found_at(p, s, "xw"), ["main.cpp:3"]);
        assert_eq!(s.parent(one(s, "xw")), Some(one(s, "::")), "M10");
    });
}

/// The surviving mutant M17: an invalid location sorts after every valid
/// one.
#[test]
fn review_unit_order_invalid_last() {
    use slip_kiln::kiln::source::Loc;
    use slip_script::kiln_subject::unit_order;
    use std::cmp::Ordering;
    with(&[("main.cpp", "int a;\n")], |p, s| {
        let a = one(s, "a");
        let loc = p.ast.decls.get(DeclId::new(a.0)).loc();
        assert_eq!(unit_order(p.sm, loc, Loc::INVALID), Ordering::Less);
        assert_eq!(unit_order(p.sm, Loc::INVALID, loc), Ordering::Greater);
        assert_eq!(unit_order(p.sm, Loc::INVALID, Loc::INVALID), Ordering::Equal);
    });
}

// ---------------------------------------------------------------------------
// Measurement (`docs/wp/WP-083.md`, "Done when"): not part of the gate.

/// `lookup` of an absent name, after the first call, on a unit given by
/// `SLIP_WP083_DIR` (the working directory) and `SLIP_WP083_ARGV` (the
/// command, arguments separated by `\x1f`). Prints the numbers.
#[test]
#[ignore = "a measurement: run by hand on a corpus unit"]
fn measure_lookup_absent() {
    use slip_model::{Command, Digest, FileRef, Language, Observer, Origin, Phase};
    let dir = std::env::var("SLIP_WP083_DIR").expect("SLIP_WP083_DIR");
    let argv = std::env::var("SLIP_WP083_ARGV").expect("SLIP_WP083_ARGV");
    let observer = Observer {
        id: Digest([7; 32]),
        command: Command {
            directory: dir.into_bytes(),
            argv: argv.split('\x1f').map(|a| a.as_bytes().to_vec()).collect(),
            env: Vec::new(),
        },
        language: Language::Cxx,
        main: FileRef { path: Vec::new(), size: 0, content: Digest([0; 32]) },
        cost_hint: 1,
        origins: vec![Origin { configuration: 0, entry: 0 }],
    };
    let report =
        slip_kiln::parse_observer_with(&observer, Phase::Map, 0, written_options(), |unit| {
            let owned = |_: &[u8]| false;
            let s = KilnSubject::new(unit, &owned).expect("a unit with tables");
            // The last component of the first four is interned nowhere in
            // the unit, so the walk stops at the symbol table; that of the
            // last three is interned (`errs`, `size`) and absent from the
            // scopes reached, so each costs its probes.
            let names: [&[&[u8]]; 7] = [
                &[b"absent_name_wp083"],
                &[b"llvm", b"absent_name_wp083"],
                &[b"std", b"absent_name_wp083"],
                &[b"llvm", b"SmallVector", b"absent_name_wp083"],
                &[b"llvm", b"SmallVector", b"errs"],
                &[b"std", b"errs"],
                &[b"llvm", b"errs", b"size"],
            ];
            let mut lines = vec![format!("declarations {}", unit.ast().decls.len())];
            for n in names {
                let t0 = Instant::now();
                let first = s.lookup(true, n);
                let cold = t0.elapsed();
                let reps = 1000u32;
                let t1 = Instant::now();
                for _ in 0..reps {
                    assert_eq!(s.lookup(true, n), first);
                }
                let warm = t1.elapsed() / reps;
                let shown: Vec<String> =
                    n.iter().map(|c| String::from_utf8_lossy(c).into_owned()).collect();
                lines.push(format!("{}: first {cold:?}, then {warm:?} per call", shown.join("::")));
            }
            let t2 = Instant::now();
            let fs = s.lookup(true, &[b"llvm", b"errs"]);
            lines.push(format!("llvm::errs: {} entities, first {:?}", fs.len(), t2.elapsed()));
            lines
        })
        .expect("the unit parses");
    for l in report.1.expect("the closure ran") {
        println!("{l}");
    }
}
