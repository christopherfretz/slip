//! `slip_kiln::written` (work package WP-067; `spec/edits.md` §9.3 and
//! §9.7, `spec/verify.md` §10.2, `spec/api.md` §5.5): the written
//! references, dependent names, declaration names and origins of a unit,
//! read from Kiln's written-reference table (Kiln `spec/ast.md` §18) and
//! declaration-extent table (§19).
//!
//! Every unit is a tree written under the target directory and parsed by
//! the production parse (`parse_observer_with`, `ParseOptions::written`):
//! the snippet front end records neither table. Every span a test looks at
//! is mapped to bytes of the file and compared byte for byte with the
//! source at the place the test expects.
//!
//! The parse changes the process's current directory, so every test
//! holds [`LOCK`]; the pair-test sweep parses on several threads, all
//! with one directory.
//!
//! A test named `kiln_hole_…` pins what Kiln's tables lack at this pin
//! (a finding for Kiln, in the package's handback): it asserts the hole,
//! so that it fails, and is turned around, the day Kiln fills it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use slip_kiln::kiln::ast::decl::DeclKind;
use slip_kiln::kiln::ast::names::{DeclNameKind as NameKind, DeclNameView};
use slip_kiln::kiln::ast::templates::SpecKind;
use slip_kiln::kiln::ast::written_refs::RefRole;
use slip_kiln::kiln::ast::{DeclId, QualType};
use slip_kiln::kiln::source::{Loc, Span};
use slip_kiln::written::{
    DeclName, DeclNameKind, Role, decl_names, dependent_names, origin, recorded, row, written_refs,
};
use slip_kiln::{ParseOptions, Unit, parse_observer_with};
use slip_model::{Command, Digest, FileRef, Language, Observer, Origin, Phase, Status};

static LOCK: Mutex<()> = Mutex::new(());

fn lock() -> MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A directory under the target directory, emptied first.
fn tree(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("written").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::canonicalize(&dir).unwrap()
}

fn observer(dir: &Path, argv: &[String]) -> Observer {
    Observer {
        id: Digest([7; 32]),
        command: Command {
            directory: dir.to_str().unwrap().as_bytes().to_vec(),
            argv: argv.iter().map(|a| a.as_bytes().to_vec()).collect(),
            env: Vec::new(),
        },
        language: Language::Cxx,
        main: FileRef { path: Vec::new(), size: 0, content: Digest([0; 32]) },
        cost_hint: 1,
        origins: vec![Origin { configuration: 0, entry: 0 }],
    }
}

fn options(written: bool) -> ParseOptions<'static> {
    let mut o = ParseOptions::default();
    o.written = written;
    o
}

/// Parses `src` as `main.cpp` (C++20) with `written` on, expects no error,
/// and calls `f` on the unit.
fn with_unit<R: Send>(name: &str, src: &str, f: impl FnOnce(&Unit<'_>) -> R + Send) -> R {
    let _l = lock();
    let dir = tree(name);
    std::fs::write(dir.join("main.cpp"), src).unwrap();
    let argv: Vec<String> =
        ["clang++", "-std=c++20", "main.cpp"].iter().map(|s| (*s).to_owned()).collect();
    let (shard, r) =
        parse_observer_with(&observer(&dir, &argv), Phase::Map, 1, options(true), f).unwrap();
    assert_eq!(shard.outcome.status, Status::Ok, "{name}: {:?}", shard.outcome);
    let _ = std::fs::remove_dir_all(&dir);
    r.expect("a parse happened")
}

/// The bytes `[lo, hi)` of the main file that `span` spells, its two ends
/// mapped to their spelling; panics when they are not in the main file.
fn bytes(u: &Unit<'_>, span: Span) -> (usize, usize) {
    let sm = u.sm();
    let (fl, lo) = sm.offset_of(sm.spelling_loc(span.lo)).expect("lo has a file");
    let (fh, hi) = sm.offset_of(sm.spelling_loc(span.hi)).expect("hi has a file");
    assert_eq!(Some(fl), u.main_file(), "lo in the main file");
    assert_eq!(fl, fh, "both ends in one file");
    (lo as usize, hi as usize)
}

fn off(u: &Unit<'_>, loc: Loc) -> usize {
    let sm = u.sm();
    sm.offset_of(sm.spelling_loc(loc)).expect("a file location").1 as usize
}

/// The byte range of the first whole-word `word` in the one occurrence of
/// `ctx` in `src`.
fn at(src: &str, ctx: &str, word: &str) -> (usize, usize) {
    let c = src.find(ctx).unwrap_or_else(|| panic!("`{ctx}` not in the source"));
    assert!(src[c + 1..].find(ctx).is_none(), "`{ctx}` twice in the source");
    let ident = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    let bytes = ctx.as_bytes();
    let w = ctx
        .match_indices(word)
        .map(|(i, _)| i)
        .find(|&i| {
            let before = i.checked_sub(1).map(|j| bytes[j]);
            let after = bytes.get(i + word.len()).copied();
            !before.is_some_and(ident) && !after.is_some_and(ident)
        })
        .unwrap_or_else(|| panic!("`{word}` not a word of `{ctx}`"));
    (c + w, c + w + word.len())
}

/// The declaration whose name `decl_names` gives at byte `lo`.
fn decl_at(u: &Unit<'_>, lo: usize) -> DeclId {
    let found: Vec<DeclName> =
        decl_names(u).into_iter().filter(|n| bytes(u, n.name).0 == lo).collect();
    assert_eq!(found.len(), 1, "one declaration named at byte {lo}: {found:?}");
    found[0].decl
}

/// The references of role `role`, as byte ranges with their referents.
fn refs_of(u: &Unit<'_>, role: Role) -> Vec<((usize, usize), DeclId)> {
    written_refs(u)
        .iter()
        .filter(|r| r.role == role)
        .map(|r| (bytes(u, r.name), r.referent))
        .collect()
}

/// The identifier of a declaration's name: its own, a constructor's or
/// destructor's class name, a deduction guide's template name; `None`
/// for the other kinds.
fn ident_of(u: &Unit<'_>, d: DeclId) -> Option<String> {
    let ast = u.ast();
    let name = ast.decls.get(d).name();
    // An inheriting constructor's using-declaration (`using A::A;`,
    // `using AT::AT;`, `using B<T>::B;`): the name written is that of the
    // qualifier's last component, which the written-reference table
    // records as a qualifier entry: the last one inside the declaration
    // before its name, and its referent's identifier.
    if name.kind() == NameKind::Constructor
        && matches!(
            ast.decls.get(d).kind(),
            DeclKind::Using | DeclKind::UnresolvedUsingValue | DeclKind::UnresolvedUsingTypename
        )
    {
        let e = ast.decl_extents.get(d)?;
        let inside =
            |s: Span| s.lo.to_raw() >= e.full.lo.to_raw() && s.hi.to_raw() <= e.name.lo.to_raw();
        let (_, q) = ast
            .written_refs
            .iter()
            .filter(|(_, r)| r.role == RefRole::Nns && inside(r.range))
            .last()?;
        return ident_of(u, q.entity?);
    }
    match ast.names.view(name) {
        DeclNameView::Identifier(s) => Some(ast.symbols().as_str(s).to_owned()),
        DeclNameView::Constructor(t) | DeclNameView::Destructor(t) => {
            let class = ast.types.as_record(QualType::unqual(t))?;
            Some(ast.symbols().as_str(ast.decls.get(class).name().as_ident()?).to_owned())
        }
        DeclNameView::DeductionGuide(t) => {
            Some(ast.symbols().as_str(ast.decls.get(t).name().as_ident()?).to_owned())
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------
// One test per role (Kiln `spec/ast.md` §18.4; `spec/verify.md` §10.2
// rule 1), each name's span checked byte for byte.

const ROLES: &str = "\
namespace a { int v; void f(int); struct B { B(int); int m; void g(); }; }
template <class T> struct Box { T t; };
template <class T> concept Small = sizeof(T) < 4;
struct D : a::B {
  D() : a::B(1), k(2) {}
  int k;
};
void use(a::B& b) {
  a::f(a::v);
  b.g(); b.m = 1;
  a::B* p = &b;
  Box<int> x;
  static_assert(Small<char>);
}
";

#[test]
fn s9_3_role_expression_name() {
    with_unit("role-expr", ROLES, |u| {
        let s = ROLES;
        let f = decl_at(u, at(s, "void f(int)", "f").0);
        let v = decl_at(u, at(s, "int v;", "v").0);
        let got = refs_of(u, Role::Expr);
        assert!(got.contains(&(at(s, "a::f(a::v)", "f"), f)), "{got:?}");
        assert!(got.contains(&(at(s, "f(a::v)", "v"), v)), "{got:?}");
        assert!(got.iter().any(|(r, _)| *r == at(s, "= &b", "b")), "{got:?}");
        for r in written_refs(u).iter().filter(|r| r.role == Role::Expr) {
            let (lo, hi) = bytes(u, r.name);
            assert_eq!(Some(s[lo..hi].to_owned()), ident_of(u, r.referent));
        }
    });
}

#[test]
fn s9_3_role_member_name() {
    with_unit("role-member", ROLES, |u| {
        let s = ROLES;
        let g = decl_at(u, at(s, "void g()", "g").0);
        let m = decl_at(u, at(s, "int m;", "m").0);
        let got = refs_of(u, Role::Member);
        assert_eq!(got, [(at(s, "b.g()", "g"), g), (at(s, "b.m =", "m"), m)]);
    });
}

#[test]
fn s9_3_role_type_name() {
    with_unit("role-type", ROLES, |u| {
        let s = ROLES;
        let b = decl_at(u, at(s, "struct B {", "B").0);
        let got = refs_of(u, Role::Type);
        assert!(got.contains(&(at(s, "use(a::B&", "B"), b)), "{got:?}");
        assert!(got.contains(&(at(s, "a::B* p", "B"), b)), "{got:?}");
    });
}

#[test]
fn s9_3_role_template_name() {
    with_unit("role-template", ROLES, |u| {
        let s = ROLES;
        let bx = decl_at(u, at(s, "struct Box", "Box").0);
        assert_eq!(refs_of(u, Role::Template), [(at(s, "Box<int> x", "Box"), bx)]);
    });
}

#[test]
fn s9_3_role_qualifier_component() {
    with_unit("role-nns", ROLES, |u| {
        let s = ROLES;
        let a = decl_at(u, at(s, "namespace a", "a").0);
        let got = refs_of(u, Role::Nns);
        for ctx in ["a::f(a::v)", "f(a::v)", ": a::B {", "a::B(1)", "use(a::B&", "a::B* p"] {
            assert!(got.contains(&(at(s, ctx, "a"), a)), "`{ctx}`: {got:?}");
        }
        assert!(got.iter().all(|(_, d)| *d == a), "{got:?}");
    });
}

#[test]
fn s9_3_role_base_specifier() {
    with_unit("role-base", ROLES, |u| {
        let s = ROLES;
        let b = decl_at(u, at(s, "struct B {", "B").0);
        assert_eq!(refs_of(u, Role::Base), [(at(s, ": a::B {", "B"), b)]);
    });
}

#[test]
fn s9_3_role_member_initialiser() {
    with_unit("role-meminit", ROLES, |u| {
        let s = ROLES;
        let b = decl_at(u, at(s, "struct B {", "B").0);
        let k = decl_at(u, at(s, "int k;", "k").0);
        assert_eq!(
            refs_of(u, Role::MemInit),
            [(at(s, "a::B(1)", "B"), b), (at(s, "k(2)", "k"), k)]
        );
    });
}

#[test]
fn s9_3_role_concept_name() {
    with_unit("role-concept", ROLES, |u| {
        let s = ROLES;
        let c = decl_at(u, at(s, "concept Small", "Small").0);
        assert_eq!(refs_of(u, Role::Concept), [(at(s, "Small<char>", "Small"), c)]);
    });
}

// The roles of Kiln's stage 3 (WP-1932), recorded since: a
// using-declaration's name, a using-directive's namespace, a namespace
// alias's target, an elaborated or friend reference, a specialisation's
// target, a designator; their qualifiers as before (`spec/edits.md` §9.3).
#[test]
fn s9_3_stage_3_roles_are_recorded() {
    let src = "\
namespace n { int f(int); struct S { int x; }; template <class T> struct H {}; }
using n::f;
using namespace n;
namespace m = n;
struct X { friend struct n::S; };
template <> struct n::H<int> {};
n::S s = {.x = 1};
";
    with_unit("stage3", src, |u| {
        let n = decl_at(u, at(src, "namespace n {", "n").0);
        let f = decl_at(u, at(src, "int f(int)", "f").0);
        let ns = decl_at(u, at(src, "struct S {", "S").0);
        let x = decl_at(u, at(src, "int x;", "x").0);
        let h = decl_at(u, at(src, "struct H {}", "H").0);
        let role_at = |ctx: &str, word: &str| -> Vec<(Role, DeclId)> {
            written_refs(u)
                .iter()
                .filter(|r| bytes(u, r.name) == at(src, ctx, word))
                .map(|r| (r.role, r.referent))
                .collect()
        };
        assert_eq!(role_at("using n::f", "f"), [(Role::UsingDecl, f)]);
        assert_eq!(role_at("using namespace n", "n"), [(Role::UsingDirective, n)]);
        assert_eq!(role_at("m = n", "n"), [(Role::NamespaceAlias, n)]);
        assert_eq!(role_at("friend struct n::S", "S"), [(Role::Elaborated, ns)]);
        let target = role_at("struct n::H<int>", "H");
        assert!(
            target.iter().all(|&(r, _)| r == Role::SpecTarget) && !target.is_empty(),
            "{target:?}"
        );
        let _ = h;
        assert_eq!(role_at(".x = 1", "x"), [(Role::Designator, x)]);
        // Its qualifier is a reference.
        assert!(refs_of(u, Role::Nns).contains(&(at(src, "using n::f", "n"), n)));
    });
}

// ---------------------------------------------------------------------
// Repeated entries (WP-067 scope 2): a name of a default member
// initialiser used by two constructors, and of a default argument used
// by two calls, is one reference. Kiln specifies one entry at the
// written site (Kiln `spec/ast.md` §18.5) and records one at this pin;
// the joining itself is unit-tested in `src/written.rs`.

#[test]
fn s9_3_default_initialiser_and_argument_are_one_reference() {
    let src = "\
int v; int h(int);
struct A { int dm = h(v); A(); A(int); };
A::A() {}
A::A(int) {}
template <class T> struct B { int dm = h(v); B() {} B(int) {} };
B<int> b1; B<int> b2(1);
void g(int y = h(v));
void k() { g(); g(); }
struct C { void m(int y = h(v)); void n() { m(); m(); } };
";
    with_unit("repeat", src, |u| {
        let v = decl_at(u, at(src, "int v;", "v").0);
        let mut uses: Vec<(usize, usize)> =
            written_refs(u).iter().filter(|r| r.referent == v).map(|r| bytes(u, r.name)).collect();
        let want = [
            at(src, "A { int dm = h(v)", "v"),
            at(src, "B { int dm = h(v)", "v"),
            at(src, "g(int y = h(v))", "v"),
            at(src, "m(int y = h(v))", "v"),
        ];
        assert_eq!(uses, want);
        let raw = u.ast().written_refs.iter().filter(|(_, e)| e.entity == Some(v)).count();
        assert_eq!(raw, want.len(), "Kiln's table, before the joining");
        uses.dedup();
        assert_eq!(uses.len(), want.len());
    });
}

// ---------------------------------------------------------------------
// Overloads, using-declarations, qualification (`spec/edits.md` §9.3
// rows 2 and 4).

const CALLS: &str = "\
namespace a { void f(int); void f(double); }
namespace b { using a::f; }
void f(int);
void use() {
  a::f(1.0);
  b::f(1);
  ::f(2);
  ::a::f(3);
  f(4);
}
";

#[test]
fn s9_3_overloaded_call_is_the_overload_selected() {
    with_unit("overload", CALLS, |u| {
        let s = CALLS;
        let fd = decl_at(u, at(s, "void f(double)", "f").0);
        let r = written_refs(u);
        let call = r.iter().find(|r| bytes(u, r.name) == at(s, "a::f(1.0)", "f")).unwrap();
        assert_eq!(call.referent, fd);
        assert!(!call.via_using);
    });
}

#[test]
fn s9_3_call_through_a_using_declaration() {
    with_unit("via-using", CALLS, |u| {
        let s = CALLS;
        let fi = decl_at(u, at(s, "void f(int); void f(double)", "f").0);
        let r = written_refs(u);
        let call = r.iter().find(|r| bytes(u, r.name) == at(s, "b::f(1)", "f")).unwrap();
        assert_eq!(call.referent, fi, "the underlying declaration, not the shadow");
        assert!(call.via_using);
        assert_eq!(r.iter().filter(|r| r.via_using).count(), 1);
    });
}

// A found declaration that is not a using-shadow (the template of the
// specialisation a call selects) is not `via_using`.
#[test]
fn s9_3_found_template_is_not_via_using() {
    let src = "\
template <class T> void tf(T) {}
template <class T> struct S { static void m(); };
void k() { tf(1); S<int>::m(); }
";
    with_unit("found-template", src, |u| {
        let r = written_refs(u);
        let call = r.iter().find(|r| bytes(u, r.name) == at(src, "tf(1)", "tf")).unwrap();
        assert!(!call.via_using);
        assert!(r.iter().all(|r| !r.via_using), "{r:?}");
    });
}

// A friend declaration's own row is not a name: the function or class it
// declares has the row and is given, once.
#[test]
fn s9_3_friend_is_its_function_once() {
    let src = "\
struct X {
  friend void g(int) {}
  friend void h(int);
  template <class T> friend struct Y;
  friend struct Z;
};
";
    with_unit("friends", src, |u| {
        let kinds: Vec<DeclKind> =
            decl_names(u).iter().map(|n| u.ast().decls.get(n.decl).kind()).collect();
        assert!(
            !kinds.iter().any(|k| matches!(k, DeclKind::Friend | DeclKind::FriendTemplate)),
            "{kinds:?}"
        );
        let decl = DeclNameKind::Declaration;
        assert_eq!(names_at(u, src, "friend void g(int)", "g"), [(decl, DeclKind::Function)]);
        assert_eq!(names_at(u, src, "friend void h(int)", "h"), [(decl, DeclKind::Function)]);
    });
}

#[test]
fn s9_3_global_and_first() {
    with_unit("global", CALLS, |u| {
        let s = CALLS;
        let r = written_refs(u);
        let find = |ctx: &str| *r.iter().find(|r| bytes(u, r.name) == at(s, ctx, "f")).unwrap();
        let global_f = find("::f(2)");
        assert!(global_f.global);
        assert_eq!(off(u, global_f.first), at(s, "::f(2)", "f").0, "`first` is after the `::`");
        let ga = find("::a::f(3)");
        assert!(ga.global);
        assert_eq!(off(u, ga.first), at(s, "::a::f(3)", "a").0);
        let qualified = find("a::f(1.0)");
        assert!(!qualified.global);
        assert_eq!(off(u, qualified.first), at(s, "a::f(1.0)", "a").0);
        let plain = find("f(4)");
        assert!(!plain.global);
        assert_eq!(off(u, plain.first), at(s, "f(4)", "f").0);
        assert_eq!(off(u, plain.first), bytes(u, plain.name).0);
        // The qualifier `a` of `::a::f` is global too, and its own first.
        let qual_a = *r.iter().find(|r| bytes(u, r.name) == at(s, "::a::f(3)", "a")).unwrap();
        assert!(qual_a.global);
        assert_eq!(qual_a.role, Role::Nns);
        assert_eq!(off(u, qual_a.first), at(s, "::a::f(3)", "a").0);
    });
}

// A name in a macro argument substituted twice is two references with
// one name range (`spec/verify.md` §10.2): Kiln's two entries have two
// expansions, so the joining keeps both.
#[test]
fn s10_2_macro_argument_substituted_twice_is_two_references() {
    let src = "\
int f(int);
#define TWICE(x) x(1) + x(2)
int k = TWICE(f);
";
    with_unit("macro-twice", src, |u| {
        let f = decl_at(u, at(src, "int f(int)", "f").0);
        let uses: Vec<_> =
            written_refs(u).iter().filter(|r| r.referent == f).map(|r| bytes(u, r.name)).collect();
        assert_eq!(uses, [at(src, "TWICE(f)", "f"); 2]);
    });
}

// ---------------------------------------------------------------------
// Dependent names (`spec/edits.md` §9.4; WP-067 scope 3).

#[test]
fn s9_4_dependent_names_are_not_references() {
    let src = "\
void f(int);
template <class T> void t(T x) {
  f(x);
  T::h();
  x.k();
}
";
    with_unit("dependent", src, |u| {
        let t = decl_at(u, at(src, "void t(T x)", "t").0);
        let pattern = u.ast().template_info(t).and_then(|i| i.pattern).expect("a pattern");
        let deps = dependent_names(u);
        let got: Vec<_> =
            deps.iter().map(|d| (d.role, bytes(u, d.name), d.enclosing, d.unresolved)).collect();
        assert_eq!(
            got,
            [
                (Role::Expr, at(src, "f(x);", "f"), Some(pattern), false),
                (Role::Expr, at(src, "T::h()", "h"), Some(pattern), false),
                (Role::Member, at(src, "x.k()", "k"), Some(pattern), false),
            ]
        );
        let refs: Vec<_> = written_refs(u).iter().map(|r| bytes(u, r.name)).collect();
        for d in &deps {
            assert!(!refs.contains(&bytes(u, d.name)));
        }
        // `T` (the qualifier) and `x` resolve: they are references.
        assert!(refs.contains(&at(src, "T::h()", "T")));
        assert!(refs.contains(&at(src, "x.k()", "x")));
    });
}

// ---------------------------------------------------------------------
// Declaration names (`spec/edits.md` §9.3 rows 1, 6; WP-067 scope 4).

const DECLS: &str = "\
void f(int);
void f(int);
void f(int) {}
namespace N { struct C { void g(); friend void fr(int); ~C(); operator int(); bool operator==(const C&) const; }; }
void N::C::g() {}
void fr(int) {}
void blk() { extern int ext; }
namespace U { using N::C; }
template <class T> void t(T) {}
template <class T> T vt = T();
template <class T> struct S {};
template <> void t<int>(int) {}
template <> int vt<int> = 1;
template struct S<int>;
extern template struct S<long>;
int a, b;
struct Base { Base(int); };
struct Derived : Base { using Base::Base; };
";

fn names_at(u: &Unit<'_>, src: &str, ctx: &str, word: &str) -> Vec<(DeclNameKind, DeclKind)> {
    let want = at(src, ctx, word);
    decl_names(u)
        .iter()
        .filter(|n| bytes(u, n.name) == want)
        .map(|n| {
            assert_eq!(&src[want.0..want.1], word);
            (n.kind, u.ast().decls.get(n.decl).kind())
        })
        .collect()
}

#[test]
fn s9_3_decl_names() {
    with_unit("decl-names", DECLS, |u| {
        let s = DECLS;
        let fun = vec![(DeclNameKind::Declaration, DeclKind::Function)];
        let decl = DeclNameKind::Declaration;
        // A function declared twice and defined: three names.
        assert_eq!(names_at(u, s, "void f(int);\nvoid f(int);\n", "f"), fun);
        assert_eq!(names_at(u, s, "f(int);\nvoid f(int) {}", "f"), fun);
        assert_eq!(names_at(u, s, "void f(int) {}", "f"), fun);
        // An out-of-line member definition: `g` only.
        assert_eq!(names_at(u, s, "void N::C::g() {}", "g"), [(decl, DeclKind::CXXMethod)]);
        // A friend declaration: the function it declares (not the
        // `Friend`, which shares its row), and its definition.
        assert_eq!(names_at(u, s, "friend void fr(int)", "fr"), fun);
        assert_eq!(names_at(u, s, "void fr(int) {}", "fr"), fun);
        // A block-scope `extern`.
        assert_eq!(names_at(u, s, "extern int ext", "ext"), [(decl, DeclKind::Var)]);
        // A using-declaration: the member it names.
        assert_eq!(
            names_at(u, s, "using N::C", "C"),
            [(DeclNameKind::UsingDeclaration, DeclKind::Using)]
        );
        // A template: the template, once (its pattern shares the row).
        assert_eq!(names_at(u, s, "void t(T) {}", "t"), [(decl, DeclKind::FunctionTemplate)]);
        assert_eq!(names_at(u, s, "T vt = T()", "vt"), [(decl, DeclKind::VarTemplate)]);
        // Explicit specialisations: the identifier, without the arguments.
        assert_eq!(
            names_at(u, s, "void t<int>(int)", "t"),
            [(DeclNameKind::ExplicitSpecialisation, DeclKind::Function)]
        );
        assert_eq!(
            names_at(u, s, "int vt<int> = 1", "vt"),
            [(DeclNameKind::ExplicitSpecialisation, DeclKind::Var)]
        );
        // Explicit instantiations of a class template, a definition and a
        // declaration.
        let inst = [(DeclNameKind::ExplicitInstantiation, DeclKind::CXXRecord)];
        assert_eq!(names_at(u, s, "template struct S<int>", "S"), inst);
        assert_eq!(names_at(u, s, "extern template struct S<long>", "S"), inst);
        // A declarator group: two names.
        assert_eq!(names_at(u, s, "int a, b;", "a"), [(decl, DeclKind::Var)]);
        assert_eq!(names_at(u, s, "int a, b;", "b"), [(decl, DeclKind::Var)]);
        // An inheriting constructor's using-declaration: the base's name,
        // the last component (the qualifier is not the name).
        let using = [(DeclNameKind::UsingDeclaration, DeclKind::Using)];
        let (lo, _) = at(s, "using Base::Base;", "Base");
        let last = (lo + "Base::".len(), lo + "Base::Base".len());
        let got: Vec<_> = decl_names(u)
            .iter()
            .filter(|n| bytes(u, n.name) == last)
            .map(|n| (n.kind, u.ast().decls.get(n.decl).kind()))
            .collect();
        assert_eq!(got, using);
        // A destructor: the identifier after `~`. A conversion function and
        // an operator: absent.
        assert_eq!(names_at(u, s, "~C()", "C"), [(decl, DeclKind::CXXDestructor)]);
        let all = decl_names(u);
        let kinds: Vec<DeclKind> = all.iter().map(|n| u.ast().decls.get(n.decl).kind()).collect();
        assert!(!kinds.contains(&DeclKind::CXXConversion), "{kinds:?}");
        assert!(!kinds.contains(&DeclKind::Friend), "{kinds:?}");
        assert!(names_at(u, s, "operator==(const", "operator").is_empty());
        // `row` agrees with every name given.
        for n in &all {
            assert_eq!(row(u, n.decl), Some(n.kind), "{n:?}");
        }
        // Every name is its declaration's identifier, byte for byte.
        for n in &all {
            let (lo, hi) = bytes(u, n.name);
            assert_eq!(Some(s[lo..hi].to_owned()), ident_of(u, n.decl), "{n:?}");
        }
    });
}

// Kiln finding K1, fixed by Kiln WP-2470: an explicit instantiation of a
// function template or of a variable template has its row in the
// declaration-extent table, as Kiln `spec/ast.md` §19.1 says (one of a
// class template had its row before).
#[test]
fn s9_3_explicit_instantiation_of_function_and_variable_has_its_row() {
    let src = "\
template <class T> void t(T) {}
template <class T> T vt = T();
template <class T> struct S2 { void m() {} };
template void t<double>(double);
extern template void t<long>(long);
template long vt<long>;
template void S2<long>::m();
template void t(short);
";
    with_unit("inst-row", src, |u| {
        let inst = DeclNameKind::ExplicitInstantiation;
        for (ctx, word, kind) in [
            ("template void t<double>", "t", DeclKind::Function),
            ("extern template void t<long>", "t", DeclKind::Function),
            ("template long vt<long>", "vt", DeclKind::Var),
            ("template void S2<long>::m()", "m", DeclKind::CXXMethod),
            ("template void t(short)", "t", DeclKind::Function),
        ] {
            assert_eq!(names_at(u, src, ctx, word), [(inst, kind)], "`{ctx}`");
        }
        let ast = u.ast();
        let instantiated: Vec<DeclId> = ast
            .decls
            .ids()
            .filter(|&d| {
                ast.spec_info(d)
                    .map(|(_, s)| s.kind)
                    .or(ast.member_spec_info(d).map(|(_, m)| m.kind))
                    .is_some_and(|k| {
                        matches!(
                            k,
                            SpecKind::ExplicitInstantiationDefinition
                                | SpecKind::ExplicitInstantiationDeclaration
                        )
                    })
            })
            .collect();
        assert_eq!(instantiated.len(), 5, "{instantiated:?}");
        assert!(instantiated.iter().all(|&d| row(u, d) == Some(inst)));
    });
}

// Kiln finding, fixed by Kiln WP-2470: two explicit instantiations of one
// specialisation (a declaration, then a definition) are two written
// declarations, each with its row (`DeclExtents::rows_of`; the second
// replaced the first before), so both names are given.
#[test]
fn s9_3_two_explicit_instantiations_of_one_specialisation_are_two_rows() {
    let src = "\
template <class T> struct S { void m() {} };
extern template struct S<int>;
template struct S<int>;
";
    with_unit("inst-twice", src, |u| {
        let inst = [(DeclNameKind::ExplicitInstantiation, DeclKind::CXXRecord)];
        assert_eq!(names_at(u, src, ";\ntemplate struct S<int>;", "S"), inst);
        assert_eq!(names_at(u, src, "extern template struct S<int>", "S"), inst);
    });
}

// A qualified member access `a.C::f` (`spec/edits.md` §9.3 row 3): the
// member name is the last component, its qualifier a reference.
#[test]
fn s9_3_qualified_member_access() {
    let src = "\
struct B { virtual void v(); };
struct D : B { void v() override; };
void k(B& b, D& d) { b.B::v(); d.D::v(); }
";
    with_unit("qualified-member", src, |u| {
        let bv = decl_at(u, at(src, "virtual void v()", "v").0);
        let dv = decl_at(u, at(src, "void v() override", "v").0);
        assert_eq!(
            refs_of(u, Role::Member),
            [(at(src, "b.B::v()", "v"), bv), (at(src, "d.D::v()", "v"), dv)]
        );
        let r = written_refs(u);
        let m = r.iter().find(|x| bytes(u, x.name) == at(src, "b.B::v()", "v")).unwrap();
        assert_eq!(off(u, m.first), at(src, "b.B::v()", "B").0);
        assert!(!m.global);
    });
}

// ---------------------------------------------------------------------
// Origins (`spec/api.md` §5.5; WP-067 scope 5).

#[test]
fn s5_5_origin() {
    let src = "\
template <class T> void t(T) {}
template <class T> struct S {
  void m() {}
  template <class U> void mt(U) {}
  int fld;
  enum E { e1 };
};
void plain() {}
void use() {
  t(1);
  S<int> s;
  s.m();
  s.mt(1.0);
  s.fld = 1;
  (void)S<int>::e1;
  plain();
}
";
    with_unit("origin", src, |u| {
        let r = written_refs(u);
        let referent = |ctx: &str, word: &str| {
            r.iter().find(|x| bytes(u, x.name) == at(src, ctx, word)).expect(ctx).referent
        };
        let cases = [
            ("t(1)", "t", "void t(T)", "t"),
            ("s.m()", "m", "void m()", "m"),
            ("s.mt(1.0)", "mt", "void mt(U)", "mt"),
            ("s.fld", "fld", "int fld", "fld"),
            ("S<int>::e1", "e1", "{ e1 }", "e1"),
            ("plain();", "plain", "void plain()", "plain"),
        ];
        for (ctx, word, decl_ctx, decl_word) in cases {
            let d = referent(ctx, word);
            let want = decl_at(u, at(src, decl_ctx, decl_word).0);
            assert_eq!(origin(u, d), want, "`{ctx}`");
            if ctx != "plain();" {
                assert_ne!(d, want, "`{ctx}`: the referent is the instantiation");
            }
            // An origin is its own origin.
            assert_eq!(origin(u, want), want);
        }
    });
}

// ---------------------------------------------------------------------
// With `written` off (WP-067 "Tests"; `spec/model.md` §8.4: the shard is
// the same whatever the options are).

#[test]
fn s8_4_written_off_records_nothing_and_changes_no_shard() {
    let _l = lock();
    let dir = tree("off");
    std::fs::write(dir.join("main.cpp"), CALLS).unwrap();
    let argv: Vec<String> =
        ["clang++", "-std=c++20", "main.cpp"].iter().map(|s| (*s).to_owned()).collect();
    let o = observer(&dir, &argv);
    let look = |u: &Unit<'_>| {
        (recorded(u), written_refs(u).len(), dependent_names(u).len(), decl_names(u).len())
    };
    let (off_shard, off) = parse_observer_with(&o, Phase::Map, 1, options(false), look).unwrap();
    let (on_shard, on) = parse_observer_with(&o, Phase::Map, 1, options(true), look).unwrap();
    assert_eq!(off, Some((false, 0, 0, 0)));
    let (rec, refs, _, names) = on.unwrap();
    assert!(rec && refs > 0 && names > 0);
    assert_eq!(on_shard, off_shard);
    assert_eq!(on_shard.to_bytes(), off_shard.to_bytes());
    assert_eq!(
        slip_kiln::parse_observer(&o, Phase::Map, 1).unwrap().to_bytes(),
        off_shard.to_bytes()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------
// The property of `docs/wp/WP-085.md` test 9: for every reference of
// every digest pair-test snippet (both sides; `crates/slip_digest/tests/`
// `differ`, `equal`, `demand`, `indeterminate`), the text of `name` is
// the referent's identifier. Every exception is a class below, counted.

/// What one side of one snippet gave.
#[derive(Default)]
struct Tally {
    sides: usize,
    unparsed: usize,
    refs: usize,
    checked: usize,
    /// Exceptions by class, each with its first example.
    except: BTreeMap<&'static str, (usize, String)>,
    /// Failures: a text that is not the referent's identifier and no
    /// class explains.
    failures: Vec<String>,
    decl_names: usize,
    decl_rows_named: usize,
    decl_checked: usize,
    /// Entries whose `found` is a `ConstructorUsingShadow`.
    ctor_shadow_found: usize,
}

impl Tally {
    fn add(&mut self, other: Tally) {
        self.sides += other.sides;
        self.unparsed += other.unparsed;
        self.refs += other.refs;
        self.checked += other.checked;
        for (k, (n, e)) in other.except {
            let slot = self.except.entry(k).or_insert((0, e));
            slot.0 += n;
        }
        self.failures.extend(other.failures);
        self.decl_names += other.decl_names;
        self.decl_rows_named += other.decl_rows_named;
        self.decl_checked += other.decl_checked;
        self.ctor_shadow_found += other.ctor_shadow_found;
    }

    fn except(&mut self, class: &'static str, example: String) {
        self.except.entry(class).or_insert((0, example)).0 += 1;
    }
}

/// The digest tests' header lines (`crates/slip_digest/tests/common`):
/// `// std:`, `// args:`, `// target:`.
fn header(main: &str) -> (String, Vec<String>, Option<String>) {
    let (mut std, mut args, mut target) = ("c++20".to_owned(), Vec::new(), None);
    for line in main.lines() {
        let Some(rest) = line.strip_prefix("// ") else { break };
        let Some((key, value)) = rest.split_once(": ") else { break };
        match key {
            "std" => value.trim().clone_into(&mut std),
            "args" => {
                args = value.split(' ').filter(|a| !a.is_empty()).map(str::to_owned).collect();
            }
            "target" => target = Some(value.trim().to_owned()),
            "errors" => {}
            _ => break,
        }
    }
    (std, args, target)
}

/// Writes one side of the test `dir` under `root/<n>` and returns the
/// command line, as the digest tests' `side_files` makes the snippet.
fn write_side(
    dir: &Path,
    b_side: bool,
    out: &Path,
    support: &[(String, String)],
) -> Option<Vec<String>> {
    let mut files: BTreeMap<String, String> = BTreeMap::new();
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        if e.path().is_file() {
            files.insert(
                e.file_name().to_string_lossy().into_owned(),
                std::fs::read_to_string(e.path()).ok()?,
            );
        }
    }
    let main = if let Some(t) = files.get("t.cpp") {
        std::fs::write(out.join("side.h"), if b_side { "#define B 1\n" } else { "" }).ok()?;
        t.clone()
    } else {
        files.get(if b_side { "b.cpp" } else { "a.cpp" })?.clone()
    };
    std::fs::write(out.join("main.cpp"), &main).ok()?;
    for (name, text) in &files {
        if !matches!(name.as_str(), "t.cpp" | "a.cpp" | "b.cpp" | "expect.txt") {
            std::fs::write(out.join(name), text).ok()?;
        }
    }
    for (name, text) in support {
        if !files.contains_key(name) {
            std::fs::write(out.join(name), text).ok()?;
        }
    }
    let (std, args, target) = header(&main);
    let mut line = vec!["clang++".to_owned(), format!("-std={std}")];
    line.extend(target.map(|t| format!("--target={t}")));
    line.extend(args);
    line.push(format!("-I{}", out.display()));
    line.push(out.join("main.cpp").display().to_string());
    Some(line)
}

/// The class of a reference whose text is not its referent's identifier,
/// or `None` when it is.
fn classify(u: &Unit<'_>, role: Role, text: &str, referent: DeclId) -> Result<(), &'static str> {
    let ast = u.ast();
    let name = ast.decls.get(referent).name();
    match ast.names.view(name) {
        DeclNameView::Identifier(s) => {
            let id = ast.symbols().as_str(s);
            if id == text {
                Ok(())
            } else if id.is_empty() {
                Err("referent unnamed")
            } else {
                Err("text is not the identifier")
            }
        }
        DeclNameView::Constructor(_) | DeclNameView::Destructor(_) => match ident_of(u, referent) {
            Some(c) if c == text => Ok(()),
            // `using AT::AT;` (Kiln WP-1932 stage 3): the inheriting
            // constructor's name is its qualifier's last component as
            // written, a typedef's or an alias's (`s9_3_inheriting_…`).
            _ if role == Role::UsingDecl => {
                Err("inheriting constructor: the name its qualifier is written with")
            }
            _ => Err("constructor or destructor: text is not the class name"),
        },
        DeclNameView::Operator(_)
        | DeclNameView::LiteralOperator(_)
        | DeclNameView::Conversion(_) => {
            if text.starts_with("operator") {
                Err("operator-function-id (checked: starts with `operator`)")
            } else if role == Role::Expr || role == Role::Member {
                Err("operator-function-id: text does not start with `operator`")
            } else {
                Err("operator-function-id in another role")
            }
        }
        DeclNameView::DeductionGuide(_) => Err("deduction guide"),
        DeclNameView::Empty => Err("referent unnamed"),
    }
}

/// Classes of exception that the property accepts, each explained in the
/// package's handback.
const EXPLAINED: &[&str] = &[
    "operator-function-id (checked: starts with `operator`)",
    "inheriting constructor: the name its qualifier is written with",
];

fn sweep_one(line: &[String], root: &Path, label: &str) -> Tally {
    let mut t = Tally { sides: 1, ..Tally::default() };
    let o = observer(root, line);
    let look = |u: &Unit<'_>| {
        let mut t = Tally::default();
        let sm = u.sm();
        for r in written_refs(u) {
            t.refs += 1;
            let Some(text) = sm.span_text(r.name) else {
                t.except("no text for the span", format!("{label}: {r:?}"));
                continue;
            };
            match classify(u, r.role, text, r.referent) {
                Ok(()) => t.checked += 1,
                Err(class) if EXPLAINED.contains(&class) => {
                    t.except(class, format!("{label}: `{text}`"));
                }
                Err(class) => {
                    t.except(class, format!("{label}: `{text}` {:?}", r.role));
                    t.failures.push(format!(
                        "{label}: {:?} `{text}` -> {:?} ({class})",
                        r.role,
                        u.ast().decls.get(r.referent).kind()
                    ));
                }
            }
        }
        let names = decl_names(u);
        for n in &names {
            let kind = u.ast().decls.get(n.decl).kind();
            if matches!(
                kind,
                DeclKind::Friend
                    | DeclKind::FriendTemplate
                    | DeclKind::UsingDirective
                    | DeclKind::UsingEnum
            ) {
                t.failures.push(format!("{label}: a {kind:?} row is a declaration name"));
            }
            if row(u, n.decl) != Some(n.kind) {
                t.failures.push(format!("{label}: `row` disagrees with {n:?}"));
            }
        }
        // No recorded name is found as a constructor's using-shadow
        // (`via_using` counts one anyway: `crates/slip_kiln/src/written.rs`).
        for (_, e) in u.ast().written_refs.iter() {
            if e.found
                .is_some_and(|f| u.ast().decls.get(f).kind() == DeclKind::ConstructorUsingShadow)
            {
                t.ctor_shadow_found += 1;
            }
        }
        let ast = u.ast();
        for (d, e) in ast.decl_extents.iter() {
            if !e.name.lo.is_valid() {
                continue;
            }
            t.decl_rows_named += 1;
            // A named row `decl_names` leaves out: each for a reason of its
            // documentation, counted by class; any other is a failure.
            if names.iter().any(|n| n.decl == d) {
                continue;
            }
            let kind = ast.decls.get(d).kind();
            let class = if ast.described_template(d).is_some() {
                "row left out: a template's pattern"
            } else if matches!(kind, DeclKind::Friend | DeclKind::FriendTemplate) {
                "row left out: a friend declaration"
            } else if matches!(kind, DeclKind::UsingDirective | DeclKind::UsingEnum) {
                "row left out: names another declaration"
            } else if ident_of(u, d).is_none() || matches!(kind, DeclKind::CXXConversion) {
                "row left out: not an identifier"
            } else {
                t.failures.push(format!(
                    "{label}: the row of {kind:?} `{}` has no name",
                    sm.span_text(e.name).unwrap_or("?")
                ));
                continue;
            };
            t.except(class, format!("{label}: {kind:?} `{}`", sm.span_text(e.name).unwrap_or("?")));
        }
        for n in names {
            t.decl_names += 1;
            match (sm.span_text(n.name), ident_of(u, n.decl)) {
                (Some(a), Some(b)) if a == b => t.decl_checked += 1,
                (Some(a), Some(b)) if b.is_empty() => {
                    t.except("deduction guide name", format!("{label}: `{a}`"));
                }
                (a, b) => t.failures.push(format!("{label}: decl name {a:?} is not {b:?}")),
            }
        }
        t
    };
    match parse_observer_with(&o, Phase::Map, 1, options(true), look) {
        Ok((_, Some(found))) => t.add(found),
        Ok((_, None)) | Err(_) => t.unparsed += 1,
    }
    t
}

#[test]
fn wp085_9_every_reference_of_every_pair_test_names_its_referent() {
    let _l = lock();
    let digest = Path::new(env!("CARGO_MANIFEST_DIR")).join("../slip_digest/tests");
    let mut support = Vec::new();
    for e in std::fs::read_dir(digest.join("support")).unwrap().flatten() {
        support.push((
            e.file_name().to_string_lossy().into_owned(),
            std::fs::read_to_string(e.path()).unwrap(),
        ));
    }
    let root = tree("pairs");
    let mut jobs: Vec<(Vec<String>, String)> = Vec::new();
    for group in ["differ", "equal", "demand", "indeterminate"] {
        let mut dirs: Vec<PathBuf> = Vec::new();
        for short in std::fs::read_dir(digest.join(group)).unwrap().flatten() {
            for row in std::fs::read_dir(short.path()).into_iter().flatten().flatten() {
                if row.path().is_dir() {
                    dirs.push(row.path());
                }
            }
        }
        dirs.sort();
        for dir in dirs {
            for b in [false, true] {
                let out = root.join(jobs.len().to_string());
                std::fs::create_dir_all(&out).unwrap();
                let rel = dir.strip_prefix(&digest).unwrap().display().to_string();
                if let Some(line) = write_side(&dir, b, &out, &support) {
                    jobs.push((line, format!("{rel} {}", if b { "B" } else { "A" })));
                }
            }
        }
    }
    // The shapes of the package's tests that the pair tests lack.
    for (i, src) in SHAPES.iter().enumerate() {
        let out = root.join(jobs.len().to_string());
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(out.join("main.cpp"), src).unwrap();
        let line = vec![
            "clang++".to_owned(),
            "-std=c++20".to_owned(),
            out.join("main.cpp").display().to_string(),
        ];
        jobs.push((line, format!("shape {i}")));
    }
    // Every command has the one directory `root`, so the threads set one
    // current directory.
    let next = std::sync::atomic::AtomicUsize::new(0);
    let threads = std::thread::available_parallelism().map_or(1, std::num::NonZero::get).min(4);
    let mut total = Tally::default();
    std::thread::scope(|scope| {
        let handles: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| {
                    let mut t = Tally::default();
                    loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some((line, label)) = jobs.get(i) else { break };
                        t.add(sweep_one(line, &root, label));
                    }
                    t
                })
            })
            .collect();
        for h in handles {
            total.add(h.join().unwrap());
        }
    });
    eprintln!(
        "written refs over the pair tests: {} sides ({} did not parse), {} references, {} checked; \
         {} declaration names of {} named rows, {} checked",
        total.sides,
        total.unparsed,
        total.refs,
        total.checked,
        total.decl_names,
        total.decl_rows_named,
        total.decl_checked
    );
    for (class, (n, example)) in &total.except {
        eprintln!("  exception {class}: {n} (first: {example})");
    }
    eprintln!("  entries found as a constructor's using-shadow: {}", total.ctor_shadow_found);
    let _ = std::fs::remove_dir_all(&root);
    assert!(total.sides > 3000, "the sweep found the pair tests");
    assert!(total.checked > 10_000, "the sweep checked references");
    assert_eq!(total.unparsed, 0, "every side and shape parsed");
    assert_eq!(total.ctor_shadow_found, 0, "a name found as a constructor shadow: turn M11 around");
    assert!(
        total.failures.is_empty(),
        "{} failures:\n{}",
        total.failures.len(),
        total.failures.join("\n")
    );
}

// ---------------------------------------------------------------------
// Fix round 1 (the independent review of WP-067).

/// Shapes the sweep also runs (the pair tests do not have them): every
/// reference, declaration name and row of each is checked like a pair
/// test's.
const SHAPES: &[&str] = &[
    INHERITING,
    DESTRUCTORS,
    "template <class T> struct SM { void m() {} };\ntemplate <> void SM<long>::m() {}\n\
     template <class T> struct V : T { using typename T::type; using T::f; };\n",
];

const INHERITING: &str = "\
struct A { A(int); };
typedef A AT;
using AU = A;
struct X : A { using AT::AT; };
struct Z : A { using AU::AU; };
template <class T> struct B { B(int); };
template <class T> struct D : B<T> { using B<T>::B; };
X x(1); Z z(2); D<int> d(3);
";

// Review F1: an inheriting constructor's using-declaration written
// through a typedef, an alias or a template-id: its name is the last
// component as written (`AT`, `AU`, `B`), not the record it resolves to.
#[test]
fn s9_3_inheriting_constructor_named_as_written() {
    with_unit("inheriting", INHERITING, |u| {
        let s = INHERITING;
        let using = DeclNameKind::UsingDeclaration;
        assert_eq!(names_at(u, s, "::AT;", "AT"), [(using, DeclKind::Using)]);
        assert_eq!(names_at(u, s, "::AU;", "AU"), [(using, DeclKind::Using)]);
        assert_eq!(names_at(u, s, "<T>::B;", "B"), [(using, DeclKind::UnresolvedUsingValue)]);
        // The qualifiers are references to the typedef, the alias and the
        // class template.
        let roles = |ctx: &str, word: &str| -> Vec<Role> {
            written_refs(u)
                .iter()
                .filter(|r| bytes(u, r.name) == at(s, ctx, word))
                .map(|r| r.role)
                .collect()
        };
        assert_eq!(roles("using AT::", "AT"), [Role::Nns]);
        assert_eq!(roles("using AU::", "AU"), [Role::Nns]);
        assert_eq!(roles("using B<T>::", "B"), [Role::Nns]);
    });
}

const DESTRUCTORS: &str = "\
struct C { ~C(); };
struct S { ~ S(); };
S::~S() {}
template <class T> struct W { ~W(); };
struct K { operator int(); int operator+(int); };
int operator\"\"_k(unsigned long long);
void use(S* p) { p->~S(); }
";

// Review item 2: a destructor's name is the identifier after `~`, cut from
// Kiln's `~T` with the byte check; a rename of the class edits it. An
// operator, a conversion function and a literal operator stay absent.
#[test]
fn s9_3_destructor_names() {
    with_unit("destructors", DESTRUCTORS, |u| {
        let s = DESTRUCTORS;
        let dtor = [(DeclNameKind::Declaration, DeclKind::CXXDestructor)];
        assert_eq!(names_at(u, s, "~C();", "C"), dtor);
        assert_eq!(names_at(u, s, "~ S();", "S"), dtor);
        assert_eq!(names_at(u, s, "~S() {}", "S"), dtor);
        assert_eq!(names_at(u, s, "~W();", "W"), dtor);
        for n in decl_names(u) {
            let text = u.sm().span_text(n.name).unwrap();
            assert!(!text.starts_with("operator") && !text.starts_with('~'), "{text}");
            assert_ne!(u.ast().decls.get(n.decl).kind(), DeclKind::CXXConversion);
        }
        // The reference `p->~S()` names the destructor by `S`.
        let d = decl_at(u, at(s, "~S() {}", "S").0);
        let r = written_refs(u);
        let call = r.iter().find(|x| bytes(u, x.name) == at(s, "p->~S()", "S")).unwrap();
        assert_eq!(u.ast().decls.get(call.referent).kind(), DeclKind::CXXDestructor);
        assert_eq!(origin(u, call.referent), origin(u, d));
    });
}

const SPELLED: &str = "\
int caf\\u00e9 = 0;
int fo\\
o = 0;
void ba\\
r();
struct S\\u00e9 { ~S\\u00e9(); };
int caf\u{e9}2 = 0;
";

// The WP-068 review's correction (`spec/landed/edits/WP-067.md`,
// "Corrections"): a declaration whose name is written with a
// universal-character-name or split by a line splice is given, with the
// span of its name token as written, not left out; the identifier is
// compared as Kiln's symbol. `slip_kiln::sites` refuses such a name
// `not_plain` (`tests/sites.rs`, `s2_3_not_plain_a_declaration_split_by_a_splice`).
#[test]
fn s9_3_decl_names_written_with_a_ucn_or_a_splice() {
    with_unit("spelled", SPELLED, |u| {
        let s = SPELLED;
        let d = DeclNameKind::Declaration;
        assert_eq!(names_at(u, s, "int caf\\u00e9", "caf\\u00e9"), [(d, DeclKind::Var)]);
        assert_eq!(names_at(u, s, "int fo\\\no", "fo\\\no"), [(d, DeclKind::Var)]);
        assert_eq!(names_at(u, s, "void ba\\\nr", "ba\\\nr"), [(d, DeclKind::Function)]);
        assert_eq!(names_at(u, s, "struct S\\u00e9", "S\\u00e9"), [(d, DeclKind::CXXRecord)]);
        assert_eq!(names_at(u, s, "~S\\u00e9()", "S\\u00e9"), [(d, DeclKind::CXXDestructor)]);
        assert_eq!(names_at(u, s, "int caf\u{e9}2", "caf\u{e9}2"), [(d, DeclKind::Var)]);
        assert_eq!(decl_names(u).len(), 6, "every declaration, nothing else");
    });
}

// Review item 3: the origin of a template's pattern is the template, for
// class, function, variable and alias templates: the injected-class-name
// `W` and `W<T>` have one origin.
#[test]
fn s5_5_origin_of_a_pattern_is_its_template() {
    let src = "\
template <class T> struct W { W* self; W<T>* other; template <class U> struct M { M* m; }; };
template <class T> int rec(T t) { return t ? 0 : rec(0); }
template <class T> T var = T();
template <class T> using Al = T*;
W<int> w;
";
    with_unit("origin-pattern", src, |u| {
        let r = written_refs(u);
        let referent = |ctx: &str, word: &str| {
            r.iter().find(|x| bytes(u, x.name) == at(src, ctx, word)).expect(ctx).referent
        };
        let w = decl_at(u, at(src, "struct W {", "W").0);
        let injected = referent("W* self", "W");
        assert_ne!(injected, w, "the injected-class-name names the pattern");
        assert_eq!(origin(u, injected), w);
        assert_eq!(referent("W<T>* other", "W"), w);
        let m = decl_at(u, at(src, "struct M {", "M").0);
        assert_eq!(origin(u, referent("M* m", "M")), m);
        let rec = decl_at(u, at(src, "int rec(T t)", "rec").0);
        assert_eq!(origin(u, referent("rec(0)", "rec")), rec);
        for (ctx, word) in [
            ("struct W {", "W"),
            ("int rec(T t)", "rec"),
            ("T var =", "var"),
            ("using Al =", "Al"),
            ("struct M {", "M"),
        ] {
            let t = decl_at(u, at(src, ctx, word).0);
            let pattern = u.ast().template_info(t).and_then(|i| i.pattern).expect(ctx);
            assert_ne!(pattern, t);
            assert_eq!(origin(u, pattern), t, "`{ctx}`");
        }
    });
}

// Review item 4: `row` reads the table for every row, those `decl_names`
// leaves out included.
#[test]
fn s9_3_row_of_every_row() {
    let src = "\
namespace n { int v; enum class E { e }; }
using namespace n;
using enum n::E;
template <class T> void t(T) {}
template <class T> struct S { void m() {} };
template <> void t<int>(int) {}
template struct S<char>;
template void t<long>(long);
struct C { ~C(); operator int(); int operator+(int); friend void fr(int); C(int); };
void use(int) { int k = 1; auto l = [&k2 = k]() { return k2; }; (void)l; }
";
    with_unit("row", src, |u| {
        let ast = u.ast();
        let decl = Some(DeclNameKind::Declaration);
        for n in decl_names(u) {
            assert_eq!(row(u, n.decl), Some(n.kind), "{n:?}");
        }
        let rows_of = |kind: DeclKind| -> Vec<DeclId> {
            ast.decl_extents
                .iter()
                .map(|(d, _)| d)
                .filter(|&d| ast.decls.get(d).kind() == kind)
                .collect()
        };
        assert_eq!(
            rows_of(DeclKind::UsingDirective).iter().map(|&d| row(u, d)).collect::<Vec<_>>(),
            [Some(DeclNameKind::UsingDirective)]
        );
        assert_eq!(
            rows_of(DeclKind::UsingEnum).iter().map(|&d| row(u, d)).collect::<Vec<_>>(),
            [Some(DeclNameKind::UsingEnum)]
        );
        assert_eq!(
            rows_of(DeclKind::Friend).iter().map(|&d| row(u, d)).collect::<Vec<_>>(),
            [decl]
        );
        for kind in [DeclKind::CXXDestructor, DeclKind::CXXConversion] {
            assert_eq!(
                rows_of(kind).iter().map(|&d| row(u, d)).collect::<Vec<_>>(),
                [decl],
                "{kind:?}"
            );
        }
        assert!(names_at(u, src, "using namespace n;", "n").is_empty());
        assert!(names_at(u, src, "using enum n::E;", "E").is_empty());
        // The operator, the unnamed parameter, the template's pattern.
        let op = names_at(u, src, "int operator+(int)", "operator");
        assert!(op.is_empty());
        let unnamed: Vec<DeclId> = rows_of(DeclKind::ParmVar)
            .into_iter()
            .filter(|&d| !ast.decl_extents.get(d).unwrap().name.lo.is_valid())
            .collect();
        assert!(!unnamed.is_empty() && unnamed.iter().all(|&d| row(u, d) == decl));
        let t = decl_at(u, at(src, "void t(T) {}", "t").0);
        let pattern = ast.template_info(t).and_then(|i| i.pattern).unwrap();
        assert_eq!(row(u, pattern), decl);
        assert_eq!(row(u, t), decl);
        // The specialisations: explicit, instantiated (a class template's
        // and, since Kiln WP-2470, a function template's: Kiln finding K1).
        assert_eq!(
            row(u, decl_at(u, at(src, "void t<int>", "t").0)),
            Some(DeclNameKind::ExplicitSpecialisation)
        );
        assert_eq!(
            row(u, decl_at(u, at(src, "template struct S<char>", "S").0)),
            Some(DeclNameKind::ExplicitInstantiation)
        );
        let long = ast
            .decls
            .ids()
            .find(|&d| {
                ast.spec_info(d)
                    .is_some_and(|(_, s)| s.kind == SpecKind::ExplicitInstantiationDefinition)
                    && ast.decls.get(d).kind() == DeclKind::Function
            })
            .expect("t<long>");
        assert_eq!(row(u, long), Some(DeclNameKind::ExplicitInstantiation));
        // An init-capture's declared name has its row (Kiln WP-2470).
        let r = written_refs(u);
        let k2 =
            r.iter().find(|x| bytes(u, x.name) == at(src, "return k2", "k2")).unwrap().referent;
        assert_eq!(row(u, k2), decl);
    });
}

// Review item 8 (surviving mutants M07, M08, M13): a member of a class
// template specialisation explicitly specialised is an explicit
// specialisation (by its `MemberSpecInfo`); a dependent using-declaration
// is a using-declaration; a using-directive and a `using enum` are not
// declaration names.
#[test]
fn s9_3_member_specialisation_and_dependent_using() {
    let src = SHAPES[2];
    with_unit("member-spec", src, |u| {
        assert_eq!(
            names_at(u, src, "SM<long>::m()", "m"),
            [(DeclNameKind::ExplicitSpecialisation, DeclKind::CXXMethod)]
        );
        let using = DeclNameKind::UsingDeclaration;
        assert_eq!(
            names_at(u, src, "T::type;", "type"),
            [(using, DeclKind::UnresolvedUsingTypename)]
        );
        assert_eq!(names_at(u, src, "T::f;", "f"), [(using, DeclKind::UnresolvedUsingValue)]);
    });
}

// Review item 7: `via_using` is also true for an enumerator named after
// `using enum` (its `found` is that declaration's shadow), not only after
// a using-declaration.
#[test]
fn s9_3_via_using_through_using_enum() {
    let src = "\
enum class E { x, y };
void k() { using enum E; (void)x; (void)E::y; }
";
    with_unit("using-enum", src, |u| {
        let r = written_refs(u);
        let find = |ctx: &str, word: &str| {
            *r.iter().find(|x| bytes(u, x.name) == at(src, ctx, word)).expect(ctx)
        };
        let x = find("(void)x", "x");
        assert!(x.via_using);
        assert_eq!(x.referent, decl_at(u, at(src, "{ x, y }", "x").0));
        assert!(!find("E::y", "y").via_using);
    });
}

// The rows of the landed file's list that the review corrected: an
// elaborated type specifier and an attribute argument that is an
// expression are references.
#[test]
fn s9_3_elaborated_type_and_attribute_expression_are_references() {
    let src = "\
void fn(int*);
constexpr int N = 8;
struct S {};
namespace n { struct E {}; }
struct alignas(N) AN {};
void k() {
  struct S* p = nullptr;
  struct n::E* q = nullptr;
  int v __attribute__((cleanup(fn))) = 0;
  [[gnu::cleanup(fn)]] int w = 0;
  (void)p; (void)q; (void)v; (void)w;
}
";
    with_unit("elaborated-attr", src, |u| {
        let s = decl_at(u, at(src, "struct S {}", "S").0);
        let e = decl_at(u, at(src, "struct E {}", "E").0);
        let f = decl_at(u, at(src, "void fn(int*)", "fn").0);
        let n = decl_at(u, at(src, "int N = 8", "N").0);
        let types = refs_of(u, Role::Type);
        assert!(types.contains(&(at(src, "struct S* p", "S"), s)));
        assert!(types.contains(&(at(src, "n::E* q", "E"), e)));
        let exprs = refs_of(u, Role::Expr);
        assert!(exprs.contains(&(at(src, "alignas(N)", "N"), n)));
        assert!(exprs.contains(&(at(src, "((cleanup(fn)", "fn"), f)));
        assert!(exprs.contains(&(at(src, "[[gnu::cleanup(fn)", "fn"), f)));
    });
}

/// Neither a reference nor a dependent name at `word` of `ctx`.
fn nothing_at(u: &Unit<'_>, src: &str, ctx: &str, word: &str) {
    let want = at(src, ctx, word);
    let refs: Vec<_> = written_refs(u).into_iter().filter(|r| bytes(u, r.name) == want).collect();
    assert!(refs.is_empty(), "`{word}` of `{ctx}` is recorded: turn this test around: {refs:?}");
    let deps: Vec<_> =
        dependent_names(u).into_iter().filter(|d| bytes(u, d.name) == want).collect();
    assert!(deps.is_empty(), "`{word}` of `{ctx}` is a dependent name: {deps:?}");
}

/// The written references at `word` of `ctx`: role and referent's kind.
fn refs_at(u: &Unit<'_>, src: &str, ctx: &str, word: &str) -> Vec<(Role, DeclKind)> {
    let want = at(src, ctx, word);
    written_refs(u)
        .into_iter()
        .filter(|r| bytes(u, r.name) == want)
        .map(|r| (r.role, u.ast().decls.get(r.referent).kind()))
        .collect()
}

// The rows neither table served (`spec/landed/edits/WP-067.md`, the
// rename's `unreachable`), each pinned: a test fails, and is turned
// around, the day Kiln records the name. Kiln's written-reference table
// stage 3 (WP-1932) and WP-2470 recorded most of them: those tests are
// `recorded_…` now; what the rename does with them is WP-138's
// (`spec/landed/edits/WP-066.md`, "For WP-138").

#[test]
fn unreachable_overloaded_operator_used_as_an_operator() {
    let src = "struct S { S operator+(S) const; };\nS k(S s) { return s + s; }\n";
    with_unit("unreach-op", src, |u| {
        nothing_at(u, src, "s + s", "+");
        for r in written_refs(u) {
            assert_ne!(u.ast().decls.get(r.referent).name().kind(), NameKind::Operator, "{r:?}");
        }
    });
}

#[test]
fn recorded_offsetof_member_designator() {
    let src = "struct S { int m; };\nint o = __builtin_offsetof(S, m);\n";
    with_unit("unreach-offsetof", src, |u| {
        assert_eq!(refs_at(u, src, "S, m)", "m"), [(Role::Designator, DeclKind::Field)]);
        let s = decl_at(u, at(src, "struct S {", "S").0);
        assert!(refs_of(u, Role::Type).contains(&(at(src, "(S, m)", "S"), s)));
    });
}

#[test]
fn recorded_init_capture_declared_name() {
    let src = "void k() { int k1 = 1; auto l = [&k2 = k1]() { return k2; }; (void)l; }\n";
    with_unit("unreach-capture", src, |u| {
        assert_eq!(names_at(u, src, "&k2 =", "k2"), [(DeclNameKind::Declaration, DeclKind::Var)]);
        let r = written_refs(u);
        let k2 =
            r.iter().find(|x| bytes(u, x.name) == at(src, "return k2", "k2")).unwrap().referent;
        assert_eq!(row(u, k2), Some(DeclNameKind::Declaration));
    });
}

#[test]
fn recorded_designator() {
    let src = "struct P { int x; };\nP p{.x = 1};\n";
    with_unit("unreach-designator", src, |u| {
        assert_eq!(refs_at(u, src, ".x = 1", "x"), [(Role::Designator, DeclKind::Field)]);
    });
}

#[test]
fn unreachable_name_in_a_string_attribute_argument() {
    let src = "extern \"C\" void f() {}\nextern \"C\" void g() __attribute__((alias(\"f\")));\n";
    with_unit("unreach-alias", src, |u| nothing_at(u, src, "alias(\"f\")", "f"));
}

#[test]
fn recorded_using_declaration_directive_and_alias_names() {
    let src = "\
namespace n { int v; }
using n::v;
using namespace n;
namespace na = n;
";
    with_unit("unreach-using", src, |u| {
        assert_eq!(refs_at(u, src, "using n::v", "v"), [(Role::UsingDecl, DeclKind::Var)]);
        let ns = [(Role::UsingDirective, DeclKind::Namespace)];
        assert_eq!(refs_at(u, src, "using namespace n", "n"), ns);
        let alias = [(Role::NamespaceAlias, DeclKind::Namespace)];
        assert_eq!(refs_at(u, src, "na = n", "n"), alias);
        let n = decl_at(u, at(src, "namespace n {", "n").0);
        assert!(
            refs_of(u, Role::Nns).contains(&(at(src, "using n::v", "n"), n)),
            "the qualifier is"
        );
    });
}

#[test]
fn recorded_friend_elaborated_reference() {
    let src = "struct S2 {};\nstruct T { friend struct S2; };\n";
    with_unit("unreach-friend", src, |u| {
        let got = refs_at(u, src, "friend struct S2", "S2");
        assert_eq!(got, [(Role::Elaborated, DeclKind::CXXRecord)]);
    });
}

#[test]
fn recorded_specialisation_target_as_a_reference() {
    let src = "\
template <class T> void t(T) {}
template <> void t<int>(int) {}
template <class T> struct H {};
template struct H<int>;
";
    with_unit("unreach-target", src, |u| {
        // One reference each, at the name of the explicit specialisation
        // or instantiation, unqualified (it begins at its name), whose
        // referent is the template declared on the line before.
        let ast = u.ast();
        let sm = u.sm();
        for (ctx, word, decl, kind) in [
            ("void t<int>", "t", "template <class T> void t", DeclKind::FunctionTemplate),
            ("struct H<int>;", "H", "template <class T> struct H", DeclKind::ClassTemplate),
        ] {
            let want = at(src, ctx, word);
            let found: Vec<_> =
                written_refs(u).into_iter().filter(|r| bytes(u, r.name) == want).collect();
            assert_eq!(found.len(), 1, "{word}: {found:?}");
            let r = &found[0];
            assert_eq!(r.role, Role::SpecTarget, "{word}");
            assert_eq!(
                sm.offset_of(sm.spelling_loc(r.first)).map(|(_, o)| o as usize),
                Some(want.0)
            );
            assert!(!r.global, "{word}");
            let n = ast.decls.get(r.referent);
            assert_eq!(n.kind(), kind, "{word}");
            let (_, lo) = sm.offset_of(sm.spelling_loc(n.loc())).expect("the template's place");
            let line = src.find(decl).expect("the template's line");
            assert!(
                (line..line + decl.len()).contains(&(lo as usize)),
                "{word}: the referent is the template at {lo}"
            );
        }
    });
}

#[test]
fn recorded_concept_in_a_type_constraint() {
    let src = "\
template <class T> concept Co = true;
template <Co T> void tc(T) {}
void ac(Co auto x) { (void)x; }
";
    with_unit("unreach-concept", src, |u| {
        let co = [(Role::Concept, DeclKind::Concept)];
        assert_eq!(refs_at(u, src, "<Co T>", "Co"), co);
        assert_eq!(refs_at(u, src, "(Co auto", "Co"), co);
    });
}

#[test]
fn unreachable_dependent_name_per_specialisation() {
    let src = "void f(int);\ntemplate <class T> void g(T x) { f(x); }\nvoid k() { g(1); }\n";
    with_unit("unreach-dependent", src, |u| {
        let want = at(src, "f(x)", "f");
        assert!(
            written_refs(u).iter().all(|r| bytes(u, r.name) != want),
            "a referent per specialisation"
        );
        assert_eq!(dependent_names(u).iter().filter(|d| bytes(u, d.name) == want).count(), 1);
    });
}

// Kiln finding K3, fixed by Kiln WP-2470: a later construct that Kiln
// resolves to an existing declaration overwrote that declaration's row,
// against Kiln `spec/ast.md` §19.1, and the earlier name was lost. Each
// written declaration has its row now (`DeclExtents::rows_of`): the
// earlier name is given; a friend naming a class declared before it
// declares nothing (WP-1932, a reference now), and a later definition's
// name is its own.
#[test]
fn s9_3_a_later_construct_keeps_a_declarations_row() {
    let src = "\
struct S;
struct T { friend struct S; };
class K; class K;
struct U { friend class K; };
template <class X> struct Q;
template <> struct Q<int>;
template <> struct Q<int> {};
";
    with_unit("k3", src, |u| {
        let decl_loc = |ctx: &str, word: &str| -> usize {
            let names: Vec<DeclName> = decl_names(u)
                .into_iter()
                .filter(|n| bytes(u, n.name) == at(src, ctx, word))
                .collect();
            assert_eq!(names.len(), 1, "`{ctx}`: {names:?}");
            off(u, u.ast().decls.get(names[0].decl).loc())
        };
        let kept = |ctx: &str, word: &str| {
            assert!(!names_at(u, src, ctx, word).is_empty(), "`{ctx}` has no row");
        };
        // A friend naming a class declared before it declares nothing new
        // (Kiln `spec/ast.md` §19.5): no declaration name, a written
        // reference of the role `Elaborated` to the class.
        let named = |ctx: &str, word: &str| {
            assert!(names_at(u, src, ctx, word).is_empty(), "`{ctx}` declares");
            assert_eq!(refs_at(u, src, ctx, word), [(Role::Elaborated, DeclKind::CXXRecord)]);
        };
        kept("struct S;\n", "S");
        named("friend struct S", "S");
        kept("; class K;\nstruct U", "K");
        named("friend class K", "K");
        kept("template <> struct Q<int>;", "Q");
        assert_eq!(
            decl_loc("template <> struct Q<int> {}", "Q"),
            at(src, "template <> struct Q<int>;", "Q").0
        );
    });
}

// Review item 8: `written` changes nothing the digest reads, on a unit
// with standard headers, macros, templates, default arguments and an
// inheriting constructor: the declaration arena and the shard are the
// same (`crates/slip_digest/tests/written_on_off.rs` compares the digest).
#[test]
fn s8_4_written_changes_nothing_the_digest_reads() {
    let _l = lock();
    let dir = tree("on-off-std");
    std::fs::write(dir.join("h.h"), ON_OFF_HEADER).unwrap();
    std::fs::write(dir.join("main.cpp"), ON_OFF_MAIN).unwrap();
    let argv: Vec<String> =
        ["clang++", "-std=c++20", "main.cpp"].iter().map(|s| (*s).to_owned()).collect();
    let o = observer(&dir, &argv);
    let look = |u: &Unit<'_>| (u.ast().decls.len(), u.ast().types.len(), recorded(u));
    let (off_shard, off) = parse_observer_with(&o, Phase::Map, 1, options(false), look).unwrap();
    let (on_shard, on) = parse_observer_with(&o, Phase::Map, 1, options(true), look).unwrap();
    assert_eq!(off_shard.outcome.status, Status::Ok, "{:?}", off_shard.outcome);
    let (off, on) = (off.unwrap(), on.unwrap());
    assert!(!off.2 && on.2);
    assert_eq!((off.0, off.1), (on.0, on.1), "declaration and type arenas");
    assert!(off.0 > 10_000, "the standard headers were parsed: {}", off.0);
    assert_eq!(on_shard.to_bytes(), off_shard.to_bytes());
    let _ = std::fs::remove_dir_all(&dir);
}

const ON_OFF_HEADER: &str = "\
#pragma once
#define M(x) x + 1
template <class T> struct B { T t; int dm = 3; B() {} B(int) {} void m(int y = 2) {} };
struct Base { Base(int); };
struct Der : Base { using Base::Base; int k = M(2); };
";
const ON_OFF_MAIN: &str = "\
#include \"h.h\"
#include <vector>
#include <string>
namespace n { struct S { int a; }; void f(S); }
template <class T> void g(T x) { f(x); }
int main() { B<int> b; b.m(); B<long> c(1); n::S s{1}; g(s); std::vector<std::string> v; \
v.push_back(\"x\"); Der d(1); return M(b.t); }
";

// Review item 8: a command that already carries the words. The front end
// takes a word twice; with `written` off the command's own words still
// record (they are the command's), and the lists are the same.
#[test]
fn s8_4_command_already_carrying_the_words() {
    let _l = lock();
    let dir = tree("carrying");
    std::fs::write(dir.join("main.cpp"), CALLS).unwrap();
    let run = |extra: &[&str], written: bool| {
        let mut argv: Vec<String> = vec!["clang++".into(), "-std=c++20".into()];
        argv.extend(extra.iter().map(|s| (*s).to_owned()));
        argv.push("main.cpp".into());
        let look = |u: &Unit<'_>| (recorded(u), written_refs(u), decl_names(u).len());
        let (shard, got) =
            parse_observer_with(&observer(&dir, &argv), Phase::Map, 1, options(written), look)
                .unwrap();
        assert_eq!(shard.outcome.status, Status::Ok, "{extra:?}: {:?}", shard.outcome);
        let (rec, refs, names) = got.unwrap();
        (rec, refs.len(), names)
    };
    let plain = run(&[], true);
    assert!(plain.0 && plain.1 > 0 && plain.2 > 0);
    let both = ["-fkiln-dump-written-refs", "-fkiln-decl-extents"];
    assert_eq!(run(&both, true), plain);
    assert_eq!(run(&both, false), plain);
    assert_eq!(run(&both[..1], true), plain);
    assert_eq!(run(&both[1..], true), plain);
    // One word only, `written` off: one table only, so not `recorded`.
    let refs_only = run(&both[..1], false);
    assert_eq!(refs_only, (false, plain.1, 0));
    let _ = std::fs::remove_dir_all(&dir);
}

// WP-071 review, A1: a partial specialisation of a class or variable
// template (in its namespace or out of line) has the template as its
// origin: its name token spells the template.
#[test]
fn s5_5_origin_of_a_partial_specialisation_is_its_template() {
    let src = "\
namespace n {
template <class T> struct S {};
template <class T> struct S<T*> {};
template <class T> constexpr int v = 1;
template <class T> constexpr int v<T*> = 2;
}
template <class T> constexpr int n::v<T&> = 4;
";
    with_unit("origin-partial", src, |u| {
        let s = decl_at(u, at(src, "struct S {}", "S").0);
        let v = decl_at(u, at(src, "int v = 1", "v").0);
        for (ctx, word, want) in
            [("struct S<T*>", "S", s), ("int v<T*>", "v", v), ("int n::v<T&>", "v", v)]
        {
            let d = decl_at(u, at(src, ctx, word).0);
            assert_ne!(d, want, "`{ctx}` is a declaration of its own");
            assert_eq!(origin(u, d), want, "`{ctx}`");
        }
    });
}

// WP-071 review, A4: a friend declaration that does not define the
// function is not followed to the pattern's (`spec/digest.md` §3.4), so a
// call's origin is the function's, instantiated or not; a friend defined
// in the template is followed.
#[test]
fn s5_5_origin_does_not_follow_a_friend_that_does_not_define() {
    for (name, instantiate) in [("origin-friend-a", "n::A<int> a;\n"), ("origin-friend-b", "")] {
        let src = format!(
            "namespace n {{\nvoid f(int);\ntemplate <class T> struct A {{ friend void f(int); T x{{}}; }};\nvoid f(int) {{}}\n}}\n{instantiate}void h() {{ n::f(1); }}\n"
        );
        with_unit(name, &src, |u| {
            let r = written_refs(u);
            let call =
                r.iter().find(|x| bytes(u, x.name) == at(&src, "n::f(1)", "f")).expect("the call");
            let f = decl_at(u, at(&src, "void f(int);\ntemplate", "f").0);
            let o = origin(u, call.referent);
            assert_eq!(u.ast().first(o), u.ast().first(f), "{name}: the origin is the function's");
        });
    }
    let src = "\
namespace n {
void f(int);
template <class T> struct A { friend void f(int) {} T x{}; };
}
n::A<int> a;
void h() { n::f(1); }
";
    with_unit("origin-friend-defined", src, |u| {
        let r = written_refs(u);
        let call =
            r.iter().find(|x| bytes(u, x.name) == at(src, "n::f(1)", "f")).expect("the call");
        let friend = decl_at(u, at(src, "friend void f(int) {}", "f").0);
        assert_eq!(origin(u, call.referent), friend, "the defined friend is followed");
    });
}
