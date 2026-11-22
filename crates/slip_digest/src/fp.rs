//! Floating-point state (`spec/digest-kinds.md` §6.1): pending, with the
//! statements.
//!
//! The enumerations of §6.1 (`FpContract`, `FpRound`, `FpEval`,
//! `FpExcept`) are written by their generated `row()` where a row names
//! them; this file holds the records.

use slip_kiln::kiln::ast::fp::FpOverride;

use crate::Area;
use crate::stream::Stream;

/// `record FpOverride` (`CompoundStmt.aux`, `ColdDeclData` `init_fp`).
#[allow(dead_code, reason = "the interface of an area that lands later (lib.rs)")]
pub(crate) fn fp_override(s: &mut Stream, _o: FpOverride) {
    s.pending(Area::Stmt);
}
