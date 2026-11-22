//! Construction, initialisation, `new` and `delete`
//! (`spec/digest-kinds.md` §7): pending, one function per kind.
//!
//! The rows of a kind's function (`expr::KindFn`): its kind's rows only,
//! in table order; `expr::child` writes the common rows before the call
//! and the `ColdExprData` rows after it (`lib.rs` "How to fill an
//! expression kind").

use slip_kiln::kiln::ast::ExprId;

use crate::Area;
use crate::stream::Stream;
use crate::walk::Walk;

/// `InitListExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `InitListExpr.*`): pending.
pub(crate) fn init_list_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `ImplicitValueInitExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `ImplicitValueInitExpr.*`): pending.
pub(crate) fn implicit_value_init_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `DesignatedInitExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `DesignatedInitExpr.*`): pending.
pub(crate) fn designated_init_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `CXXConstructExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `CXXConstructExpr.*`): pending.
pub(crate) fn cxx_construct_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `CXXTemporaryObjectExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `CXXTemporaryObjectExpr.*`): pending.
pub(crate) fn cxx_temporary_object_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `CXXInheritedCtorInitExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `CXXInheritedCtorInitExpr.*`): pending.
pub(crate) fn cxx_inherited_ctor_init_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `CXXDefaultArgExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `CXXDefaultArgExpr.*`): pending.
pub(crate) fn cxx_default_arg_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `CXXDefaultInitExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `CXXDefaultInitExpr.*`): pending.
pub(crate) fn cxx_default_init_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `CXXScalarValueInitExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `CXXScalarValueInitExpr.*`): pending.
pub(crate) fn cxx_scalar_value_init_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `CXXParenListInitExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `CXXParenListInitExpr.*`): pending.
pub(crate) fn cxx_paren_list_init_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `ArrayInitLoopExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `ArrayInitLoopExpr.*`): pending.
pub(crate) fn array_init_loop_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `ArrayInitIndexExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `ArrayInitIndexExpr.*`): pending.
pub(crate) fn array_init_index_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `CXXStdInitializerListExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `CXXStdInitializerListExpr.*`): pending.
pub(crate) fn cxx_std_initializer_list_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `CompoundLiteralExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `CompoundLiteralExpr.*`): pending.
pub(crate) fn compound_literal_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `NoInitExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `NoInitExpr.*`): pending.
pub(crate) fn no_init_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `DesignatedInitUpdateExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `DesignatedInitUpdateExpr.*`): pending.
pub(crate) fn designated_init_update_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `CXXNewExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `CXXNewExpr.*`): pending.
pub(crate) fn cxx_new_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `CXXDeleteExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `CXXDeleteExpr.*`): pending.
pub(crate) fn cxx_delete_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `CXXPseudoDestructorExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `CXXPseudoDestructorExpr.*`): pending.
pub(crate) fn cxx_pseudo_destructor_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `CXXThrowExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `CXXThrowExpr.*`): pending.
pub(crate) fn cxx_throw_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `CXXRewrittenBinaryOperator` (`spec/digest-kinds.md` §7, `ExprNode` rows `CXXRewrittenBinaryOperator.*`): pending.
pub(crate) fn cxx_rewritten_binary_operator(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `ParenListExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `ParenListExpr.*`): pending.
pub(crate) fn paren_list_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `ColdExprData` `designators` (§7.4) of `e`, which has an entry:
/// `enum Designator*`, outermost first. Called by `expr::child` for an
/// expression with an entry; with none it writes the empty list itself.
pub(crate) fn designators(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}
