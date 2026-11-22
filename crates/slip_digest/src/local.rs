//! Local entities (`spec/digest.md` §4.3, `spec/digest-kinds.md` §2.4):
//! their ordinals here; walking a declaration in place is pending.

use slip_kiln::kiln::ast::DeclId;

use crate::Area;
use crate::stream::Stream;
use crate::walk::Walk;

/// The ordinal of a local entity of the unit being written: the next
/// one when the walk first meets it, by its declaration or by an earlier
/// reference (a `goto` before its label).
pub(crate) fn ordinal(w: &mut Walk<'_>, d: DeclId) -> u64 {
    let d = w.ast.first(d);
    if let Some(&n) = w.unit.locals.get(&d) {
        return n;
    }
    let n = w.unit.next_local;
    w.unit.next_local += 1;
    w.unit.locals.insert(d, n);
    n
}

/// `decl`: a declaration met inside a unit, walked in place (§2.4).
#[allow(dead_code, reason = "the interface of an area that lands later (lib.rs)")]
pub(crate) fn write_decl(_w: &mut Walk<'_>, s: &mut Stream, _d: DeclId) {
    s.pending(Area::Local);
}

/// `record LambdaData` (§10.4): the `lambda` row of a closure type's
/// `RecordData`.
#[allow(dead_code, reason = "the interface of an area that lands later (lib.rs)")]
pub(crate) fn lambda_data(_w: &mut Walk<'_>, s: &mut Stream, _l: slip_kiln::kiln::ast::LambdaId) {
    s.pending(Area::Local);
}
