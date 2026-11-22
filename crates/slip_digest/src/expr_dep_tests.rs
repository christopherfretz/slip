//! Tests of `expr_dep.rs` at the level of the stream: what a pair test
//! cannot isolate (the name of a raw `DeclName` against a declaration's,
//! the frame of a profile, the state a profile puts back).

use slip_kiln::kiln::ast::ExprId;
use slip_kiln::kiln::ast::decl::DeclKind;
use slip_kiln::kiln::ast::expr::ExprKind;

use crate::decl_func::tests::{of_kind, with_walk};
use crate::stream::{FlagWord, Stream};
use crate::walk::Walk;

/// Every expression of kind `k`.
fn of_expr_kind(w: &Walk<'_>, k: ExprKind) -> Vec<ExprId> {
    (1..=w.ast.exprs.len())
        .filter_map(|i| ExprId::try_new(u32::try_from(i).ok()?))
        .filter(|&e| w.ast.exprs.get(e).kind() == k)
        .collect()
}

/// `decl_name` of a declaration's raw name is what `Walk::plain_name`
/// gives the declaration (`keys.rs`), for every kind of name.
#[test]
fn a_raw_name_is_the_declarations_name() {
    let src = "struct S { S(); ~S(); operator int(); int operator+(int); void f(); };\n\
               unsigned long long operator\"\"_k(unsigned long long);\n\
               template<class T> struct G { G(T); };\nG(int) -> G<int>;\n";
    with_walk(src, |w| {
        let ds: Vec<_> = w.ast.decls.ids().collect();
        let mut seen = 0;
        for d in ds {
            let raw = w.ast.decls.get(d).name().to_raw();
            let want = w.plain_name(d).ok().flatten();
            let got = super::decl_name(w, raw);
            assert_eq!(got, want, "{:?}", w.ast.decls.get(d).kind());
            seen += usize::from(got.is_some());
        }
        assert!(seen >= 8, "names of every kind were compared: {seen}");
    });
}

/// A profile puts back the expression state it found, numbering and
/// frame included, and leaves no frame behind.
#[test]
fn a_profile_puts_the_state_back() {
    let src = "template<class T> auto f(T a, T b) -> decltype(a + b);\n";
    with_walk(src, |w| {
        let op = of_expr_kind(w, ExprKind::BinaryOperator);
        let e = *op.first().expect("a + b");
        w.unit.expr.next_opaque = 3;
        w.unit.expr.dependent = 2;
        let mut s = Stream::new();
        super::profile_expr(w, &mut s, e);
        assert_eq!((w.unit.expr.next_opaque, w.unit.expr.dependent), (3, 2));
        assert!(w.unit.expr.profile.is_none());
        assert_eq!(w.unit.next_local, 0, "parameters are Ref::param, no ordinal");
        assert_eq!(s.cause(), None);
    });
}

/// In a profile the parameters are `Ref::param` by position: `a + b`
/// and `b + a` differ, and the same positions give the same bytes in
/// another function.
#[test]
fn profile_parameters_are_positional() {
    let src = "template<class T> auto f(T a, T b) -> decltype(a + b);\n\
               template<class T> auto g(T b, T a) -> decltype(b + a);\n\
               template<class T> auto h(T a, T b) -> decltype(b + a);\n";
    with_walk(src, |w| {
        let op = of_expr_kind(w, ExprKind::BinaryOperator);
        assert_eq!(op.len(), 3);
        let bytes: Vec<Vec<u8>> = op
            .iter()
            .map(|&e| {
                let mut s = Stream::new();
                super::profile_expr(w, &mut s, e);
                assert_eq!(s.cause(), None);
                s.as_bytes().to_vec()
            })
            .collect();
        assert_eq!(bytes[0], bytes[1], "the same positions");
        assert_ne!(bytes[0], bytes[2], "swapped positions");
    });
}

/// A requires-expression's parameters are written by position in a
/// profile (profile mode, `expr_profile.rs`), whatever the unit has
/// numbered already.
#[test]
fn requires_parameters_are_numbered_in_the_profile() {
    let src = "template<class T> void f(T) requires requires(T x, T y) { x + y; };\n";
    with_walk(src, |w| {
        let r = of_expr_kind(w, ExprKind::RequiresExpr);
        let e = *r.first().expect("the requires-expression");
        let mut fresh = Stream::new();
        super::profile_expr(w, &mut fresh, e);
        // The unit has numbered other locals: nothing changes.
        w.unit.next_local = 5;
        let mut later = Stream::new();
        super::profile_expr(w, &mut later, e);
        assert_eq!(fresh.as_bytes(), later.as_bytes());
        // A parameter's stream may be pending (another area's rows).
        assert!(
            !matches!(fresh.cause(), Some(crate::Cause::Unaccounted(_))),
            "{:?}",
            fresh.cause()
        );
        assert_eq!(w.unit.next_local, 5);
        assert!(of_kind(w, DeclKind::ParmVar).len() >= 3);
    });
}

/// The bytes kind function `f` writes for every node of kind `k` in
/// `src` (C++26, no errors), in id order.
fn kind_bytes(src: &str, k: ExprKind, f: crate::expr::KindFn) -> Vec<Vec<u8>> {
    with_walk(src, |w| {
        of_expr_kind(w, k)
            .into_iter()
            .map(|e| {
                let mut s = Stream::new();
                f(w, &mut s, e);
                s.as_bytes().to_vec()
            })
            .collect()
    })
}

fn prefix(f: impl FnOnce(&mut Stream)) -> Vec<u8> {
    let mut s = Stream::new();
    f(&mut s);
    s.as_bytes().to_vec()
}

/// Whether some written stream starts with each of `want`.
fn starts(got: &[Vec<u8>], want: &[Vec<u8>]) {
    for p in want {
        assert!(got.iter().any(|g| g.starts_with(p)), "no node writes {p:?}: {got:?}");
    }
}

/// `SubstNonTypeTemplateParmExpr.bits-reference-parameter` (WP-034
/// rule): a member template of an instantiation names each replacement;
/// one parameter is a reference, one is not, and the flag word says so
/// (a constant or a skipped row fails).
#[test]
fn subst_reference_parameter_bit() {
    let src = "constexpr int c = 1;\n\
               template<const int &N> struct AR { template<class U> static auto g(U u) -> decltype(u + N); };\n\
               template<int N> struct A { template<class U> static auto g(U u) -> decltype(u + N); };\n\
               auto p = &AR<c>::g<int>;\nauto q = &A<1>::g<int>;\n";
    let got = kind_bytes(
        src,
        ExprKind::SubstNonTypeTemplateParmExpr,
        super::subst_non_type_template_parm_expr,
    );
    starts(
        &got,
        &[
            prefix(|s| {
                s.flags(FlagWord::new().bit(true));
            }),
            prefix(|s| {
                s.flags(FlagWord::new().bit(false));
            }),
        ],
    );
}

/// `RequiresExpr.aux` and `ConceptSpecializationExpr.aux`: the stored
/// result of each node, 0 in a pattern, 1 false, 2 true.
#[test]
fn stored_results() {
    let src = "template<class T> concept C = sizeof(T) == 4;\n\
               bool a = C<int>;\nbool b = C<char>;\ntemplate<class T> bool d = C<T>;\n";
    let got =
        kind_bytes(src, ExprKind::ConceptSpecializationExpr, super::concept_specialization_expr);
    starts(
        &got,
        &[0u64, 1, 2].map(|v| {
            prefix(|s| {
                s.uv(v);
            })
        }),
    );
    let src = "struct X {};\n\
               template<class T> constexpr bool r = requires(T t) { t + 1; };\n\
               template<class T> concept K = requires(T t) { t + 1; };\n\
               bool a = K<int>;\nbool b = K<X>;\nbool c = r<int>;\n";
    let got = kind_bytes(src, ExprKind::RequiresExpr, super::requires_expr);
    starts(
        &got,
        &[0u64, 2].map(|v| {
            prefix(|s| {
                s.uv(v);
            })
        }),
    );
}

/// `CXXExpansionSelectExpr.aux` and `.a`: the form of each expansion
/// statement's pattern and its template level's depth (in function
/// bodies, which the statements' package writes: here directly).
#[test]
fn expansion_select_form_and_depth() {
    use crate::tables::expansion_form;
    let src = "struct P { int a, b; };\n\
               template<class T> void f(T t) { template for (auto x : t) { (void)x; } }\n\
               template<class T> void g() { template for (auto x : {1, 2}) { (void)x; } }\n\
               void h() { P p{1, 2}; template for (auto x : p) { (void)x; } }\n";
    let got = kind_bytes(src, ExprKind::CXXExpansionSelectExpr, super::cxx_expansion_select_expr);
    starts(
        &got,
        &[expansion_form::DEPENDENT, expansion_form::ENUMERATING].map(|r| {
            prefix(|s| {
                s.tag(r);
            })
        }),
    );
    // Kiln builds the node with `Dependent` or `Enumerating` only
    // (`kiln_sema` expansion.rs:248, :1142): the other two forms are the
    // statement's. The depth: 0 in a function that is not a template, 1
    // in a template's.
    let depths: std::collections::BTreeSet<u8> =
        got.iter().filter_map(|g| g.get(1).copied()).collect();
    assert!(depths.len() >= 2, "depths {depths:?}");
}

/// Whether at least two nodes were written and some two differ.
fn distinct(got: &[Vec<u8>]) {
    assert!(got.len() >= 2, "{} nodes", got.len());
    assert!(got.iter().any(|g| g != &got[0]), "every node wrote the same bytes: {got:?}");
}

/// `SubstNonTypeTemplateParmPackExpr.b`: two instantiations with other
/// arguments for the same pack write other bytes. A pair test cannot
/// isolate the row: the arguments are those of the enclosing
/// instantiation, in its key.
#[test]
fn subst_pack_arguments() {
    let src = "template<int... Ns> struct S { template<int... Ms> static auto g() -> decltype(((Ns * Ms) + ...)); };\n\
               auto p = &S<1, 2>::g<3, 4>;\nauto q = &S<1, 3>::g<3, 4>;\n";
    distinct(&kind_bytes(
        src,
        ExprKind::SubstNonTypeTemplateParmPackExpr,
        super::subst_non_type_template_parm_pack_expr,
    ));
}

/// `FunctionParmPackExpr.b`: the expanded parameters of two
/// instantiations are other declarations (the pair test cannot isolate
/// the row, as above).
#[test]
fn function_parameter_pack_expansion() {
    let src = "template<class... Ts> struct R { template<class... Us> static auto g(Ts... ts) -> decltype(((ts + Us()) + ...)); };\n\
               auto p = &R<int, long>::g<int, int>;\nauto q = &R<int, long, short>::g<int, int, int>;\n";
    distinct(&kind_bytes(src, ExprKind::FunctionParmPackExpr, super::function_parm_pack_expr));
}

/// `Ref::param` writes a parameter's function-scope depth and position
/// (the full walk's form, `param_ref`): `a` of `f` and `b` of the
/// function type in its parameter are both at position 0, at depths 0
/// and 1 (item 7, M28).
#[test]
fn param_ref_writes_depth_and_position() {
    let src = "template<class T> auto f(T a, int (*g)(T b), T c) -> int;\n";
    with_walk(src, |w| {
        let bytes = |w: &mut Walk<'_>, which: &str| {
            let d = crate::decl_func::tests::named(w, which, DeclKind::ParmVar)[0];
            let mut s = Stream::new();
            super::param_ref(w, &mut s, d, crate::Cause::Unaccounted("test"));
            assert_eq!(s.cause(), None);
            s.as_bytes().to_vec()
        };
        let (a, b, c) = (bytes(w, "a"), bytes(w, "b"), bytes(w, "c"));
        assert_ne!(a, b, "depth");
        assert_ne!(a, c, "position");
    });
}
