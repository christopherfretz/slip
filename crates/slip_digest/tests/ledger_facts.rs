//! The facts of Kiln's written-reference table (Kiln `spec/ast.md` §18,
//! at the pin) that the ledger of written names leans on (work package
//! WP-156, fix round 3, commit 1; `docs/wp/WP-156.md` "Tests the
//! implementer adds", KT1 2.9). Each probe parses a shape of the third
//! review (A1 to A5, X1 to X8, W3 to W10, `at<3>`, `Q3<int, int>::s`, a
//! thread-safety attribute) with the table recorded, and checks that the
//! entry of `make` (or of the type or template name) exists, names the
//! entity expected and is held by the declaration expected (its
//! `enclosing`: the unit is the entity at or above it). The joins: a
//! walked `DeclRefExpr` and `MemberExpr`, an operator call's callee and a
//! literal operator's callee find their entry by `name_lo` and entity, in
//! a macro's body and argument too (§18.11.1, §18.13.7).
//!
//! Nothing here reads the digest: the facts are Kiln's.

mod common;

use slip_kiln::ParseOptions;
use slip_kiln::kiln::ast::expr::ExprKind;
use slip_kiln::kiln::ast::{Ast, DeclId};
use slip_kiln::kiln::source::{FileId, SourceManager};
use slip_kiln::tree::Tree;

/// The names every shape uses: `make(x)` resolves to `lib::make` by
/// argument-dependent lookup, the template `::make` is the other
/// candidate a rebinding would pick.
const PRE: &str = "namespace lib { struct X {}; int make(X); }
template <class T> int make(T);
lib::X x;
template <class T> struct Wr { T m; };
template <class T, class U> concept Same = __is_same(T, U);
";

const MAKE: &str = "Function lib::make(lib::X)";
const TU: &str = "TranslationUnit (unnamed)";

/// One entry, as text: its role, its entity and its holder (each
/// `kind name`), whether its name is in a macro, its line.
#[derive(Debug, Clone)]
struct Row {
    role: String,
    entity: String,
    holder: String,
    flags: u32,
    macro_loc: bool,
    line: u32,
}

/// A walked name expression and the role of the entry it joins.
#[derive(Debug, Clone)]
struct Join {
    kind: ExprKind,
    decl: String,
    macro_loc: bool,
    role: Option<String>,
}

#[derive(Debug)]
struct Facts {
    rows: Vec<Row>,
    joins: Vec<Join>,
    /// The first line of the shape's own text, after `PRE`.
    body: u32,
}

impl Facts {
    /// The entries of role `role` naming `entity`.
    fn of(&self, role: &str, entity: &str) -> Vec<&Row> {
        self.rows.iter().filter(|r| r.role == role && r.entity == entity).collect()
    }

    /// Exactly the holders `holders`, in table order, for the entries of
    /// role `role` naming `entity`.
    fn held(&self, role: &str, entity: &str, holders: &[&str]) {
        let got: Vec<&str> = self.of(role, entity).iter().map(|r| r.holder.as_str()).collect();
        assert_eq!(got, holders, "{role} entries of {entity}: {self:#?}");
    }

    /// The index in table order of the first entry of `role` naming
    /// `entity`.
    fn index(&self, role: &str, entity: &str) -> usize {
        self.rows
            .iter()
            .position(|r| r.role == role && r.entity == entity)
            .unwrap_or_else(|| panic!("no {role} entry of {entity}: {self:#?}"))
    }
}

fn name(ast: &Ast, d: Option<DeclId>) -> String {
    match d {
        None => "-".into(),
        Some(d) => format!("{:?} {}", ast.decls.get(d).kind(), slip_digest::display_name(ast, d)),
    }
}

fn facts_of(ast: &Ast, sm: &SourceManager, main: FileId, body: u32) -> Facts {
    let refs = &ast.written_refs;
    assert!(refs.is_on(), "the table is recorded");
    assert!(refs.unrecorded().is_empty(), "nothing unrecorded");
    let rows = refs
        .iter()
        .map(|(id, r)| Row {
            role: format!("{:?}", r.role),
            entity: name(ast, r.entity),
            holder: name(ast, r.enclosing),
            flags: u32::from(r.flags),
            macro_loc: sm.is_macro_loc(refs.name_lo(id)),
            line: sm.line_col(refs.name_lo(id)).map_or(0, |l| l.line),
        })
        .collect();
    let options = slip_kiln::kiln::ast::dump::DumpOptions {
        sources: Some(sm),
        main_file: Some(main),
        ..Default::default()
    };
    let mut walked = Vec::new();
    slip_kiln::kiln::ast::dump::for_each_written_expr(ast, &options, |e, _| walked.push(e));
    walked.sort_unstable();
    walked.dedup();
    let mut joins = Vec::new();
    for e in walked {
        let n = ast.exprs.get(e);
        if !matches!(n.kind(), ExprKind::DeclRefExpr | ExprKind::MemberExpr) {
            continue;
        }
        let d = ast.exprs.decl(e);
        let role = refs
            .iter()
            .find(|(id, r)| refs.name_lo(*id) == n.loc() && r.entity == Some(d))
            .map(|(_, r)| format!("{:?}", r.role));
        joins.push(Join {
            kind: n.kind(),
            decl: name(ast, Some(d)),
            macro_loc: sm.is_macro_loc(n.loc()),
            role,
        });
    }
    Facts { rows, joins, body }
}

/// The table of `PRE` then `body`, parsed under `std` (C++17 and later).
fn facts(body: &str, std: &str) -> Facts {
    let src = format!("{PRE}{body}");
    let files = [("main.cpp", src.as_str())];
    let snippet =
        slip_kiln::snippet::Snippet { files: &files, main: "main.cpp", include: &[""], std };
    let extra = slip_kiln::snippet::Extra::default();
    let (f, errors, diagnostics) = slip_kiln::snippet::parse_with_tables(&snippet, &extra, |p| {
        (facts_of(p.ast, p.sm, p.main_file, 6), p.errors, p.diagnostics.clone())
    })
    .expect("the snippet parses");
    assert_eq!(errors, 0, "{diagnostics}");
    f
}

fn cxx20(body: &str) -> Facts {
    facts(body, "c++20")
}

/// Every entry of `f`'s own text that names `make` is held by `holder`, and
/// there is at least one.
fn make_held_by(f: &Facts, holder: &str) {
    let rows = f.of("Expr", MAKE);
    let past: Vec<&&Row> = rows.iter().filter(|r| r.line >= f.body).collect();
    assert!(!past.is_empty(), "an entry of make: {f:#?}");
    for r in past {
        assert_eq!(r.holder, holder, "make's holder: {f:#?}");
        assert_eq!(r.flags & 3, 0, "make is neither dependent nor unresolved: {r:?}");
    }
}

// A1: a name in an explicit instantiation's `noexcept`, held by the
// context the qualifier enters (§18.3 "Not holders").

#[test]
fn a1_c01_instantiation_directive_noexcept() {
    let f = cxx20(
        "template <class T> auto ga(T) noexcept(true) { return 1; }\n\
         template auto ga<int>(int) noexcept(sizeof(make(x)) == 4);\n",
    );
    make_held_by(&f, TU);
    f.held("SpecTarget", "FunctionTemplate ga", &[TU]);
}

#[test]
fn a1_c02_extern_directive_noexcept() {
    let f = cxx20(
        "template <class T> auto gb(T) noexcept(true) { return 1; }\n\
         extern template auto gb<int>(int) noexcept(sizeof(make(x)) == 4);\n\
         int ub = gb(0);\n",
    );
    make_held_by(&f, TU);
}

#[test]
fn a1_c03_decltype_auto_directive_noexcept() {
    let f = cxx20(
        "template <class T> decltype(auto) gc(T) noexcept(true) { return 1; }\n\
         template decltype(auto) gc<int>(int) noexcept(sizeof(make(x)) == 4);\n",
    );
    make_held_by(&f, TU);
}

/// The class the qualifier enters is the implicit instantiation
/// `GM<int>`: `enclosing` names a declaration of an instantiation here
/// (KT1 2.9.4 as worded does not hold; §18.3 says so).
#[test]
fn a1_c27_member_directive_noexcept() {
    let f = cxx20(
        "template <class T> struct GM { auto f() noexcept(true) { return 1; } };\n\
         template auto GM<int>::f() noexcept(sizeof(make(x)) == 4);\n",
    );
    make_held_by(&f, "CXXRecord GM<int>");
    f.held("Nns", "ClassTemplate GM", &[TU]);
    f.held("SpecTarget", "CXXMethod GM<int>::f()", &[TU]);
}

#[test]
fn a1_directive_in_a_namespace() {
    let f = cxx20(
        "namespace acme {\n\
         template <class T> auto ga(T) noexcept(true) { return 1; }\n\
         template auto ga<int>(int) noexcept(sizeof(make(x)) == 4);\n}\n",
    );
    make_held_by(&f, "Namespace acme");
}

/// A2: a friend specialisation's `noexcept` (late-parsed) is held by the
/// class granting friendship; its declarator's name by the friend.
#[test]
fn a2_c14_friend_specialization_noexcept() {
    let f = cxx20(
        "template <class T> void ftn(T) noexcept(true) {}\n\
         struct FN { friend void ftn<>(int) noexcept(sizeof(make(x)) == 4); };\n",
    );
    make_held_by(&f, "CXXRecord FN");
    f.held("Elaborated", "FunctionTemplate ftn", &["Friend FN::(unnamed)"]);
}

// A3: a `_BitInt` width in a macro, held by the function.

#[test]
fn a3_c37_bit_int_cast_in_a_macro() {
    let f = cxx20(
        "#define CAST(w, v) ((_BitInt(w))(v))\n\
         int c37() { return (int)CAST(sizeof(make(x)) * 8, 1); }\n",
    );
    make_held_by(&f, "Function c37()");
    assert!(f.of("Expr", MAKE).iter().any(|r| r.macro_loc), "{f:#?}");
}

#[test]
fn a3_c38_bit_int_sizeof_in_a_macro() {
    let f = cxx20(
        "#define SZ(w) sizeof(_BitInt(w))\n\
         unsigned long c38() { return SZ(sizeof(make(x)) * 8); }\n",
    );
    make_held_by(&f, "Function c38()");
}

/// A3's other half and the joins: names from a macro's argument and body
/// that a stream walks join their entries, as do a member access, an
/// operator call's callee and a literal operator's callee.
#[test]
fn a3_walked_names_in_macros_join() {
    let f = cxx20(
        "#define CALL(a) make(a)\n#define CALLX make(x)\n\
         int m1() { return CALL(x) + CALLX; }\n\
         struct SS { int mm; int operator+(int) const; };\n\
         #define MEM(s) s.mm\n#define PLUS(s) (s + 1)\n\
         int m2(SS s) { return MEM(s) + PLUS(s); }\n\
         int operator\"\"_k(unsigned long long);\n#define LIT 12_k\n\
         int m3() { return LIT; }\n",
    );
    make_held_by(&f, "Function m1()");
    let joined = |kind: ExprKind, decl: &str, role: &str| {
        let j: Vec<&Join> = f.joins.iter().filter(|j| j.kind == kind && j.decl == decl).collect();
        assert!(!j.is_empty(), "walked {kind:?} {decl}: {f:#?}");
        for j in j {
            assert!(j.macro_loc, "{j:?} is in a macro");
            assert_eq!(j.role.as_deref(), Some(role), "{j:?} joins its entry");
        }
    };
    joined(ExprKind::DeclRefExpr, MAKE, "Expr");
    joined(ExprKind::DeclRefExpr, "Var x", "Expr");
    joined(ExprKind::MemberExpr, "Field SS::mm", "Member");
    joined(ExprKind::DeclRefExpr, "CXXMethod SS::operator+(int) const", "Operator");
    joined(ExprKind::DeclRefExpr, "Function operator\"\"_k(unsigned long long)", "UdSuffix");
}

/// The same joins outside a macro: every walked name expression of the
/// shape joins.
#[test]
fn walked_names_join() {
    let f = cxx20(
        "struct SS { int mm; int operator+(int) const; };\n\
         int operator\"\"_k(unsigned long long);\n\
         int o1(SS s) { return s + 1 + 12_k + s.mm + make(x); }\n",
    );
    let roles: Vec<(&str, Option<&str>)> =
        f.joins.iter().map(|j| (j.decl.as_str(), j.role.as_deref())).collect();
    for want in [
        ("ParmVar s", Some("Expr")),
        ("CXXMethod SS::operator+(int) const", Some("Operator")),
        ("Function operator\"\"_k(unsigned long long)", Some("UdSuffix")),
        ("Field SS::mm", Some("Member")),
        (MAKE, Some("Expr")),
    ] {
        assert!(roles.contains(&want), "{want:?}: {f:#?}");
    }
    assert!(f.joins.iter().all(|j| j.role.is_some()), "every walked name joins: {f:#?}");
}

// A4: an attribute's argument, held by the declaration.

#[test]
fn a4_c41_vector_size() {
    let f = cxx20("typedef int v4 __attribute__((vector_size(sizeof(make(x)) * 4)));\nv4 vv;\n");
    make_held_by(&f, "Typedef v4");
}

#[test]
fn a4_c44_ext_vector_type() {
    let f =
        cxx20("using v8 = int __attribute__((ext_vector_type(sizeof(make(x)) * 2)));\nv8 ev;\n");
    make_held_by(&f, "TypeAlias v8");
}

/// A5: a dynamic exception specification's type under C++14, held by the
/// function. The snippet front end takes C++17 and later, so this is the
/// production parse of a tree under the work root.
#[test]
fn a5_dynamic_exception_specification_cxx14() {
    let src = format!(
        "{}void fdyn() throw(Wr<decltype(make(x))>);\n",
        PRE.replace("template <class T, class U> concept Same = __is_same(T, U);\n", "")
    );
    let tree = Tree::new(&common::work_root(), "wp156-ledger-a5", &[("main.cpp", src.as_bytes())])
        .expect("the tree");
    let observer = tree.observer("main.cpp", "c++14", &[]);
    let mut options = ParseOptions::default();
    options.written = true;
    let (_shard, got) = tree
        .parse(&observer, &options, |unit| {
            facts_of(unit.ast(), unit.sm(), unit.main_file().expect("a main file"), 5)
        })
        .expect("the unit parses");
    let f = got.expect("the closure ran");
    make_held_by(&f, "Function fdyn()");
    f.held("Template", "ClassTemplate Wr", &["Function fdyn()"]);
}

// X1 to X8.

#[test]
fn x1_friend_declarator_qualifier() {
    let f = cxx20(
        "template <int N> struct Q { static int f(); };\n\
         struct F { friend int Q<sizeof(make(x))>::f(); };\n",
    );
    make_held_by(&f, "Friend F::(unnamed)");
    f.held("Nns", "ClassTemplate Q", &["Friend F::(unnamed)"]);
}

/// X2: a destructor's template-id; the destructor's own `Member` entry
/// (at the `~`) joins the walked member expression.
#[test]
fn x2_destructor_template_id() {
    let f = cxx20("void d10() { Wr<int> wv{0}; wv.~Wr<decltype(make(x))>(); }\n");
    make_held_by(&f, "Function d10()");
    let j = f.joins.iter().find(|j| j.kind == ExprKind::MemberExpr).expect("the member");
    assert_eq!(j.role.as_deref(), Some("Member"), "{f:#?}");
}

/// G01's c10: a destructor named by `decltype`; its operand's names have
/// entries held by the function.
#[test]
fn g01_c10_destructor_decltype() {
    let f = cxx20("void d10() { Wr<int> wv{0}; wv.~decltype(Wr<int>{make(x)})(); }\n");
    make_held_by(&f, "Function d10()");
}

#[test]
fn x3_function_directive_noexcept() {
    let f = cxx20(
        "template <class T> void g(T) noexcept(true) {}\n\
         template void g<int>(int) noexcept(sizeof(make(x)) == 4);\n",
    );
    make_held_by(&f, TU);
}

#[test]
fn x7_friend_specialization_argument() {
    let f = cxx20(
        "template <int N> void ftf() {}\nstruct F7 { friend void ftf<sizeof(make(x))>(); };\n",
    );
    make_held_by(&f, "Friend F7::(unnamed)");
    f.held("Elaborated", "FunctionTemplate ftf", &["Friend F7::(unnamed)"]);
}

#[test]
fn x8_bit_int_width() {
    let f = cxx20("_BitInt(sizeof(make(x)) * 8) bi = 0;\n");
    make_held_by(&f, "Var bi");
}

// W3, W10 and T4: a type-constraint's entry has role `Concept`, is held
// by the declaration, and comes after the entries of its arguments.

#[test]
fn w3_constrained_placeholder_variable() {
    let f = cxx20("Same<decltype(make(x))> auto v = 1;\n");
    make_held_by(&f, "Var v");
    f.held("Concept", "Concept Same", &["Var v"]);
    assert!(f.index("Expr", MAKE) < f.index("Concept", "Concept Same"), "{f:#?}");
}

#[test]
fn w10_constrained_placeholder_return() {
    let f = cxx20("Same<decltype(make(x))> auto g() { return 1; }\n");
    make_held_by(&f, "Function g()");
    f.held("Concept", "Concept Same", &["Function g()"]);
    assert!(f.index("Expr", MAKE) < f.index("Concept", "Concept Same"), "{f:#?}");
}

#[test]
fn t4_constraint_type_argument() {
    let f = cxx20(
        "struct X1 { int a; };\n\
         template <class T, class U> concept C = sizeof(T) == sizeof(U);\n\
         C<X1> auto v = 1;\n",
    );
    let at = f.index("Type", "CXXRecord X1");
    assert_eq!(f.rows[at].holder, "Var v", "{f:#?}");
    f.held("Concept", "Concept C", &["Var v"]);
    assert!(at < f.index("Concept", "Concept C"), "{f:#?}");
}

// W7, `Q3<int, int>::s` and T2: a declarator qualifier's entries are held
// by the declaration and recorded before its `Redecl` entry.

#[test]
fn w7_specialized_member_qualifier() {
    let f = cxx20(
        "template <int N> struct Q { static int s; };\n\
         template <int N> int Q<N>::s = 1;\n\
         template <> int Q<sizeof(make(x))>::s = 5;\n",
    );
    make_held_by(&f, "Var Q<4>::s");
    let redecl = f.index("Redecl", "Var Q<4>::s");
    assert!(f.index("Expr", MAKE) < redecl, "{f:#?}");
    assert_eq!(f.rows[redecl].holder, "Var Q<4>::s");
    assert!(
        f.rows[..redecl]
            .iter()
            .any(|r| r.role == "Nns" && r.entity == "ClassTemplate Q" && r.holder == "Var Q<4>::s"),
        "{f:#?}"
    );
}

#[test]
fn t2_qualifier_type_argument() {
    let f = cxx20(
        "struct X1 { int a; };\ntemplate <int N> struct Q { static int s; };\n\
         template <int N> int Q<N>::s = 1;\ntemplate <> int Q<sizeof(X1)>::s = 5;\n",
    );
    let redecl = f.index("Redecl", "Var Q<4>::s");
    let x1: Vec<usize> = (0..f.rows.len())
        .filter(|&i| f.rows[i].role == "Type" && f.rows[i].entity == "CXXRecord X1")
        .collect();
    assert_eq!(x1.len(), 1, "{f:#?}");
    assert!(x1[0] < redecl && f.rows[x1[0]].holder == "Var Q<4>::s", "{f:#?}");
}

/// The fix round 2 STOP (b): `template <> int Q3<int, int>::s = 1;`
/// writes its qualifier's entries, held by the specialisation; the body
/// that names `Q3<int, decltype(make(x))>::s` holds its own.
#[test]
fn q3_explicit_specialization_and_a_body() {
    let f = cxx20(
        "template <class T, class U> struct Q3 { static int s; };\n\
         template <class T, class U> int Q3<T, U>::s = 0;\n\
         template <> int Q3<int, int>::s = 1;\n\
         int f() { return Q3<int, decltype(make(x))>::s; }\n",
    );
    let redecl = f.index("Redecl", "Var Q3<int, int>::s");
    assert_eq!(f.rows[redecl].holder, "Var Q3<int, int>::s");
    assert!(
        f.rows[..redecl].iter().any(|r| r.role == "Nns" && r.holder == "Var Q3<int, int>::s"),
        "{f:#?}"
    );
    make_held_by(&f, "Function f()");
}

// W8, `at<3>` and T1: an explicit instantiation of a function is held by
// the translation unit: its target, and its arguments' names.

#[test]
fn w8_directive_argument() {
    let f = cxx20("template <int N> int ft() { return N; }\ntemplate int ft<sizeof(make(x))>();\n");
    make_held_by(&f, TU);
    f.held("SpecTarget", "FunctionTemplate ft", &[TU]);
}

/// The fix round 2 STOP (a): `at<3>` writes the entry of `at` alone;
/// `at<sizeof(make(x))>` adds `make`.
#[test]
fn at3_directive_with_and_without_a_name() {
    let f = cxx20(
        "template <int N> int at() { return N; }\n\
         extern template int at<3>();\n\
         template int at<sizeof(make(x))>();\n",
    );
    f.held("SpecTarget", "FunctionTemplate at", &[TU, TU]);
    let lines: Vec<u32> =
        f.of("Expr", MAKE).iter().map(|r| r.line).filter(|&l| l >= f.body).collect();
    assert_eq!(lines, [8], "make on the second directive's line only: {f:#?}");
    let one = cxx20("template <int N> int at() { return N; }\nextern template int at<3>();\n");
    let past: Vec<&Row> = one.rows.iter().filter(|r| r.line == 7).collect();
    assert_eq!(past.len(), 1, "at<3> writes one entry: {one:#?}");
    assert_eq!(past[0].role, "SpecTarget");
}

#[test]
fn t1_directive_type_argument() {
    let f = cxx20(
        "struct X1 { int a; };\ntemplate <int N> int at() { return N; }\n\
         template int at<sizeof(X1)>();\n",
    );
    let rows: Vec<&Row> = f.rows.iter().filter(|r| r.line == 8).collect();
    assert!(
        rows.iter().any(|r| r.role == "Type" && r.entity == "CXXRecord X1" && r.holder == TU),
        "{f:#?}"
    );
}

/// B2's cr05: a member's explicit instantiation writes its qualifier and
/// target, held by the translation unit.
#[test]
fn b2_cr05_member_directive() {
    let f = cxx20("template <int N> struct QI { void f() {} };\ntemplate void QI<3>::f();\n");
    f.held("Nns", "ClassTemplate QI", &[TU]);
    f.held("SpecTarget", "CXXMethod QI<3>::f()", &[TU]);
}

/// A thread-safety attribute's argument is an entry held by the
/// declaration that carries it.
#[test]
fn thread_safety_attribute() {
    let f = cxx20(
        "struct __attribute__((capability(\"mutex\"))) Mu {};\nMu mu;\n\
         int guarded __attribute__((guarded_by(mu)));\n\
         void lk() __attribute__((requires_capability(mu)));\n",
    );
    f.held("Expr", "Var mu", &["Var guarded", "Function lk()"]);
}

/// A declarator group's shared decl-specifier entries carry `REF_GROUP`
/// and are held by the first declaration (§18.3.1).
#[test]
fn declarator_group() {
    let f = cxx20("Wr<decltype(make(x))> ga1, ga2;\n");
    let rows = f.of("Expr", MAKE);
    let past: Vec<&&Row> = rows.iter().filter(|r| r.line >= f.body).collect();
    assert_eq!(past.len(), 1, "{f:#?}");
    assert_eq!(past[0].holder, "Var ga1");
    assert_ne!(past[0].flags & 32, 0, "REF_GROUP: {f:#?}");
}

/// A default argument's names are held by the parameter.
#[test]
fn default_argument() {
    let f = cxx20("void da(int a = make(x));\nint ua() { da(); return 0; }\n");
    make_held_by(&f, "ParmVar a");
}
