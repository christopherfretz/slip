//! Statements (`spec/digest-kinds.md` §6): pending (WP-024).

use slip_kiln::kiln::ast::StmtId;

use crate::Area;
use crate::stream::Stream;
use crate::walk::Walk;

/// `child`: the statement `st`, walked here (§6).
#[allow(dead_code, reason = "the interface of an area that lands later (lib.rs)")]
pub(crate) fn child(_w: &mut Walk<'_>, s: &mut Stream, _st: StmtId) {
    s.pending(Area::Stmt);
}
