//! The file range of a token span, its tier, and unit order
//! (`spec/api.md` §1 and §5.4, `spec/edits.md` §2.2; work package WP-084,
//! `spec/landed/api/WP-084.md`).
//!
//! **The range is Kiln's.** Which characters of a file editing a span of
//! tokens edits is `SourceManager::editable_file_range` (Kiln
//! `spec/source.md` "Editable file ranges"), and where a token ends is
//! `kiln_lex::token_end` (Kiln `spec/lex.md` "End of a token", reached
//! through [`Unit::token_end`]); whether a location starts a token is
//! Kiln's lexer ([`Unit::is_token_start`]). This module has no rule of its
//! own for a range of file characters or for what a token is; it only
//! refuses ranges Kiln gives: empty or reversed ones, ones in a
//! `<scratch space>` buffer, and two interim refusals until Kiln records
//! what they guess at (a possibly split `>`, a directive line;
//! `spec/landed/api/WP-084.md` "Until Kiln answers").
//!
//! What is Slip's:
//!
//! - **The tier** of a range Kiln gives, which Kiln's answer does not
//!   carry. It is derived from Kiln's own two rules,
//!   `file_loc_at_macro_start` and `file_loc_at_macro_end`: when each end
//!   of the span passes its rule (a file location passes both) Kiln's
//!   answer came from its rules 1 to 3 on the span as given, and the
//!   range is [`Tier::File`]; when Kiln answered and that is not so, the
//!   answer came through its rule 4 (both ends in macro-argument entries
//!   of one use of one parameter, moved to where the argument is
//!   written), and the range is [`Tier::MacroArgument`].
//! - **Unit order** ([`unit_order`]), which Kiln has no function for.

use std::cmp::Ordering;

use crate::Unit;
use crate::kiln::source::{FileId, FileOrigin, Loc, SourceManager, Span};

/// The tier of a file range (`spec/api.md` §5.4).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tier {
    /// Bytes written in a file, outside every macro argument; they may
    /// hold whole macro invocations.
    File = 0,
    /// Bytes inside one argument of one macro invocation, standing for
    /// one use of that argument (editing them edits every use).
    MacroArgument = 1,
}

/// A run of bytes of one file, and its tier.
///
/// `lo` and `hi` are Kiln's: offsets in its text of the file
/// (`SourceManager::text`), `lo < hi`. They are offsets in the file's
/// bytes exactly when Kiln changed nothing on loading the file
/// (`spec/edits.md` §2.3, `file_recoded`), which this module does not
/// look at.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct FileRange {
    pub file: FileId,
    pub lo: u32,
    pub hi: u32,
    pub tier: Tier,
}

/// The file range of the tokens `span` covers, for a span that already
/// ends at the end of its last token (a written reference's `range`, a
/// declaration extent's `name`), or `None`.
///
/// Kiln's `editable_file_range` of `span`, unchanged, with its tier (the
/// module documentation). `None` for an empty span (version 0 of the API
/// has no insertion point, `spec/api.md` §5.4), wherever Kiln gives none
/// or an empty or reversed run, in a `<scratch space>` buffer (`#embed`'s
/// tokens), and (INTERIM) for a range that holds or is in a directive
/// line. A span that is one whole macro invocation is that invocation's
/// bytes, at tier 0, also when its one token came from an argument
/// (`ID(foo)` with `#define ID(x) x`: the seven bytes `ID(foo)`).
///
/// It does not look at the buffer's origin, its load report or its
/// bytes: `not_a_file`, `file_recoded` and `not_plain` are the callers'
/// (`spec/edits.md` §2.3).
pub fn file_range_of_span(sm: &SourceManager, span: Span) -> Option<FileRange> {
    if span.lo == span.hi {
        return None;
    }
    let (file, lo, hi) = sm.editable_file_range(span)?;
    // An AST node's end may lie before its begin (a structured binding's
    // member reference begins at its name and ends at the `[`): such a
    // span, or one Kiln maps to an empty or reversed range, is no range.
    if lo >= hi {
        return None;
    }
    // Not a file the unit entered: `#embed`'s tokens, a `##` result.
    if sm.is_written_in_scratch_space(sm.loc(file, lo)) {
        return None;
    }
    // INTERIM (until Kiln records directive lines and skipped groups,
    // `spec/landed/api/WP-084.md` "Until Kiln answers"): a range that holds
    // a directive or part of a skipped group is no range.
    if touches_directive(sm.text(file).as_bytes(), lo as usize, hi as usize) {
        return None;
    }
    // Kiln's rules 1 to 3 on the span as given: each end that is a macro
    // location passes its rule (both rules are the identity on a file
    // location). Otherwise Kiln answered through rule 4.
    let whole = sm.file_loc_at_macro_start(span.lo).is_some()
        && sm.file_loc_at_macro_end(span.hi).is_some();
    let tier = if whole { Tier::File } else { Tier::MacroArgument };
    Some(FileRange { file, lo, hi, tier })
}

/// The file range of the tokens from the one at `first` to the one at
/// `last` (the locations of their first characters, as an expression's
/// `begin_loc` and `end_loc` are), or `None`.
///
/// `None` unless `first` and `last` are each the start of a token as
/// Kiln's lexer lexes the buffer ([`Unit::is_token_start`]: an AST node
/// can begin inside a token, as a call of a user-defined literal begins
/// at its suffix). The span ends at Kiln's end of the last token
/// ([`Unit::token_end`]); `None` when that has none, else
/// [`file_range_of_span`].
///
/// INTERIM (`spec/landed/api/WP-084.md` "Until Kiln answers"): `None`
/// when the last token, as `token_end` lexes it, begins with `>` and is
/// longer than one byte (`>>`, `>=`, `>>=`). The parser may have split
/// it (`B<vt<1>> y;` ends `vt<1>` at the first `>` of `>>`), which
/// `token_end` cannot know, and the end would be a byte too far.
pub fn file_range(unit: &Unit<'_>, first: Loc, last: Loc) -> Option<FileRange> {
    if !unit.is_token_start(first) || !unit.is_token_start(last) {
        return None;
    }
    let end = unit.token_end(last)?;
    if may_be_split(unit.sm(), last, end) {
        return None;
    }
    file_range_of_span(unit.sm(), Span::new(first, end))
}

/// Whether the token `last..end` is one the parser can split at its
/// first `>` (INTERIM, [`file_range`]).
fn may_be_split(sm: &SourceManager, last: Loc, end: Loc) -> bool {
    sm.span_text(Span::new(last, end)).is_some_and(|t| t.len() > 1 && t.starts_with('>'))
}

/// INTERIM: whether the bytes `lo..hi` of `text` hold a directive line or
/// lie in one: the line `lo` is on, or a line that starts inside
/// `lo..hi`, whose first byte after blanks, line splices and comments is
/// `#` or `%:`. Every skipped group is bounded by such lines. It can only
/// refuse: a raw string or a comment holding such a line is refused too.
fn touches_directive(text: &[u8], lo: usize, hi: usize) -> bool {
    let first = text[..lo].iter().rposition(|&c| c == b'\n').map_or(0, |k| k + 1);
    std::iter::once(first)
        .chain((lo..hi).filter(|&k| text[k] == b'\n').map(|k| k + 1))
        .any(|start| starts_with_hash(text, start))
}

/// Whether the line at `k` begins, after blanks, splices and comments,
/// with `#` or `%:`.
fn starts_with_hash(text: &[u8], mut k: usize) -> bool {
    loop {
        match text.get(k..).unwrap_or_default() {
            [b' ' | b'\t' | b'\x0b' | b'\x0c' | b'\r', ..] => k += 1,
            [b'\\', b'\n', ..] => k += 2,
            [b'\\', b'\r', b'\n', ..] => k += 3,
            [b'/', b'*', rest @ ..] => match rest.windows(2).position(|w| w == b"*/") {
                Some(p) => k += 2 + p + 2,
                None => return false,
            },
            [b'#', ..] | [b'%', b':', ..] => return true,
            _ => return false,
        }
    }
}

/// Bound on a walk to the root (`spec/landed/api/WP-084.md`): every step
/// moves to an older entry or an enclosing inclusion, so a chain is
/// finite; the bound only guards a malformed table.
const MAX_DEPTH: usize = 1 << 16;

/// The order of `a` and `b` in the unit's token sequence after phase 4
/// (`spec/api.md` §1 "Unit order").
///
/// Each location has a chain from a root buffer down to it: a macro
/// location's parent is where its entry's expansion begins
/// (`expansion_info(..).expansion.lo`: the invocation for a macro entry,
/// the parameter in the replacement list for an argument), a file
/// location's parent is the `#include` that entered its inclusion
/// (`include_loc`), and a location with neither is a root. The order is
/// that of the chains from the root, compared element by element by raw
/// location; a chain that is a prefix of the other comes first (an
/// invocation before the tokens it expands to, an `#include` before the
/// included file's tokens).
///
/// Within one inclusion and within one expansion entry, raw locations are
/// in text order, and the entries that share a parent location (an
/// argument split over several entries) are in creation order, which is
/// raw order; so for two tokens of the sequence this is their order
/// there. It is a total order on all locations: equal exactly when `a ==
/// b`. Locations that are no token of the sequence (an invocation's
/// parentheses, an argument as written) are placed by the same rule.
/// The invalid location is a root and comes first. Then the predefines
/// buffer (`<built-in>`), which the preprocessor reads before the main
/// file although it creates it after: it holds the command line's macros
/// and the `#include` lines of `-include`, so every location of a forced
/// header comes before every location of the main file. Then the other
/// roots (the main file, a buffer never included, the `<scratch space>`
/// buffer of an `#embed`) in raw order. That is wrong for `#embed`: its
/// tokens are written in a fresh scratch buffer, a root Kiln links to no
/// directive, so they sort after the main file (`spec/landed/api/WP-084.md`
/// "Until Kiln answers").
///
/// Each call walks both chains (two allocations); a large sort pays that
/// per comparison.
///
/// # Panics
/// When a chain is longer than 2^16 locations, which only a malformed
/// location table makes.
pub fn unit_order(sm: &SourceManager, a: Loc, b: Loc) -> Ordering {
    if a == b {
        return Ordering::Equal;
    }
    // One inclusion or one entry: raw order.
    if sm.same_entry_distance(a, b).is_some() || sm.same_entry_distance(b, a).is_some() {
        return a.to_raw().cmp(&b.to_raw());
    }
    let (pa, pb) = (chain(sm, a), chain(sm, b));
    // Root first: compare from the far ends.
    for (k, (x, y)) in pa.iter().rev().zip(pb.iter().rev()).enumerate() {
        if x != y {
            if k == 0 {
                return root_rank(sm, *x).cmp(&root_rank(sm, *y)).then(x.cmp(y));
            }
            return x.cmp(y);
        }
    }
    pa.len().cmp(&pb.len())
}

/// The rank of a root (raw location) in unit order: the invalid location,
/// then the predefines buffer, then the rest. The predefines buffer is
/// the in-memory buffer the preprocessor names `<built-in>` when it
/// enters the main file (Kiln `Preprocessor::enter_main_file`), the name
/// Kiln's own `-E` printer recognises it by.
fn root_rank(sm: &SourceManager, raw: u32) -> u8 {
    let loc = Loc::from_raw(raw);
    if !loc.is_valid() {
        return 0;
    }
    let builtin = !loc.is_macro()
        && sm.offset_of(loc).is_some_and(|(f, _)| {
            sm.file_origin(f) == FileOrigin::Virtual && sm.file_name(f) == "<built-in>"
        });
    if builtin { 1 } else { 2 }
}

/// The chain of `loc` to its root, `loc` first, as raw locations.
///
/// # Panics
/// When the chain is longer than `MAX_DEPTH`, which only a malformed
/// location table makes (every step goes to an older entry or an
/// enclosing inclusion).
fn chain(sm: &SourceManager, mut loc: Loc) -> Vec<u32> {
    let mut out = Vec::new();
    for _ in 0..MAX_DEPTH {
        out.push(loc.to_raw());
        let parent = if loc.is_macro() {
            sm.expansion_info(loc).map_or(Loc::INVALID, |e| e.expansion.lo)
        } else {
            sm.include_loc(loc)
        };
        if !parent.is_valid() {
            return out;
        }
        loc = parent;
    }
    panic!("unit_order: a chain of locations longer than {MAX_DEPTH} from {loc:?}");
}
