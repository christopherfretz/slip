//! Unit order (`spec/api.md` §1), computed slowly.
//!
//! **To be replaced by `slip_kiln::ranges::unit_order` (WP-084)**, which
//! has this signature. Until then every comparison of the subject goes
//! through [`unit_order`] and nothing else, so the swap is one line.
//!
//! A file location is ordered by its inclusion chain: the offset of each
//! `#include` from the outermost file inwards, then its own offset. That
//! is the order of the tokens after translation phase 4, since the tokens
//! of an included file stand where its `#include` was. A macro location
//! is ordered at its outermost invocation (`expansion_loc`), and two
//! locations of the same invocation by their raw value (Kiln allocates an
//! expansion's locations in the order of its tokens). Two root buffers
//! (the main file, the predefines, a virtual buffer) are ordered by their
//! `FileId`, which is creation order: a declaration in the predefines is
//! implicit, so no answer of the subject depends on that rank.

use std::cmp::Ordering;

use slip_kiln::kiln::source::{FileId, Loc, SourceManager};

/// How many `#include` levels are followed (a unit is far shallower).
const DEPTH: usize = 4096;

/// The order of `a` and `b` in the unit's token sequence after phase 4.
/// An invalid location sorts after every valid one.
pub fn unit_order(sm: &SourceManager, a: Loc, b: Loc) -> Ordering {
    if a == b {
        return Ordering::Equal;
    }
    match (chain(sm, a), chain(sm, b)) {
        (Some(x), Some(y)) => x.cmp(&y).then(a.cmp(&b)),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => a.cmp(&b),
    }
}

/// The root buffer's rank, then the offsets from the outermost file in.
fn chain(sm: &SourceManager, loc: Loc) -> Option<(u64, Vec<u32>)> {
    let mut at = sm.expansion_loc(loc);
    let mut offsets = Vec::new();
    let mut root: Option<FileId> = None;
    for _ in 0..DEPTH {
        let (file, off) = sm.offset_of(at)?;
        offsets.push(off);
        root = Some(file);
        let up = sm.include_loc(at);
        if !up.is_valid() {
            break;
        }
        at = up;
    }
    offsets.reverse();
    let root = root?;
    Some((root.index() as u64, offsets))
}
