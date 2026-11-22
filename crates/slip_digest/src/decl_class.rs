//! The payload records of classes, enumerations, their members and
//! typedefs (`spec/digest-kinds.md` §5.5): pending.
//!
//! Each function writes one `record T` for the declaration `d`, read as
//! §2.3 says. Until the package of this file lands, each writes the
//! pending placeholder and then the expressions the record owns, so that
//! the other areas can be reached and tested (the "reach stub": it is no
//! table's encoding and is replaced whole).

use slip_kiln::kiln::ast::DeclId;

use crate::Area;
use crate::stream::Stream;
use crate::walk::Walk;

/// `TypedefData`.
pub(crate) fn typedef_data(_w: &mut Walk<'_>, s: &mut Stream, _d: DeclId) {
    s.pending(Area::DeclData);
}

/// `EnumData`.
pub(crate) fn enum_data(_w: &mut Walk<'_>, s: &mut Stream, _d: DeclId) {
    s.pending(Area::DeclData);
}

/// `EnumeratorData`.
pub(crate) fn enumerator_data(w: &mut Walk<'_>, s: &mut Stream, d: DeclId) {
    s.pending(Area::DeclData);
    let init = w.ast.decls.enumerator(d).and_then(|e| e.init);
    crate::expr::child_opt(w, s, init);
}

/// `RecordData`.
pub(crate) fn record_data(_w: &mut Walk<'_>, s: &mut Stream, _d: DeclId) {
    s.pending(Area::DeclData);
}

/// `FieldData`.
pub(crate) fn field_data(w: &mut Walk<'_>, s: &mut Stream, d: DeclId) {
    s.pending(Area::DeclData);
    let (width, init) = w.ast.decls.field(d).map_or((None, None), |f| (f.bit_width, f.init));
    crate::expr::child_opt(w, s, width);
    crate::expr::child_opt(w, s, init);
}
