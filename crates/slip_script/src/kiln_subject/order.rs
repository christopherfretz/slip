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
//! expansion's locations in the order of its tokens). A unit has two root
//! buffers: the main file and the predefines buffer, `<built-in>`, which
//! Kiln creates after the main file but reads first (it pushes it on top
//! of the main file, `kiln_pp/src/preprocessor.rs` `enter_main_file`).
//! A forced include (`-include x.h`) is an `#include` line of that buffer,
//! so the predefines root is ordered before every other; other roots (a
//! buffer Kiln adds for itself) by `FileId`, which is creation order.

use std::cmp::Ordering;

use slip_kiln::kiln::source::{FileId, Loc, SourceManager};

/// How many `#include` levels are followed: past the bound a chain has a
/// cycle, which Kiln's include stack does not make.
const DEPTH: usize = 1 << 20;

/// The name of the predefines buffer (`kiln_pp/src/predef.rs`).
const PREDEFINES: &str = "<built-in>";

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
    let rank = if sm.file_name(root) == PREDEFINES { 0 } else { root.index() as u64 + 1 };
    Some((rank, offsets))
}
