//! The other tables of `Ast` (`spec/digest-kinds.md` §10): pending
//! (WP-024).

use slip_kiln::kiln::ast::DeclId;

use crate::Area;
use crate::stream::Stream;
use crate::walk::Walk;

/// the rows `points` and `explicit_defs` of `ModuleTable` for `d` (§10.7).
pub(crate) fn module_rows(_w: &mut Walk<'_>, s: &mut Stream, _d: DeclId) {
    s.pending(Area::Other);
}
