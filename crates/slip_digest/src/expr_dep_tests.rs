//! Tests of `expr_dep.rs` at the level of the stream: what a pair test
//! cannot isolate (the name of a raw `DeclName` against a declaration's,
//! the frame of a profile, the state a profile puts back).

use slip_kiln::kiln::ast::ExprId;
use slip_kiln::kiln::ast::decl::DeclKind;
use slip_kiln::kiln::ast::expr::ExprKind;

use crate::decl_func::tests::{of_kind, with_walk};
use crate::stream::Stream;
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

/// A requires-expression's parameters are numbered by the profile's
/// frame from 0, whatever the unit has numbered already.
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
        assert!(!matches!(fresh.cause(), Some(crate::Cause::Unaccounted(_))), "{:?}", fresh.cause());
        assert_eq!(w.unit.next_local, 5);
        assert!(of_kind(w, DeclKind::ParmVar).len() >= 3);
    });
}
