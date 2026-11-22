//! Names and constructs of templates (`spec/digest-kinds.md` §7), and
//! canonical expression operands (§4.4): pending, one function per kind.
//!
//! The rows of a kind's function (`expr::KindFn`): its kind's rows only,
//! in table order; `expr::child` writes the common rows before the call
//! and the `ColdExprData` rows after it (`lib.rs` "How to fill an
//! expression kind").

use slip_kiln::kiln::ast::{ExprId, ProfileId};

use crate::Area;
use crate::stream::Stream;
use crate::walk::Walk;

/// `UnresolvedLookupExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `UnresolvedLookupExpr.*`): pending.
pub(crate) fn unresolved_lookup_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `DependentScopeDeclRefExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `DependentScopeDeclRefExpr.*`): pending.
pub(crate) fn dependent_scope_decl_ref_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `CXXDependentScopeMemberExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `CXXDependentScopeMemberExpr.*`): pending.
pub(crate) fn cxx_dependent_scope_member_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `SubstNonTypeTemplateParmExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `SubstNonTypeTemplateParmExpr.*`): pending.
pub(crate) fn subst_non_type_template_parm_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `SubstNonTypeTemplateParmPackExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `SubstNonTypeTemplateParmPackExpr.*`): pending.
pub(crate) fn subst_non_type_template_parm_pack_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `FunctionParmPackExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `FunctionParmPackExpr.*`): pending.
pub(crate) fn function_parm_pack_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `SizeOfPackExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `SizeOfPackExpr.*`): pending.
pub(crate) fn size_of_pack_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `RequiresExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `RequiresExpr.*`): pending.
pub(crate) fn requires_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `ConceptSpecializationExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `ConceptSpecializationExpr.*`): pending.
pub(crate) fn concept_specialization_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `LambdaExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `LambdaExpr.*`): pending.
pub(crate) fn lambda_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `PackExpansionExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `PackExpansionExpr.*`): pending.
pub(crate) fn pack_expansion_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `CXXFoldExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `CXXFoldExpr.*`): pending.
pub(crate) fn cxx_fold_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `PackIndexingExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `PackIndexingExpr.*`): pending.
pub(crate) fn pack_indexing_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `CXXUnresolvedConstructExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `CXXUnresolvedConstructExpr.*`): pending.
pub(crate) fn cxx_unresolved_construct_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `CXXExpansionSelectExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `CXXExpansionSelectExpr.*`): pending.
pub(crate) fn cxx_expansion_select_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `IntegerPackExpr` (`spec/digest-kinds.md` §7, `ExprNode` rows `IntegerPackExpr.*`): pending.
pub(crate) fn integer_pack_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `profile`: a canonical expression operand (§4.4).
pub(crate) fn profile(_w: &mut Walk<'_>, s: &mut Stream, _p: ProfileId) {
    s.pending(Area::Expr);
}

/// `profile` given by its expression (a function's trailing
/// requires-clause in its discriminator, digest-kinds §3.3): the canonical
/// operand of that expression (§4.4). The whole operand of a
/// `DependentDecltype` is walked with `expr::child_keep_paren` (§7
/// "Parentheses").
pub(crate) fn profile_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}
