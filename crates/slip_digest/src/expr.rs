//! Expressions and canonical expression operands (`spec/digest-kinds.md`
//! §7, §4.4): pending (WP-024).

use slip_kiln::kiln::ast::{ExprId, ProfileId};

use crate::Area;
use crate::stream::Stream;
use crate::walk::Walk;

/// `child`: the expression `e`, walked here (§7).
pub(crate) fn child(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// `child?`.
pub(crate) fn child_opt(w: &mut Walk<'_>, s: &mut Stream, e: Option<ExprId>) {
    s.opt(e.is_some());
    if let Some(e) = e {
        child(w, s, e);
    }
}

/// `profile`: a canonical expression operand (§4.4).
pub(crate) fn profile(_w: &mut Walk<'_>, s: &mut Stream, _p: ProfileId) {
    s.pending(Area::Expr);
}

/// `profile` given by its expression (a function's trailing
/// requires-clause in its discriminator, digest-kinds §3.3): the canonical
/// operand of that expression (§4.4).
pub(crate) fn profile_expr(_w: &mut Walk<'_>, s: &mut Stream, _e: ExprId) {
    s.pending(Area::Expr);
}

/// A requires-clause (a template head's, a function's trailing one), as
/// the `child` its row names.
pub(crate) fn requires_clause(w: &mut Walk<'_>, s: &mut Stream, e: ExprId) {
    child(w, s, e);
}
