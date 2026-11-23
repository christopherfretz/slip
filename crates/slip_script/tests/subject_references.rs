//! `KilnSubject` for references, ranges and places (`docs/wp/WP-085.md`):
//! `spec/api.md` §5.4, §5.5, §6.5, §6.6, §7.2 rule 1, §7.4 rule 3, on
//! snippets (`slip_kiln::snippet`), calling the trait directly. Each test
//! names its section and rule, or the package's test number.
//!
//! A range is shown as `path:text` (or `path:lo..hi:text` where the test
//! is byte-exact); no range as `-`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use slip_kiln::snippet::{Extra, Parsed, Snippet, parse_with_tables};
use slip_script::host::parse_qualified_name;
use slip_script::kiln_subject::KilnSubject;
use slip_script::subject::{Declaration, Entity, PlaceOf, Range, Reference, Subject, Value};

// ---------------------------------------------------------------------------
// Harness.

/// Parses `files` (the first is the main file) with Kiln's two tables and
/// calls `f` with the subject over it.
fn with<R>(files: &[(&str, &str)], f: impl FnOnce(&Parsed<'_>, &KilnSubject<'_>) -> R) -> R {
    let snippet = Snippet { files, main: files[0].0, include: &[""], std: "c++23" };
    parse_with_tables(&snippet, &Extra::default(), |p| {
        assert_eq!(p.errors, 0, "{}", p.diagnostics);
        let owned = |_: &[u8]| true;
        let s = KilnSubject::from_snippet(p, &owned).expect("a unit with tables");
        f(p, &s)
    })
    .expect("the snippet parses")
}

fn main_only<R>(src: &str, f: impl FnOnce(&Parsed<'_>, &KilnSubject<'_>) -> R) -> R {
    with(&[("main.cpp", src)], f)
}

fn one(s: &KilnSubject<'_>, name: &str) -> Entity {
    let (global, components) = parse_qualified_name(name.as_bytes()).expect("a qualified name");
    let found = s.lookup(global, &components);
    assert_eq!(found.len(), 1, "{name}: {found:?}");
    found[0]
}

/// Every live reference, by number.
fn all(s: &KilnSubject<'_>) -> Vec<Reference> {
    let mut out = Vec::new();
    while s.is_live(Value::Reference(Reference(out.len() as u32))) {
        out.push(Reference(out.len() as u32));
    }
    out
}

fn path(s: &KilnSubject<'_>, r: Range) -> String {
    String::from_utf8_lossy(&s.file_ref(r.file).path).into_owned()
}

/// `path:text`, or `-`.
fn show(s: &KilnSubject<'_>, r: Option<Range>) -> String {
    match r {
        Some(r) => format!("{}:{}", path(s, r), String::from_utf8_lossy(s.text(r))),
        None => "-".to_owned(),
    }
}

/// `path:lo..hi:text`, or `-`.
fn exact(s: &KilnSubject<'_>, r: Option<Range>) -> String {
    match r {
        Some(r) => {
            format!("{}:{}..{}:{}", path(s, r), r.lo, r.hi, String::from_utf8_lossy(s.text(r)))
        }
        None => "-".to_owned(),
    }
}

fn name(s: &KilnSubject<'_>, r: Reference) -> Option<Range> {
    s.name_range(Value::Reference(r)).expect("no error")
}

fn extent(s: &KilnSubject<'_>, r: Reference) -> Option<Range> {
    s.extent(Value::Reference(r)).expect("no error")
}

/// `name | extent` of each reference to `e`, in the order given.
fn refs(s: &KilnSubject<'_>, e: Entity) -> Vec<String> {
    s.references_to(e)
        .into_iter()
        .map(|r| format!("{} | {}", show(s, name(s, r)), show(s, extent(s, r))))
        .collect()
}

/// The same, byte-exact.
fn refs_exact(s: &KilnSubject<'_>, e: Entity) -> Vec<String> {
    s.references_to(e)
        .into_iter()
        .map(|r| format!("{} | {}", exact(s, name(s, r)), exact(s, extent(s, r))))
        .collect()
}

/// `main.cpp:lo..hi:text` of the last `text` inside the `k`-th
/// occurrence of `context` in `src`.
fn at(src: &str, context: &str, k: usize, text: &str) -> String {
    let base = src.match_indices(context).nth(k).unwrap_or_else(|| panic!("{context} #{k}")).0;
    let lo = base + context.rfind(text).unwrap_or_else(|| panic!("{text} in {context}"));
    format!("main.cpp:{}..{}:{text}", lo, lo + text.len())
}

/// The entity, among the referents of the unit's references, whose
/// display is `shown` (for names `lookup` does not take: operators,
/// conversion functions).
fn referred(s: &KilnSubject<'_>, shown: &str) -> Entity {
    let mut found: Vec<Entity> = all(s)
        .into_iter()
        .map(|r| s.referent(r))
        .filter(|&e| s.display(Value::Entity(e)).starts_with(shown))
        .collect();
    found.sort();
    found.dedup();
    let every: Vec<String> =
        all(s).into_iter().map(|r| s.display(Value::Entity(s.referent(r)))).collect();
    assert_eq!(found.len(), 1, "{shown}: {found:?} among {every:?}");
    found[0]
}

fn place(s: &KilnSubject<'_>, v: Value) -> String {
    match s.place(v) {
        PlaceOf::Own(r) => format!("own {}", show(s, Some(r))),
        PlaceOf::Nearest(r) => format!("nearest {}", show(s, Some(r))),
        PlaceOf::Unnamed => "unnamed".to_owned(),
        PlaceOf::Nowhere => "nowhere".to_owned(),
    }
}

/// The conversion function declared in class `class`.
fn conversion(s: &KilnSubject<'_>, class: &str) -> Entity {
    let c = one(s, class);
    let found: Vec<Entity> = (1..100_000)
        .map(Entity)
        .filter(|&e| s.is_live(Value::Entity(e)) && s.parent(e) == Some(c))
        .filter(|&e| s.display(Value::Entity(e)).contains("operator int"))
        .collect();
    assert_eq!(found.len(), 1, "{found:?}");
    found[0]
}

fn first_decl(s: &KilnSubject<'_>, e: Entity) -> Declaration {
    s.declarations(e).expect("declarations")[0]
}

// ---------------------------------------------------------------------------
// Construction.

/// `docs/wp/WP-085.md` "Decided since": no subject over a unit without
/// the tables.
#[test]
fn wp085_construction_fails_without_the_tables() {
    let files = [("main.cpp", "int x;")];
    let snippet = Snippet { files: &files, main: "main.cpp", include: &[""], std: "c++23" };
    let err = slip_kiln::snippet::parse(&snippet, |p| {
        let owned = |_: &[u8]| true;
        KilnSubject::from_snippet(p, &owned).err()
    })
    .unwrap();
    assert!(err.is_some_and(|e| e.contains("ParseOptions::written")));
}

// ---------------------------------------------------------------------------
// Test 1: one per role of §6.5 rule 3, byte-exact.

const ROLES: &str = "namespace a { int c; struct T { int m; void f(); };
template <class U> struct V {}; struct B { B(); }; }
int u = a::c;
void g(a::T t) { t.m = 1; t.f(); }
a::V<int> v;
struct D : a::B { int k; D() : a::B(), k(2) {} };
";

#[test]
fn s6_5_rule3_expression_name_and_qualifier() {
    main_only(ROLES, |_, s| {
        let src = ROLES;
        assert_eq!(
            refs_exact(s, one(s, "a::c")),
            [format!("{} | {}", at(src, "a::c", 0, "c"), at(src, "a::c", 0, "a::c"))]
        );
        // Qualifier components: `a` of each qualified name, unit order.
        let a = refs_exact(s, one(s, "a"));
        assert_eq!(a.len(), 5, "{a:#?}");
        let first_a = src.find("a::c").unwrap();
        assert_eq!(
            a[0],
            format!(
                "main.cpp:{first_a}..{}:a | main.cpp:{first_a}..{}:a",
                first_a + 1,
                first_a + 1
            )
        );
    });
}

#[test]
fn s6_5_rule3_member_name() {
    main_only(ROLES, |_, s| {
        let m = ROLES.find("t.m").unwrap() + 2;
        assert_eq!(
            refs_exact(s, one(s, "a::T::m")),
            [format!("main.cpp:{m}..{}:m | main.cpp:{m}..{}:m", m + 1, m + 1)]
        );
        assert_eq!(refs(s, one(s, "a::T::f")), ["main.cpp:f | main.cpp:f"]);
    });
}

#[test]
fn s6_5_rule3_type_name() {
    main_only(ROLES, |_, s| {
        assert_eq!(
            refs_exact(s, one(s, "a::T")),
            [format!("{} | {}", at(ROLES, "a::T", 0, "T"), at(ROLES, "a::T", 0, "a::T"))]
        );
    });
}

#[test]
fn s6_5_rule3_template_name() {
    main_only(ROLES, |_, s| {
        assert_eq!(
            refs_exact(s, one(s, "a::V")),
            [format!("{} | {}", at(ROLES, "a::V", 0, "V"), at(ROLES, "a::V", 0, "a::V"))]
        );
    });
}

#[test]
fn s6_5_rule3_base_specifier_and_member_initialiser() {
    main_only(ROLES, |_, s| {
        // `B` of the class: the base specifier and the member initialiser
        // naming the base (the constructor's own name `B()` in the class is
        // a declaration's).
        assert_eq!(
            refs_exact(s, one(s, "a::B")),
            [
                format!("{} | {}", at(ROLES, "a::B", 0, "B"), at(ROLES, "a::B", 0, "a::B")),
                format!("{} | {}", at(ROLES, "a::B", 1, "B"), at(ROLES, "a::B", 1, "a::B")),
            ]
        );
        assert_eq!(
            refs_exact(s, one(s, "D::k")),
            [format!("{} | {}", at(ROLES, "k(2)", 0, "k"), at(ROLES, "k(2)", 0, "k"))]
        );
    });
}

// ---------------------------------------------------------------------------
// Test 2: §6.5 rule 4, not returned.

/// Rule 4 rows over one prelude: `none` names the entities of which no
/// reference may be returned.
fn not_returned(rest: &str, none: &[&str]) {
    let src = format!(
        "namespace a {{ namespace b {{
struct T {{ void f(); static int s; }};
template <class U> void tf(U) {{}}
struct S2; }} }}
template <class C> concept Co = true;
struct P {{ int x; }};
{rest}"
    );
    main_only(&src, |_, s| {
        for n in none {
            assert_eq!(refs(s, one(s, n)), Vec::<String>::new(), "{n} in {rest}");
        }
    });
}

#[test]
fn s6_5_rule4_using_declaration() {
    not_returned("using a::b::T;\n", &["a", "a::b", "a::b::T"]);
}

#[test]
fn s6_5_rule4_using_directive() {
    not_returned("using namespace a::b;\n", &["a", "a::b"]);
}

#[test]
fn s6_5_rule4_namespace_alias_target() {
    not_returned("namespace Y = a::b;\n", &["a", "a::b"]);
}

#[test]
fn s6_5_rule4_declarator_qualifier() {
    not_returned("void a::b::T::f() {}\nint a::b::T::s = 1;\n", &["a", "a::b", "a::b::T"]);
}

#[test]
fn s6_5_rule4_friend_reference() {
    not_returned("struct W { friend struct a::b::S2; };\n", &["a", "a::b", "a::b::S2"]);
}

#[test]
fn s6_5_rule4_elaborated_declaration() {
    not_returned(
        "namespace a { namespace b { struct S2; } }\nclass Fwd;\nclass Fwd;\n",
        &["a::b::S2", "Fwd"],
    );
}

#[test]
fn s6_5_rule4_explicit_specialisation_target() {
    not_returned("template <> void a::b::tf<int>(int) {}\n", &["a", "a::b", "a::b::tf"]);
}

#[test]
fn s6_5_rule4_concept_name() {
    not_returned("static_assert(Co<int>);\n", &["Co"]);
}

#[test]
fn s6_5_rule4_designator() {
    not_returned("P pp = { .x = 1 };\n", &["P::x"]);
}

/// §6.5 rule 4 row 2: a name resolved only at instantiation.
#[test]
fn s6_5_rule4_dependent_name() {
    not_returned(
        "template <class X> void h(X t) { t.f(); a::b::tf(t); }\nvoid g() { h(a::b::T()); }\n",
        &["a::b::T::f"],
    );
}

/// §6.5 rule 4 "an elaborated reference" is Kiln's elaborated role (a
/// declaration `class X;`, a friend), not an elaborated type specifier in
/// a type, which Kiln records as a type name (Kiln `spec/ast.md` §19, the
/// role table; `spec/landed/api/WP-085.md` decision 2).
#[test]
fn s6_5_rule4_elaborated_type_specifier_is_a_type_name() {
    let src = "struct S {};\nstruct S *p;\nvoid f(struct S *q);\n";
    main_only(src, |_, s| {
        assert_eq!(refs(s, one(s, "S")), ["main.cpp:S | main.cpp:S"; 2]);
    });
}

/// A Kiln hole of the K3 kind (`spec/landed/api/WP-085.md` "For Kiln"):
/// a qualified elaborated type specifier in a declaration (`struct n::S
/// *p;`) overwrites the row of the class's definition, with the
/// specifier's qualifier as the row's. So today the qualifier `n` is
/// taken for a declarator's (§6.5 rule 4) and not returned, and the
/// definition's name range is the specifier's `S`.
#[test]
fn kiln_hole_k3_a_qualified_elaborated_specifier_overwrites_the_row() {
    let src = "namespace n { struct S {}; }\nstruct n::S *p;\n";
    main_only(src, |_, s| {
        assert_eq!(refs(s, one(s, "n::S")), ["main.cpp:S | main.cpp:n::S"]);
        assert_eq!(refs(s, one(s, "n")), Vec::<String>::new());
        let d = s.declarations(one(s, "n::S")).unwrap();
        let names: Vec<String> =
            d.iter().map(|&d| exact(s, s.name_range(Value::Declaration(d)).unwrap())).collect();
        assert_eq!(names, [at(src, "n::S", 0, "S")]);
    });
}

/// Kiln hole K1 (`spec/landed/edits/WP-067.md`): an explicit instantiation
/// of a function template has no row, so its declarator's qualifiers are
/// not known to be declarative and are returned today. Its name is a
/// specialisation target, not returned.
#[test]
fn kiln_hole_k1_explicit_instantiation_qualifiers_are_returned() {
    let src = "namespace a { template <class U> void tf(U) {} }
template void a::tf<long>(long);
";
    main_only(src, |_, s| {
        assert_eq!(refs(s, one(s, "a")), ["main.cpp:a | main.cpp:a"]);
        assert_eq!(refs(s, one(s, "a::tf")), Vec::<String>::new());
    });
}

// ---------------------------------------------------------------------------
// Test 3: a default member initialiser used by two constructors.

#[test]
fn s6_5_rule1_default_member_initialiser_once() {
    let src = "constexpr int z = 1;
struct M { int k = z; M() {} M(int) {} };
";
    main_only(src, |_, s| {
        assert_eq!(refs(s, one(s, "z")), ["main.cpp:z | main.cpp:z"]);
    });
}

// ---------------------------------------------------------------------------
// Test 4: extents (§6.6 rule 2).

#[test]
fn s6_6_rule2_extents() {
    let src = "namespace a { namespace b { int c; template <class U> int d = 0;
struct T { using x = int; operator int(); }; T operator+(T, T); } }
int u1 = a::b::c;
int u2 = ::a::b::c;
int u3 = :: /* gap */ a::b::c;
int u4 = a::b::template d<int>;
template <class T> void h() { typename T::x y; }
int u5 = a::b::operator+(a::b::T(), a::b::T());
int u6 = a::b::T().operator int();
";
    main_only(src, |_, s| {
        let c = refs(s, one(s, "a::b::c"));
        assert_eq!(
            c,
            [
                "main.cpp:c | main.cpp:a::b::c",
                "main.cpp:c | main.cpp:::a::b::c",
                "main.cpp:c | main.cpp::: /* gap */ a::b::c",
            ]
        );
        // `template` before the name is in the extent: it is after the
        // first qualifier token.
        assert_eq!(refs(s, one(s, "a::b::d")), ["main.cpp:d | main.cpp:a::b::template d"]);
        // A dependent name is not returned (§6.5 rule 4): `x` of `T::x`.
        assert_eq!(refs(s, one(s, "a::b::T::x")), Vec::<String>::new());
        // An operator name: no name range, an extent.
        assert_eq!(refs(s, referred(s, "a::b::operator+")), ["- | main.cpp:a::b::operator+"]);
        let conv = s.references_to(referred(s, "a::b::T::operator int"));
        assert_eq!(conv.len(), 1);
        assert_eq!(show(s, name(s, conv[0])), "-");
        assert_eq!(show(s, extent(s, conv[0])), "main.cpp:operator int");
    });
}

/// §6.6 rule 2: an unqualified reference's extent is its name range.
#[test]
fn s6_6_rule2_unqualified_extent_is_the_name() {
    main_only("int c;\nint u = c;\n", |_, s| {
        let r = s.references_to(one(s, "c"))[0];
        assert_eq!(name(s, r), extent(s, r));
        assert!(name(s, r).is_some());
    });
}

/// §6.6 rule 2: a destructor name's extent holds its `~`.
#[test]
fn s6_6_rule2_destructor_extent() {
    let src = "struct T { ~T(); };\nvoid g(T *p) { p->~T(); p->T::~T(); }\n";
    main_only(src, |_, s| {
        let all: Vec<String> = all(s)
            .into_iter()
            .map(|r| format!("{} | {}", show(s, name(s, r)), show(s, extent(s, r))))
            .collect();
        assert!(all.contains(&"- | main.cpp:~T".to_owned()), "{all:#?}");
        assert!(all.contains(&"- | main.cpp:T::~T".to_owned()), "{all:#?}");
    });
}

// ---------------------------------------------------------------------------
// Test 5: tiers.

#[test]
fn s5_4_tier1_argument_used_twice_is_one_reference() {
    let src = "int c;\n#define TWICE(x) x + x\nint u = TWICE(c);\n";
    main_only(src, |_, s| {
        let r = s.references_to(one(s, "c"));
        assert_eq!(r.len(), 1);
        let n = name(s, r[0]).expect("a name range");
        assert_eq!(show(s, Some(n)), "main.cpp:c");
        assert_eq!(n.lo as usize, src.rfind('c').unwrap());
        assert_eq!(s.tiers(n), Some([false, true]));
        assert_eq!(place(s, Value::Reference(r[0])), "own main.cpp:c");
    });
}

#[test]
fn s5_4_tier2_a_name_in_a_macro_body() {
    let src = "int f();\n#define CALL f() + 1\nint u = CALL;\n";
    main_only(src, |_, s| {
        let r = s.references_to(one(s, "f"));
        assert_eq!(r.len(), 1);
        assert_eq!(name(s, r[0]), None);
        assert_eq!(extent(s, r[0]), None);
        // The nearest place: the invocation, not the value's own.
        assert_eq!(place(s, Value::Reference(r[0])), "nearest main.cpp:CALL");
    });
}

/// §6.5 rule 1 (decision 3): two names written in one macro body, with
/// no ranges and one invocation, are two references.
#[test]
fn s6_5_rule1_two_names_in_one_macro_body() {
    let src = "int x;\n#define TWICE x + x\nint u = TWICE;\n";
    main_only(src, |_, s| {
        assert_eq!(refs(s, one(s, "x")), ["- | -", "- | -"]);
    });
}

/// §6.5 rule 3: a written name whose referent is not an entity (a label,
/// a contract's result name) is no reference.
#[test]
fn s6_5_rule3_referents_that_are_not_entities() {
    let src = "int f(int a) post (r: r > 0) { l: if (a) goto l; return 1; }\n";
    let snippet =
        Snippet { files: &[("main.cpp", src)], main: "main.cpp", include: &[""], std: "c++26" };
    parse_with_tables(&snippet, &Extra::default(), |p| {
        assert_eq!(p.errors, 0, "{}", p.diagnostics);
        let owned = |_: &[u8]| true;
        let s = KilnSubject::from_snippet(p, &owned).expect("a unit with tables");
        let shown: Vec<String> = all(&s)
            .into_iter()
            .map(|r| {
                format!("{}:{}", show(&s, name(&s, r)), s.is_live(Value::Entity(s.referent(r))))
            })
            .collect();
        assert_eq!(shown, ["main.cpp:a:true"]);
    })
    .expect("the snippet parses");
}

/// §5.4 rule 6 (decision 3 of WP-084): a reference that is the whole
/// expansion has the invocation as extent, at tier 0, and no name range.
#[test]
fn s5_4_whole_expansion_extent_is_the_invocation() {
    let src =
        "namespace a { int c; }\n#define CALL a::c\n#define ID(x) x\nint u = CALL + ID(a::c);\n";
    main_only(src, |_, s| {
        let r = s.references_to(one(s, "a::c"));
        assert_eq!(r.len(), 2);
        assert_eq!(show(s, name(s, r[0])), "-");
        let x = extent(s, r[0]).expect("an extent");
        assert_eq!(show(s, Some(x)), "main.cpp:CALL");
        assert_eq!(s.tiers(x), Some([true, false]));
        // `ID(a::c)`: the name is part of the argument, at tier 1; the
        // extent is the whole expansion, so the invocation, at tier 0.
        assert_eq!(show(s, name(s, r[1])), "main.cpp:c");
        let x = extent(s, r[1]).expect("an extent");
        assert_eq!(show(s, Some(x)), "main.cpp:ID(a::c)");
        assert_eq!(s.tiers(x), Some([true, false]));
    });
}

/// `docs/wp/WP-085.md` item 4: a name that is a whole expansion has no
/// name range, even when the invocation is spelled like the name.
#[test]
fn s6_6_rule1_name_that_is_a_whole_expansion() {
    let src = "int foo;\n#define ID(x) x\n#define foo2 foo\nint u = ID(foo) + foo2;\n";
    main_only(src, |_, s| {
        let r = s.references_to(one(s, "foo"));
        assert_eq!(r.len(), 2);
        // `ID(foo)`: the whole expansion; its range is the invocation,
        // which is not the identifier token.
        assert_eq!(show(s, name(s, r[0])), "-");
        assert_eq!(show(s, extent(s, r[0])), "main.cpp:ID(foo)");
        // `foo2`: the whole expansion.
        assert_eq!(show(s, name(s, r[1])), "-");
        assert_eq!(show(s, extent(s, r[1])), "main.cpp:foo2");
    });
}

// The interim refusals of `slip_kiln::ranges` (WP-084 "What a consumer
// must not assume" 4, 5, 7): no range, and the reference is returned.

/// The refusal of a span ending at `>>`, `>=` or `>>=` is `file_range`'s,
/// which takes the last token and must find its end (a split `>`). This
/// package asks `file_range_of_span` of the tables' spans, whose ends Kiln
/// gives (`spec/landed/api/WP-085.md` decision 6): an operator name ending
/// at such a token has its extent, and a reference's name never ends at a
/// split `>` (template arguments are not part of it, §6.6 rule 2).
#[test]
fn s5_4_interim_span_ending_at_a_split_greater_does_not_reach_a_reference() {
    let src = "struct S {}; int operator>>(S, S); bool operator>=(S, S);
template <class T> struct W {}; W<W<int>> w;
int u = operator>>(S(), S()) + operator>=(S(), S());
";
    main_only(src, |_, s| {
        let shr = s.references_to(referred(s, "operator>>"));
        assert_eq!(shr.len(), 1);
        assert_eq!(
            (show(s, name(s, shr[0])), show(s, extent(s, shr[0]))),
            ("-".to_owned(), "main.cpp:operator>>".to_owned())
        );
        let ge = s.references_to(referred(s, "operator>="));
        assert_eq!(ge.len(), 1);
        assert_eq!(show(s, extent(s, ge[0])), "main.cpp:operator>=");
        // `W<W<int>>`: each `W` is its own name.
        assert_eq!(refs(s, one(s, "W")), ["main.cpp:W | main.cpp:W"; 2]);
    });
}

#[test]
fn s5_4_interim_a_directive_line_inside_a_comment() {
    let src = "namespace a { int c; }\nint u = a:: /*\n#x\n*/ c;\n";
    main_only(src, |_, s| {
        let r = s.references_to(one(s, "a::c"));
        assert_eq!(r.len(), 1);
        assert_eq!(show(s, name(s, r[0])), "main.cpp:c");
        assert_eq!(extent(s, r[0]), None);
    });
}

#[test]
fn s5_4_interim_begin_and_end_out_of_order() {
    // `+ 0` so that the name is not the whole expansion (whose range would
    // be the invocation).
    let src = "namespace a { int c; }\n#define REV(x, y) y x + 0\nint u = REV(c, a::);\n";
    main_only(src, |_, s| {
        let r = s.references_to(one(s, "a::c"));
        assert_eq!(r.len(), 1);
        assert_eq!(show(s, name(s, r[0])), "main.cpp:c");
        assert_eq!(extent(s, r[0]), None);
        assert_eq!(place(s, Value::Reference(r[0])), "nearest main.cpp:c");
    });
}

/// `#embed` tokens are integer literals in a scratch buffer (WP-084
/// decision 11): no name of a reference is one, and the names around
/// them keep their ranges. A snippet's `#embed` finds nothing, so this
/// parses files written under the test's temporary directory.
#[test]
fn s5_4_interim_embed_tokens_are_no_name() {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("wp085-embed");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("d.bin"), b"AB").unwrap();
    let main = "constexpr int c = 1;\nconst int d[] = {\n#embed \"d.bin\"\n, c};\nint u = c;\n";
    std::fs::write(root.join("main.cpp"), main).unwrap();
    let got = on_disk(&root, &["clang++", "-std=c++26", "-c", "main.cpp"], |s| {
        let r = s.references_to(one(s, "c"));
        let shown: Vec<String> = r.iter().map(|&x| show(s, name(s, x))).collect();
        let bad = all(s)
            .into_iter()
            .filter(|&x| name(s, x).is_some_and(|n| !path(s, n).ends_with("main.cpp")))
            .count();
        (shown, bad)
    });
    assert_eq!(got.0.len(), 2, "{:?}", got.0);
    assert!(got.0.iter().all(|x| x.ends_with("main.cpp:c")), "{:?}", got.0);
    assert_eq!(got.1, 0);
}

/// Parses `argv` in `dir` (a real parse, with the tables) and calls `f`
/// with the subject.
/// One parse at a time: the front end resolves the command's paths
/// against the process's working directory.
static ONE: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn on_disk<R: Send>(dir: &Path, argv: &[&str], f: impl FnOnce(&KilnSubject<'_>) -> R + Send) -> R {
    use slip_model::{Command, Digest, FileRef, Language, Observer, Origin, Phase};
    let observer = Observer {
        id: Digest([7; 32]),
        command: Command {
            directory: dir.display().to_string().into_bytes(),
            argv: argv.iter().map(|a| a.as_bytes().to_vec()).collect(),
            env: Vec::new(),
        },
        language: Language::Cxx,
        main: FileRef { path: Vec::new(), size: 0, content: Digest([0; 32]) },
        cost_hint: 1,
        origins: vec![Origin { configuration: 0, entry: 0 }],
    };
    let _one = ONE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut options = slip_kiln::ParseOptions::default();
    options.written = true;
    let report = slip_kiln::parse_observer_with(&observer, Phase::Map, 0, options, |unit| {
        let owned = |_: &[u8]| true;
        f(&KilnSubject::new(unit, &owned).expect("a unit with tables"))
    })
    .expect("the unit parses");
    report.1.expect("the closure ran")
}

// ---------------------------------------------------------------------------
// Written names once (`docs/wp/WP-085.md`, WP-084 "must not assume" 2).

#[test]
fn s6_5_header_without_a_guard_entered_twice() {
    // Each entry declares its own variable, so that the header can be
    // entered twice.
    let files = [
        (
            "main.cpp",
            "int c;\n#define N u1\n#include \"h.h\"\n#undef N\n#define N u2\n#include \"h.h\"\n",
        ),
        ("h.h", "static int N = c + c;\n"),
    ];
    with(&files, |_, s| {
        let r = s.references_to(one(s, "c"));
        // Two written names, each entered twice: two references.
        assert_eq!(r.len(), 2, "{:?}", refs(s, one(s, "c")));
        let n: Vec<Range> = r.iter().map(|&x| name(s, x).unwrap()).collect();
        assert_ne!(n[0], n[1]);
        assert_eq!(s.tiers(n[0]), Some([true, false]));
        assert_eq!(s.unresolved_named(b"c"), Some(0));
    });
}

#[test]
fn s6_5_rule6_x_macro_file_counts_each_name_once() {
    let files = [
        (
            "main.cpp",
            "int c;\ntemplate <class T> void g(T t) {\n#define X(n) (void)n;\n#include \"x.def\"\n#undef X\n#define X(n) (void)(n + 1);\n#include \"x.def\"\n}\n",
        ),
        ("x.def", "X(c) X(t.m) X(frob(t))\n"),
    ];
    with(&files, |_, s| {
        let r = s.references_to(one(s, "c"));
        // One written `c`, through two definitions of `X` and two entries:
        // one reference.
        assert_eq!(r.len(), 1, "{:?}", refs(s, one(s, "c")));
        assert_eq!(show(s, name(s, r[0])), "x.def:c");
        assert_eq!(s.tiers(name(s, r[0]).unwrap()), Some([false, true]));
        // `m` and `frob` are unresolved, each written once.
        assert_eq!(s.unresolved_named(b"m"), Some(1));
        assert_eq!(s.unresolved_named(b"frob"), Some(1));
        assert_eq!(s.unresolved_named(b"nothing"), Some(0));
    });
}

/// §6.5 rule 6: the counter counts unresolved written names by spelling.
#[test]
fn s6_5_rule6_unresolved_names_by_spelling() {
    let src = "struct A { int m; };
template <class T> void h(T t) { t.m = 0; t.m = 1; f(t); typename T::m y; }
";
    main_only(src, |_, s| {
        assert_eq!(s.unresolved_named(b"m"), Some(3));
        assert_eq!(s.unresolved_named(b"f"), Some(1));
        assert_eq!(s.unresolved_named(b"A"), Some(0));
    });
}

// ---------------------------------------------------------------------------
// Test 6: references_to matches by origin (§5.5).

#[test]
fn s6_5_rule1_template_includes_its_specialisations() {
    let src = "template <class T> struct V { void m() {} };
template <> struct V<int> {};
template <class T> void tf(T) {}
void g() { V<long> a; a.m(); V<int> b; tf(1); tf<char>('c'); (void)b; }
";
    main_only(src, |_, s| {
        // `V<long>` and `V<int>` name the template; `V<int>` is an
        // explicit specialisation, its own entity, whose references are its
        // own (§5.5: an origin is the written entity a value comes from).
        let v = refs(s, one(s, "V"));
        assert!(v.contains(&"main.cpp:V | main.cpp:V".to_owned()), "{v:?}");
        assert_eq!(refs(s, one(s, "V::m")), ["main.cpp:m | main.cpp:m"]);
        assert_eq!(refs(s, one(s, "tf")), ["main.cpp:tf | main.cpp:tf"; 2]);
    });
}

#[test]
fn s6_5_rule1_overload_only_that_overload() {
    let src = "void f(int);\nvoid f(double);\nvoid g() { f(1); f(1.0); f(2); }\n";
    main_only(src, |_, s| {
        let fs = s.lookup(false, &[b"f"]);
        assert_eq!(fs.len(), 2);
        let lines = |e: Entity| -> Vec<u32> {
            s.references_to(e)
                .into_iter()
                .map(|r| name(s, r).unwrap().lo)
                .map(|lo| src[..lo as usize].matches("f(").count() as u32)
                .collect()
        };
        // Each name by how many `f(` precede it: the two declarations,
        // then the calls.
        assert_eq!(lines(fs[0]), [2, 4]);
        assert_eq!(lines(fs[1]), [3]);
    });
}

// ---------------------------------------------------------------------------
// Test 7: found_directly (§7.2 rule 1).

#[test]
fn s7_2_rule1_found_directly() {
    let src = "namespace n { int a; int b; inline namespace i { int c; } }
using n::a;
int u1 = a;
using namespace n;
int u2 = b + c;
";
    main_only(src, |_, s| {
        let a = s.references_to(one(s, "n::a"));
        assert_eq!(a.len(), 1);
        assert!(!s.found_directly(a[0]));
        for n in ["n::b", "n::i::c"] {
            let r = s.references_to(one(s, n));
            assert_eq!(r.len(), 1, "{n}");
            assert!(s.found_directly(r[0]), "{n}");
        }
    });
}

/// §7.2 rule 1: `references_at` gives the references whose extent is the
/// range, once each.
#[test]
fn s7_2_rule1_references_at() {
    let src = "namespace a { int c; }\nint u = a::c;\n";
    main_only(src, |_, s| {
        let r = s.references_to(one(s, "a::c"))[0];
        let x = extent(s, r).unwrap();
        assert_eq!(s.references_at(x), [r]);
        let n = name(s, r).unwrap();
        assert_eq!(s.references_at(n), Vec::<Reference>::new());
        // The qualifier `a` is a reference of its own range.
        let q = s.references_to(one(s, "a"))[0];
        assert_eq!(s.references_at(extent(s, q).unwrap()), [q]);
    });
}

// ---------------------------------------------------------------------------
// Test 8: unit order (§1, §6.5 rule 1).

#[test]
fn s6_5_rule1_unit_order_across_headers() {
    let files = [
        (
            "main.cpp",
            "int c;\nint m1 = c;\n#include \"b.h\"\nint m2 = c;\n#include \"a.h\"\nint m3 = c;\n",
        ),
        ("a.h", "int a1 = c;\n"),
        ("b.h", "int b1 = c;\n"),
    ];
    with(&files, |_, s| {
        let r = s.references_to(one(s, "c"));
        let lines: Vec<String> = r
            .iter()
            .map(|&x| {
                let n = name(s, x).unwrap();
                format!("{}@{}", path(s, n), n.lo)
            })
            .collect();
        let m: Vec<usize> = files[0].1.match_indices("= c").map(|(i, _)| i + 2).collect();
        assert_eq!(
            lines,
            [
                format!("main.cpp@{}", m[0]),
                "b.h@9".to_owned(),
                format!("main.cpp@{}", m[1]),
                "a.h@9".to_owned(),
                format!("main.cpp@{}", m[2]),
            ]
        );
    });
}

/// §6.5 rule 1 "outermost first": a qualifier before the name it
/// qualifies, at one first token.
#[test]
fn s6_5_rule1_outermost_first() {
    let src = "namespace a { namespace a { int c; } }\nint u = a::a::c;\n";
    main_only(src, |_, s| {
        let outer = refs(s, one(s, "a"));
        assert_eq!(outer, ["main.cpp:a | main.cpp:a"]);
        assert_eq!(refs(s, one(s, "a::a")), ["main.cpp:a | main.cpp:a::a"]);
    });
}

// ---------------------------------------------------------------------------
// Places (§7.4 rule 3) and declaration names (§6.6 rule 1).

#[test]
fn s7_4_rule3_places() {
    let src = "namespace a { int c; }\n#define DECL(n) int n;\nDECL(d)\n#define E int e;\nE\nint u = a::c;\nstruct S { operator int(); };\n";
    main_only(src, |_, s| {
        let r = s.references_to(one(s, "a::c"))[0];
        assert_eq!(place(s, Value::Reference(r)), "own main.cpp:a::c");
        let c = one(s, "a::c");
        assert_eq!(place(s, Value::Entity(c)), "own main.cpp:c");
        assert_eq!(place(s, Value::Declaration(first_decl(s, c))), "own main.cpp:c");
        // Declared through a macro argument: its own name, tier 1.
        let d = one(s, "d");
        assert_eq!(place(s, Value::Entity(d)), "own main.cpp:d");
        let dn = s.name_range(Value::Declaration(first_decl(s, d))).unwrap().unwrap();
        assert_eq!(s.tiers(dn), Some([false, true]));
        // Declared in a macro body: the invocation.
        let e = one(s, "e");
        assert_eq!(place(s, Value::Entity(e)), "nearest main.cpp:E");
        assert_eq!(s.name_range(Value::Declaration(first_decl(s, e))).unwrap(), None);
        // A conversion function: no name range, its name's tokens.
        let conv = conversion(s, "S");
        let cd = first_decl(s, conv);
        assert_eq!(s.name_range(Value::Declaration(cd)).unwrap(), None);
        assert_eq!(place(s, Value::Declaration(cd)), "nearest main.cpp:operator int");
    });
}

/// §7.4 rule 3: an entity with no written declaration is nowhere; an
/// implicit declaration has no range on any path (WP-084 "must not
/// assume" 1): it is no declaration of the subject, and a reference to it
/// is a written name.
#[test]
fn s7_4_rule3_implicit_declarations() {
    let src = "struct T {};\nvoid g(T a, T b) { a.operator=(b); }\n";
    main_only(src, |_, s| {
        let refs: Vec<Reference> = all(s);
        let assign: Vec<Reference> = refs
            .into_iter()
            .filter(|&r| s.display(Value::Entity(s.referent(r))).contains("operator="))
            .collect();
        assert_eq!(assign.len(), 1);
        let e = s.referent(assign[0]);
        assert_eq!(s.declarations(e).unwrap(), Vec::<Declaration>::new());
        assert_eq!(place(s, Value::Entity(e)), "nowhere");
        assert_eq!(show(s, name(s, assign[0])), "-");
        assert_eq!(show(s, extent(s, assign[0])), "main.cpp:operator=");
    });
}

/// §6.6 rule 1 of a declaration: the identifier token of its name.
#[test]
fn s6_6_rule1_declaration_names() {
    let src = "namespace a { struct T { void f(); }; }\nvoid a::T::f() {}\n";
    main_only(src, |_, s| {
        let f = one(s, "a::T::f");
        let ds = s.declarations(f).unwrap();
        assert_eq!(ds.len(), 2);
        let names: Vec<String> =
            ds.iter().map(|&d| exact(s, s.name_range(Value::Declaration(d)).unwrap())).collect();
        assert_eq!(names, [at(src, "f()", 0, "f"), at(src, "f()", 1, "f")]);
        assert_eq!(s.extent(Value::Declaration(ds[0])).unwrap(), None);
    });
}

/// §5.4, §7.3 rule 2: tiers are given only for a range given out.
#[test]
fn s5_4_tiers_only_for_given_ranges() {
    let src = "namespace a { int c; }\nint u = a::c;\n";
    main_only(src, |_, s| {
        let r = s.references_to(one(s, "a::c"))[0];
        let x = extent(s, r).unwrap();
        assert_eq!(s.tiers(x), Some([true, false]));
        assert_eq!(s.tiers(Range { lo: x.lo + 1, ..x }), None);
    });
}

// ---------------------------------------------------------------------------
// `written(d)` from the table (`docs/wp/WP-085.md` "Decided since").

/// Block-scope declarations and template parameters have rows: they are
/// written.
#[test]
fn s6_4_rule3_block_scope_and_template_parameters_are_written() {
    let src =
        "template <class T, int N> void g() { int local = N; struct L { int f; }; (void)local; }\n";
    main_only(src, |p, s| {
        let mut seen = BTreeMap::new();
        for i in 1..=p.ast.decls.len() as u32 {
            let d = Declaration(i);
            if s.is_live(Value::Declaration(d)) {
                let id = String::from_utf8_lossy(&s.identifier(Value::Declaration(d)).unwrap())
                    .into_owned();
                seen.insert(id, place(s, Value::Declaration(d)));
            }
        }
        for n in ["T", "N", "local", "L", "f"] {
            assert_eq!(
                seen.get(n).map(String::as_str),
                Some(&*format!("own main.cpp:{n}")),
                "{n}: {seen:?}"
            );
        }
    });
}

/// Kiln hole K3 (`spec/landed/edits/WP-067.md`): a later friend
/// declaration overwrites an earlier declaration's row, so the earlier
/// declaration's name range is the friend's name today.
#[test]
fn kiln_hole_k3_a_friend_overwrites_the_row() {
    let src = "struct S;\nstruct T { friend struct S; };\n";
    main_only(src, |_, s| {
        let e = one(s, "S");
        let ds = s.declarations(e).unwrap();
        let names: Vec<String> =
            ds.iter().map(|&d| exact(s, s.name_range(Value::Declaration(d)).unwrap())).collect();
        assert_eq!(names, [at(src, "struct S;", 1, "S")]);
    });
}

/// Kiln hole (`spec/landed/edits/WP-067.md`): an init-capture has no row,
/// so it is no written declaration today.
#[test]
fn kiln_hole_an_init_capture_has_no_row() {
    let src = "void g() { auto l = [ic = 1] { return ic; }; (void)l; }\n";
    main_only(src, |p, s| {
        for i in 1..=p.ast.decls.len() as u32 {
            let d = Declaration(i);
            if s.is_live(Value::Declaration(d)) {
                assert_ne!(s.identifier(Value::Declaration(d)).unwrap(), b"ic");
            }
        }
        // Its reference is returned.
        let r: Vec<Reference> = all(s)
            .into_iter()
            .filter(|&r| s.identifier(Value::Reference(r)).unwrap() == b"ic")
            .collect();
        assert_eq!(r.len(), 1);
        assert_eq!(place(s, Value::Entity(s.referent(r[0]))), "nowhere");
    });
}

/// §6.3: a reference's identifier is the one written there.
#[test]
fn s6_3_rule1_identifier_of_a_reference() {
    let src = "struct T { int m; operator int(); };\nint u = T().m + T().operator int();\n";
    main_only(src, |_, s| {
        let ids: Vec<Vec<u8>> =
            all(s).into_iter().map(|r| s.identifier(Value::Reference(r)).unwrap()).collect();
        assert!(ids.contains(&b"m".to_vec()));
        assert!(ids.contains(&Vec::new()));
        assert!(ids.contains(&b"T".to_vec()));
    });
}

// ---------------------------------------------------------------------------
// Rules no test above isolates (the mutations of
// `spec/landed/api/WP-085.md`).

/// §6.6 rule 1: an object-like macro named like the entity it expands to
/// (`#define c c`): the invocation is spelled like the name, but it is not
/// the name's token, so no name range.
#[test]
fn s6_6_rule1_a_macro_named_like_its_expansion() {
    let src = "int c;\n#define c c\nint u = c;\n";
    main_only(src, |_, s| {
        let r = s.references_to(one(s, "c"));
        assert_eq!(r.len(), 1);
        assert_eq!(show(s, name(s, r[0])), "-");
        assert_eq!(show(s, extent(s, r[0])), "main.cpp:c");
    });
}

/// §6.6 rule 1: a name written with a line splice is not the identifier's
/// bytes, so no name range; its extent is its token.
#[test]
fn s6_6_rule1_a_name_with_a_line_splice() {
    let src = "int foo;\nint u = fo\\\no;\n";
    main_only(src, |_, s| {
        let r = s.references_to(one(s, "foo"));
        assert_eq!(r.len(), 1);
        assert_eq!(show(s, name(s, r[0])), "-");
        assert_eq!(show(s, extent(s, r[0])), "main.cpp:fo\\\no");
    });
}

/// `docs/wp/WP-085.md` item 5: a leading `::` before a qualifier that a
/// macro produces is the token before the invocation.
#[test]
fn s6_6_rule2_leading_global_before_a_macro_qualifier() {
    let src = "namespace a { int c; }\n#define NS a\nint u = ::NS::c;\n";
    main_only(src, |_, s| {
        assert_eq!(refs(s, one(s, "a::c")), ["main.cpp:c | main.cpp:::NS::c"]);
    });
}

/// `docs/wp/WP-085.md` item 5: a `::` not found on the line of the first
/// component gives no extent; the place is then the rest of the name.
#[test]
fn s7_4_rule3_global_not_found_then_the_written_rest() {
    let src = "namespace a { int c; }\nint u = ::\na::c;\n";
    main_only(src, |_, s| {
        let r = s.references_to(one(s, "a::c"));
        assert_eq!(r.len(), 1);
        assert_eq!(extent(s, r[0]), None);
        assert_eq!(show(s, name(s, r[0])), "main.cpp:c");
        assert_eq!(place(s, Value::Reference(r[0])), "nearest main.cpp:a::c");
    });
}

/// §6.5 rule 1: one written name that resolves to two entities in two
/// entries of a header is one reference to each.
#[test]
fn s6_5_rule1_a_header_resolving_differently_in_two_entries() {
    let files = [
        (
            "main.cpp",
            "namespace A { int c; }\nnamespace B { int c; }\nnamespace A {\n#include \"h.h\"\n}\nnamespace B {\n#include \"h.h\"\n}\n",
        ),
        ("h.h", "int u = c;\n"),
    ];
    with(&files, |_, s| {
        assert_eq!(refs(s, one(s, "A::c")), ["h.h:c | h.h:c"]);
        assert_eq!(refs(s, one(s, "B::c")), ["h.h:c | h.h:c"]);
    });
}

/// §5.4, §7.3 rule 2: one range reached at tier 0 by one reference and at
/// tier 1 by another (a header in which a name is a macro in one entry
/// and a function in the other).
#[test]
fn s5_4_one_range_at_two_tiers() {
    let files = [
        (
            "main.cpp",
            "int c;\n#define F(x) (x)\n#define N u1\n#include \"h.h\"\n#undef F\n#undef N\nint F(int);\n#define N u2\n#include \"h.h\"\n",
        ),
        ("h.h", "int N = F(c);\n"),
    ];
    with(&files, |_, s| {
        let r = s.references_to(one(s, "c"));
        assert_eq!(r.len(), 2, "{:?}", refs(s, one(s, "c")));
        let n = name(s, r[0]).unwrap();
        assert_eq!(name(s, r[1]), Some(n));
        assert_eq!(s.tiers(n), Some([true, true]));
    });
}

/// §6.5 rule 1 "outermost first": two references to one entity at one
/// first token (`T::T`, the injected class name), the longer first.
#[test]
fn s6_5_rule1_outermost_first_at_one_token() {
    let src = "struct T { static int m; };\nint u = T::T::m;\n";
    main_only(src, |_, s| {
        assert_eq!(refs(s, one(s, "T")), ["main.cpp:T | main.cpp:T::T", "main.cpp:T | main.cpp:T"]);
    });
}

/// §6.5 rule 4: a declarator's qualifier that a macro produces is still a
/// declarator's.
#[test]
fn s6_5_rule4_a_declarator_qualifier_from_a_macro() {
    let src = "namespace a { struct T { void f(); }; }\n#define Q a::T\nvoid Q::f() {}\n";
    main_only(src, |_, s| {
        assert_eq!(refs(s, one(s, "a")), Vec::<String>::new());
        assert_eq!(refs(s, one(s, "a::T")), Vec::<String>::new());
    });
}

/// §6.5 rule 4: a macro in the middle of a declarator's qualifier, whose
/// two ends are file text: the names it produces are not returned.
#[test]
fn s6_5_rule4_a_macro_inside_a_declarator_qualifier() {
    let src = "namespace a { namespace b { struct T { void f(); }; } }\n#define M b\nvoid a::M::T::f() {}\nint u = sizeof(a::b::T);\n";
    main_only(src, |_, s| {
        assert_eq!(refs(s, one(s, "a::b")), ["main.cpp:b | main.cpp:a::b"]);
    });
}

/// §6.5 rule 4: a declarator's qualifier that starts in the file and ends
/// inside a macro invocation holding the name.
#[test]
fn s6_5_rule4_a_declarator_qualifier_ending_in_a_macro() {
    let src = "namespace a { struct T { void f(); }; }\n#define QN T::f\nvoid a::QN() {}\nint u = sizeof(a::T);\n";
    main_only(src, |_, s| {
        assert_eq!(refs(s, one(s, "a")), ["main.cpp:a | main.cpp:a"]);
        assert_eq!(refs(s, one(s, "a::T")), ["main.cpp:T | main.cpp:a::T"]);
    });
}

/// §6.5 rule 4: the names of a using-directive and a namespace alias are
/// not returned, whatever their rows' qualifiers.
#[test]
fn s6_5_rule4_using_directive_and_alias_whole() {
    let src = "namespace a { namespace b { int c; } }\nusing namespace a::b;\nnamespace Y = a::b;\nusing a::b::c;\n";
    main_only(src, |_, s| {
        for n in ["a", "a::b", "a::b::c"] {
            assert_eq!(refs(s, one(s, n)), Vec::<String>::new(), "{n}");
        }
    });
}

/// `spec/edits.md` §2.3 `file_recoded`: a file holding invalid UTF-8,
/// which Kiln replaces on loading, gives no range: Kiln's offsets are not
/// the file's bytes'. A snippet's files are text, so this parses files
/// written under the test's temporary directory.
#[test]
fn s5_4_a_recoded_file_gives_no_range() {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("wp085-recoded");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("bad.h"), b"// \xFF\xFE\nint u = c;\n").unwrap();
    std::fs::write(root.join("main.cpp"), "int c;\n#include \"bad.h\"\nint v = c;\n").unwrap();
    let got = on_disk(&root, &["clang++", "-std=c++20", "-c", "main.cpp"], |s| {
        s.references_to(one(s, "c"))
            .into_iter()
            .map(|r| (show(s, name(s, r)), show(s, extent(s, r)), place(s, Value::Reference(r))))
            .collect::<Vec<_>>()
    });
    assert_eq!(got.len(), 2, "{got:?}");
    assert_eq!((got[0].0.as_str(), got[0].1.as_str(), got[0].2.as_str()), ("-", "-", "unnamed"));
    assert!(got[1].0.ends_with("main.cpp:c"), "{got:?}");
}

// ---------------------------------------------------------------------------
// Test 9: over the digest's pair tests, the text of every name range is
// its referent's identifier, every extent holds its name range, and
// `references_at` of every extent holds its reference.

const PAIR_ROOTS: &[&str] = &["differ", "equal", "demand", "indeterminate"];

fn pair_dirs(root: &Path) -> Vec<PathBuf> {
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

struct Side {
    files: Vec<(String, String)>,
    std: String,
    args: Vec<String>,
    target: Option<String>,
}

/// The two sides of a pair test as the digest's runner builds them
/// (`crates/slip_digest/tests/common/mod.rs`; the same as
/// `crates/slip_kiln/tests/ranges.rs`).
fn pair_sides(dir: &Path, support: &[(String, String)]) -> Vec<Side> {
    let mut all: BTreeMap<String, String> = BTreeMap::new();
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        if e.path().is_file()
            && let (Some(n), Ok(t)) =
                (e.file_name().to_str().map(str::to_owned), std::fs::read_to_string(e.path()))
        {
            all.insert(n, t);
        }
    }
    let mut out = Vec::new();
    for side in ["a", "b"] {
        let mut files = Vec::new();
        let main = if let Some(t) = all.get("t.cpp") {
            files
                .push(("side.h".to_owned(), if side == "a" { "" } else { "#define B 1\n" }.into()));
            t.clone()
        } else if let Some(m) = all.get(&format!("{side}.cpp")) {
            m.clone()
        } else {
            continue;
        };
        files.insert(0, ("main.cpp".to_owned(), main.clone()));
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
        out.push(s);
    }
    out
}

#[derive(Default)]
struct Tally {
    references: u64,
    named: u64,
    extents: u64,
    failures: Vec<String>,
}

impl Tally {
    fn add(&mut self, o: Tally) {
        self.references += o.references;
        self.named += o.named;
        self.extents += o.extents;
        self.failures.extend(o.failures);
    }
}

/// The property of test 9 over one subject.
fn property(s: &KilnSubject<'_>, label: &str) -> Tally {
    let mut t = Tally::default();
    for r in all(s) {
        t.references += 1;
        let id = s.identifier(Value::Entity(s.referent(r))).unwrap_or_default();
        let n = name(s, r);
        let x = extent(s, r);
        if let Some(n) = n {
            t.named += 1;
            if s.text(n) != id.as_slice() {
                t.failures.push(format!(
                    "{label}: name range {} is not {:?}",
                    show(s, Some(n)),
                    String::from_utf8_lossy(&id)
                ));
            }
            if s.tiers(n).is_none_or(|k| k == [false, false]) {
                t.failures.push(format!("{label}: name range {} has no tier", show(s, Some(n))));
            }
        }
        if let Some(x) = x {
            t.extents += 1;
            if let Some(n) = n
                && !(n.file == x.file && x.lo <= n.lo && n.hi <= x.hi)
            {
                t.failures.push(format!(
                    "{label}: {} outside {}",
                    show(s, Some(n)),
                    show(s, Some(x))
                ));
            }
            if !s.references_at(x).contains(&r) {
                t.failures.push(format!("{label}: references_at {} misses it", show(s, Some(x))));
            }
            if s.tiers(x).is_none() {
                t.failures.push(format!("{label}: extent {} has no tier", show(s, Some(x))));
            }
        }
        if !s.references_to(s.origin(s.referent(r))).contains(&r) {
            t.failures.push(format!("{label}: not among the references to its origin"));
        }
    }
    t
}

fn test_threads() -> usize {
    match std::env::var("SLIP_TEST_THREADS") {
        Ok(v) => v.parse::<usize>().expect("SLIP_TEST_THREADS: a positive number").max(1),
        Err(_) => std::thread::available_parallelism().map_or(1, std::num::NonZero::get).min(4),
    }
}

#[test]
fn wp085_test9_property_over_digest_pair_tests() {
    let tests = Path::new(env!("CARGO_MANIFEST_DIR")).join("../slip_digest/tests");
    let mut support: Vec<(String, String)> = Vec::new();
    for e in std::fs::read_dir(tests.join("support")).into_iter().flatten().flatten() {
        if let (Some(n), Ok(t)) =
            (e.file_name().to_str().map(str::to_owned), std::fs::read_to_string(e.path()))
        {
            support.push((n, t));
        }
    }
    support.sort();
    let dirs: Vec<PathBuf> = PAIR_ROOTS.iter().flat_map(|r| pair_dirs(&tests.join(r))).collect();
    assert!(dirs.len() > 1000, "found only {} pair-test directories", dirs.len());
    let next = std::sync::atomic::AtomicUsize::new(0);
    let parts: Vec<Tally> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..test_threads())
            .map(|_| {
                scope.spawn(|| {
                    let mut t = Tally::default();
                    loop {
                        let k = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(dir) = dirs.get(k) else { break };
                        let label = dir.strip_prefix(&tests).unwrap_or(dir).display().to_string();
                        for (i, side) in pair_sides(dir, &support).iter().enumerate() {
                            let files: Vec<(&str, &str)> =
                                side.files.iter().map(|(p, t)| (p.as_str(), t.as_str())).collect();
                            let snippet = Snippet {
                                files: &files,
                                main: "main.cpp",
                                include: &[""],
                                std: &side.std,
                            };
                            let args: Vec<&str> = side.args.iter().map(String::as_str).collect();
                            let extra = Extra { args: &args, target: side.target.as_deref() };
                            let label = format!("{label} side {i}");
                            let got = parse_with_tables(&snippet, &extra, |p| {
                                let owned = |_: &[u8]| false;
                                let s = KilnSubject::from_snippet(p, &owned).expect("tables");
                                property(&s, &label)
                            });
                            if let Ok(got) = got {
                                t.add(got);
                            }
                        }
                    }
                    t
                })
            })
            .collect();
        workers.into_iter().map(|w| w.join().unwrap()).collect()
    });
    let mut tally = Tally::default();
    for t in parts {
        tally.add(t);
    }
    tally.failures.sort();
    eprintln!(
        "{} directories: {} references, {} with a name range, {} with an extent",
        dirs.len(),
        tally.references,
        tally.named,
        tally.extents
    );
    assert!(tally.references > 10_000, "{}", tally.references);
    assert!(tally.failures.is_empty(), "{}", tally.failures.join("\n"));
}

// ---------------------------------------------------------------------------
// Measurement (`docs/wp/WP-085.md`, "Done when"): not part of the gate.

/// Test 9's property and the costs of the subject's indexes on a unit
/// given by `SLIP_WP085_DIR` (the working directory) and `SLIP_WP085_ARGV`
/// (the command, arguments separated by `\x1f`). Prints the numbers.
#[test]
#[ignore = "a measurement: run by hand on a corpus unit"]
fn measure_references_on_a_unit() {
    use std::time::Instant;

    use slip_model::{Command, Digest, FileRef, Language, Observer, Origin, Phase};
    let dir = std::env::var("SLIP_WP085_DIR").expect("SLIP_WP085_DIR");
    let argv = std::env::var("SLIP_WP085_ARGV").expect("SLIP_WP085_ARGV");
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
    let mut options = slip_kiln::ParseOptions::default();
    options.written = true;
    let t0 = Instant::now();
    let report = slip_kiln::parse_observer_with(&observer, Phase::Map, 0, options, |unit| {
        let parsed = t0.elapsed();
        let owned = |_: &[u8]| false;
        let t1 = Instant::now();
        let s = KilnSubject::new(unit, &owned).expect("a unit with tables");
        let made = t1.elapsed();
        let written = slip_kiln::written::written_refs(unit).len();
        let t2 = Instant::now();
        let refs = all(&s);
        let first = refs.first().map(|&r| s.references_to(s.origin(s.referent(r))));
        let by_origin = t2.elapsed();
        let t3 = Instant::now();
        let at = refs.iter().find_map(|&r| extent(&s, r)).map(|x| s.references_at(x));
        let by_extent = t3.elapsed();
        let t4 = Instant::now();
        let tally = property(&s, "unit");
        let checked = t4.elapsed();
        let _ = (first, at);
        vec![
            format!("declarations {}, written-reference entries {written}", unit.ast().decls.len()),
            format!(
                "references {}, with a name range {}, with an extent {}",
                tally.references, tally.named, tally.extents
            ),
            format!("{s:?}"),
            format!("parse {parsed:?}; subject made (tables read) {made:?}"),
            format!("index by origin, first query {by_origin:?}; index by extent {by_extent:?}"),
            format!("property over every reference {checked:?}"),
            format!(
                "failures {}: {:#?}",
                tally.failures.len(),
                &tally.failures[..tally.failures.len().min(20)]
            ),
        ]
    })
    .expect("the unit parses");
    for l in report.1.expect("the closure ran") {
        println!("{l}");
    }
}
