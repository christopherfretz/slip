//! The expression kinds of no other file (`spec/digest-kinds.md` §7):
//! pending, one function per kind.
//!
//! The rows of a kind's function (`expr::KindFn`): its kind's rows only,
//! in table order; `expr::child` writes the common rows before the call
//! and the `ColdExprData` rows after it (`lib.rs` "How to fill an
//! expression kind").

use slip_kiln::kiln::ast::ExprId;

use crate::Area;
use crate::stream::Stream;
use crate::walk::Walk;

/// `PredefinedExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `PredefinedExpr.*`): pending.
pub(crate) fn predefined_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `SourceLocExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `SourceLocExpr.*`): pending.
pub(crate) fn source_loc_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `CXXNoexceptExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `CXXNoexceptExpr.*`): pending.
pub(crate) fn cxx_noexcept_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `CXXTypeidExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `CXXTypeidExpr.*`): pending.
pub(crate) fn cxx_typeid_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `UnaryExprOrTypeTraitExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `UnaryExprOrTypeTraitExpr.*`): pending.
pub(crate) fn unary_expr_or_type_trait_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `TypeTraitExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `TypeTraitExpr.*`): pending.
pub(crate) fn type_trait_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `ArrayTypeTraitExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `ArrayTypeTraitExpr.*`): pending.
pub(crate) fn array_type_trait_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `ExpressionTraitExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `ExpressionTraitExpr.*`): pending.
pub(crate) fn expression_trait_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `OffsetOfExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `OffsetOfExpr.*`): pending.
pub(crate) fn offset_of_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `CoawaitExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `CoawaitExpr.*`): pending.
pub(crate) fn coawait_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `DependentCoawaitExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `DependentCoawaitExpr.*`): pending.
pub(crate) fn dependent_coawait_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `CoyieldExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `CoyieldExpr.*`): pending.
pub(crate) fn coyield_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `StmtExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `StmtExpr.*`): pending.
pub(crate) fn stmt_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `AddrLabelExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `AddrLabelExpr.*`): pending.
pub(crate) fn addr_label_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `ChooseExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `ChooseExpr.*`): pending.
pub(crate) fn choose_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `VAArgExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `VAArgExpr.*`): pending.
pub(crate) fn va_arg_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `AtomicExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `AtomicExpr.*`): pending.
pub(crate) fn atomic_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `ShuffleVectorExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `ShuffleVectorExpr.*`): pending.
pub(crate) fn shuffle_vector_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `ConvertVectorExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `ConvertVectorExpr.*`): pending.
pub(crate) fn convert_vector_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `ExtVectorElementExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `ExtVectorElementExpr.*`): pending.
pub(crate) fn ext_vector_element_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `PseudoObjectExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `PseudoObjectExpr.*`): pending.
pub(crate) fn pseudo_object_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `GenericSelectionExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `GenericSelectionExpr.*`): pending.
pub(crate) fn generic_selection_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `ColdExprData` `array_trait` (§7.4) of `e`, which has an entry: the
/// rows of `record ArrayTraitExtra` (`expr::child` wrote the presence).
pub(crate) fn array_trait_extra(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `ColdExprData` `offsetof_pattern-components` (§7.4) of `e`, which has
/// an `offsetof_pattern` entry: `enum OffsetofComponent*` (`expr::child`
/// wrote `offsetof_pattern-type`).
pub(crate) fn offsetof_components(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `ColdExprData` `offsetof_layout` (§7.4) of `e`, which has an entry:
/// `enum OffsetofStep*`.
pub(crate) fn offsetof_layout(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}
