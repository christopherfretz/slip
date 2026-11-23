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
use slip_kiln::kiln::ast::{DeclId, QualType};
use slip_kiln::kiln::source::{Loc, Span};
use slip_kiln::written::{
    DeclName, DeclNameKind, Role, decl_names, dependent_names, origin, recorded, written_refs,
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

fn options(written: bool) -> ParseOptions {
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
    // `using A::A;`: the base, by the constructor its first shadow names.
    if ast.decls.get(d).kind() == DeclKind::Using && name.kind() == NameKind::Constructor {
        let shadow = ast.using_shadows(d).next()?;
        return ident_of(u, ast.using_target(shadow)?);
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

// The roles of Kiln's stage 3 (WP-1932) are not recorded at this pin: a
// using-declaration's name, a using-directive's namespace, a namespace
// alias's target, an elaborated or friend reference, a specialisation's
// target, a designator. Their qualifiers are (`spec/edits.md` §9.3: the
// rows neither table serves).
#[test]
fn kiln_hole_stage_3_roles_are_not_recorded() {
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
        let roles: Vec<Role> = written_refs(u).iter().map(|r| r.role).collect();
        for r in [
            Role::UsingDecl,
            Role::UsingDirective,
            Role::NamespaceAlias,
            Role::Elaborated,
            Role::SpecTarget,
            Role::Designator,
        ] {
            assert!(!roles.contains(&r), "{r:?} is recorded: turn this test around");
        }
        let x = decl_at(u, at(src, "int x;", "x").0);
        assert!(written_refs(u).iter().all(|r| r.referent != x), "the designator `.x`");
        let fs: Vec<_> = written_refs(u)
            .iter()
            .filter(|r| bytes(u, r.name) == at(src, "using n::f", "f"))
            .map(|r| r.role)
            .collect();
        assert!(fs.is_empty(), "the using-declaration's name: {fs:?}");
        // Its qualifier is a reference.
        let n = decl_at(u, at(src, "namespace n {", "n").0);
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
        // A destructor, a conversion function and an operator: absent.
        let all = decl_names(u);
        let kinds: Vec<DeclKind> = all.iter().map(|n| u.ast().decls.get(n.decl).kind()).collect();
        assert!(!kinds.contains(&DeclKind::CXXDestructor), "{kinds:?}");
        assert!(!kinds.contains(&DeclKind::CXXConversion), "{kinds:?}");
        assert!(!kinds.contains(&DeclKind::Friend), "{kinds:?}");
        assert!(names_at(u, s, "operator==(const", "operator").is_empty());
        assert!(names_at(u, s, "~C()", "C").is_empty());
        // Every name is its declaration's identifier, byte for byte.
        for n in &all {
            let (lo, hi) = bytes(u, n.name);
            assert_eq!(Some(s[lo..hi].to_owned()), ident_of(u, n.decl), "{n:?}");
        }
    });
}

// Kiln finding: an explicit instantiation of a function template or of a
// variable template has no row in the declaration-extent table (only its
// parameters do), although Kiln `spec/ast.md` §19.1 says an explicit
// instantiation is a written declaration with the specialisation's row;
// one of a class template has its row.
#[test]
fn kiln_hole_explicit_instantiation_of_function_and_variable_has_no_extent() {
    let src = "\
template <class T> void t(T) {}
template <class T> T vt = T();
template void t<double>(double);
extern template void t<long>(long);
template long vt<long>;
";
    with_unit("inst-hole", src, |u| {
        for (ctx, word) in [
            ("template void t<double>", "t"),
            ("extern template void t<long>", "t"),
            ("template long vt<long>", "vt"),
        ] {
            assert!(
                names_at(u, src, ctx, word).is_empty(),
                "`{ctx}` has a row: turn this test around"
            );
        }
        let rows: Vec<DeclKind> =
            u.ast().decl_extents.iter().map(|(d, _)| u.ast().decls.get(d).kind()).collect();
        assert_eq!(
            rows.iter()
                .filter(|k| !matches!(k, DeclKind::ParmVar | DeclKind::TemplateTypeParm))
                .count(),
            4,
            "the two templates and their patterns only: {rows:?}"
        );
    });
}

// Kiln finding: two explicit instantiations of one specialisation (a
// declaration, then a definition) are two written declarations, but the
// table keys a row by the specialisation and the second replaces the
// first (`DeclExtents::set`), so the first one's name is lost.
#[test]
fn kiln_hole_two_explicit_instantiations_of_one_specialisation_are_one_row() {
    let src = "\
template <class T> struct S { void m() {} };
extern template struct S<int>;
template struct S<int>;
";
    with_unit("inst-twice", src, |u| {
        let inst = [(DeclNameKind::ExplicitInstantiation, DeclKind::CXXRecord)];
        assert_eq!(names_at(u, src, ";\ntemplate struct S<int>;", "S"), inst);
        assert!(
            names_at(u, src, "extern template struct S<int>", "S").is_empty(),
            "the declaration has its row: turn this test around"
        );
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
const EXPLAINED: &[&str] = &["operator-function-id (checked: starts with `operator`)"];

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
            if matches!(kind, DeclKind::Friend | DeclKind::FriendTemplate) {
                t.failures.push(format!("{label}: a {kind:?} row is a declaration name"));
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
            } else if ident_of(u, d).is_none()
                || matches!(kind, DeclKind::CXXDestructor | DeclKind::CXXConversion)
            {
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
    let _ = std::fs::remove_dir_all(&root);
    assert!(total.sides > 3000, "the sweep found the pair tests");
    assert!(total.checked > 10_000, "the sweep checked references");
    assert!(
        total.failures.is_empty(),
        "{} failures:\n{}",
        total.failures.len(),
        total.failures.join("\n")
    );
}
