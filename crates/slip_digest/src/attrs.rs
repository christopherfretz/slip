//! Attributes and annotations (`spec/digest-kinds.md` §10.1, §10.5):
//! pending (WP-024).

use slip_kiln::kiln::ast::DeclId;

use crate::Area;
use crate::stream::Stream;
use crate::walk::Walk;

/// The row `decl_attrs` of `AttrArena` (`attrs`, §10.1) and the rows of
/// `AnnotArena` (§10.5) for `d`.
pub(crate) fn decl_attrs(_w: &mut Walk<'_>, s: &mut Stream, _d: DeclId) {
    s.pending(Area::Attrs);
}
