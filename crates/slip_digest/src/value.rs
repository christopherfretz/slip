//! Constant values and strings (`spec/digest-kinds.md` §8, §10.3):
//! pending (WP-024).

use slip_kiln::kiln::ast::{ConstValueId, StringLitId};

use crate::Area;
use crate::stream::Stream;
use crate::walk::Walk;

/// `value`: the constant `v` by the rows of §8.
pub(crate) fn value(_w: &mut Walk<'_>, s: &mut Stream, _v: ConstValueId) {
    s.pending(Area::Value);
}

/// `string`: a string literal by content (§10.3).
#[allow(dead_code, reason = "the interface of an area that lands later (lib.rs)")]
pub(crate) fn string(_w: &mut Walk<'_>, s: &mut Stream, _l: StringLitId) {
    s.pending(Area::Value);
}
