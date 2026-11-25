//! Where a name token is written, and why it cannot be edited
//! (`spec/edits.md` §2.2, §2.3, §4.2; work package WP-068,
//! `spec/landed/edits/WP-068.md`).
//!
//! **The range is `slip_kiln::ranges`'.** This module computes no range
//! and no end of a token. Every range it gives is an answer of
//! [`ranges`](crate::ranges) for a span Kiln gave (a name's span, the one
//! token of an invocation's name): [`ranges::token_range_or_why`],
//! [`ranges::file_range`], and for a `macro_body` refusal, which points
//! into a `#define` line, [`ranges::spelled_range`]. What `sites` adds is
//! read from Kiln's source manager as facts: where the token is written
//! (the tier, which Kiln has no function for: `expansion_info`,
//! `immediate_spelling_loc`, `spelling_loc`, `is_written_in_scratch_space`,
//! `file_origin`), the outermost invocation (`expansion_loc`), whether the
//! file's text is its bytes (`load_report`), and the file's identity (the
//! shard's `inputs`, through [`Unit::real_path`]).
//!
//! **Reasons.** [`Reason`] is the table of `spec/edits.md` §2.3, in its
//! order; [`Reason::wire`] is its value on the wire (§4.2,
//! `slip_model::edits::Reason`).

use slip_model::FileRef;
use slip_model::edits::{MacroSite, Reason as WireReason, tiers};
use slip_model::transform::{FileClass, Ownership};

use crate::Unit;
use crate::kiln::source::{ExpansionKind, FileId, FileOrigin, Loc, Span};
use crate::ranges::{self, NoRange, Tier};

/// Why a name is not edited: the rows of `spec/edits.md` §2.3, in the
/// table's order (when several hold, the first is given).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Reason {
    /// Tier 2: written in a macro's replacement list.
    MacroBody,
    /// Tier 3: made by `##` or `#`, or by a builtin macro.
    Pasted,
    /// Tier X: the span is not one token, or not of one place, or Kiln
    /// gives no range.
    Crossing,
    /// Tier X: written in a buffer that is not a file (the predefines,
    /// the command line's macros).
    NotAFile,
    /// The range holds a directive or part of a skipped group
    /// (`slip_kiln::ranges`' answer; at this pin an interim byte check,
    /// `spec/landed/api/WP-084.md` decision 10).
    Directive,
    /// The file is read-only.
    NotOwned,
    /// The file is rebuilt: the build writes its bytes (WP-118,
    /// `spec/edits.md` §2.1).
    Rebuilt,
    /// Kiln's text of the file is not its bytes (invalid UTF-8 replaced).
    FileRecoded,
    /// The bytes of the range are not the name's spelling.
    NotPlain,
}

impl Reason {
    /// The wire's value (`spec/edits.md` §4.2).
    pub fn wire(self) -> WireReason {
        match self {
            Reason::MacroBody => WireReason::MacroBody,
            Reason::Pasted => WireReason::Pasted,
            Reason::Crossing => WireReason::Crossing,
            Reason::NotAFile => WireReason::NotAFile,
            Reason::Directive => WireReason::Directive,
            Reason::NotOwned => WireReason::NotOwned,
            Reason::Rebuilt => WireReason::Rebuilt,
            Reason::FileRecoded => WireReason::FileRecoded,
            Reason::NotPlain => WireReason::NotPlain,
        }
    }
}

/// Where a name token is written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Located {
    /// Tier 0 or 1: the token's bytes in a file, editable. `lo..hi` and
    /// the tier are `ranges::file_range_of_span`'s for the name's span.
    At { file: FileId, lo: u64, hi: u64, tier: u64, macro_site: Option<MacroSite> },
    /// It cannot be edited. `at` is the range a `refuse` claim carries
    /// (`spec/edits.md` §4.2); `None` is `[0, 0)` of the main file.
    /// `tier` is one bit of `slip_model::edits::tiers`.
    Refused {
        reason: Reason,
        tier: u64,
        at: Option<(FileId, u64, u64)>,
        macro_site: Option<MacroSite>,
    },
}

/// The questions a native producer asks about the names of one unit.
pub struct Sites<'u, 'r> {
    unit: &'u Unit<'r>,
    ownership: &'u Ownership,
}

/// Where rule 1 found the token written.
#[derive(Clone, Copy, Debug)]
enum Written {
    /// At a file location: tier 0 (`false`) or 1 (`true`) when its buffer
    /// is a file (the second field), tier X `not_a_file` otherwise, which
    /// in §2.3's order comes after `crossing` (decided by rule 2).
    File(bool, bool),
    /// Refused: the reason and its tier bit.
    Not(Reason, u64),
}

/// Bound on the walk of rule 1: every step goes to an older entry.
const MAX_STEPS: usize = 1 << 16;

impl<'u, 'r> Sites<'u, 'r> {
    /// The sites of `unit`, whose owned files are `ownership`'s.
    pub fn new(unit: &'u Unit<'r>, ownership: &'u Ownership) -> Sites<'u, 'r> {
        Sites { unit, ownership }
    }

    /// Where the name token `name` is written, and whether it may be
    /// edited (`spec/edits.md` §2.2, §2.3). `name` is the span of one
    /// name token as `slip_kiln::written` gives it; `spelling` the
    /// identifier the caller expects there. A span of more than one token
    /// (a qualified name, `operator+`) is refused `crossing` (rule 2).
    ///
    /// The rules, in the order of §2.3's table:
    ///
    /// 1. The location of the name's first character is followed to its
    ///    immediate spelling while it is in a macro-argument entry. A file
    ///    location is tier 0 if no step was taken and 1 otherwise, unless
    ///    it is in `<scratch space>` (tier 3, `pasted`); a location in any
    ///    other entry is spelled in scratch space (3, `pasted`), in a
    ///    buffer that is not a file (X, `not_a_file`), or in a
    ///    replacement list (2, `macro_body`).
    /// 2. The range is `ranges::token_range_or_why(name)`: none, or a span
    ///    that is not one token, is X `crossing`, or, when `ranges` says
    ///    the range holds a directive,
    ///    `directive` (`not_a_file` if the token of rule 1 is written in a
    ///    buffer that is not a file: that row comes first).
    /// 3. The token of rule 1, or the range, is in a buffer that is not a
    ///    file: X `not_a_file`.
    /// 4. The file is read-only: `not_owned`.
    /// 5. Its load report is not clean: `file_recoded`.
    /// 6. Its bytes in the range are not `spelling`, or the tier of rule 1
    ///    is not `ranges`' tier: `not_plain`.
    ///
    /// The refusal's `at`: for `macro_body` the token's bytes in the
    /// definition (`ranges::spelled_range`; the invocation's name when
    /// there are none); for `not_owned` and `not_plain` the range of rule
    /// 2; for every other reason the invocation's name token. No `at` is
    /// given in a buffer that is not a file, nor in one whose load report
    /// is not clean (its offsets would not be the file's).
    ///
    /// `macro_site` is the outermost invocation's name token
    /// (`expansion_loc` of the name's first character; its range is
    /// `ranges::file_range` of that one token) and its spelling, when the
    /// name came from an expansion and that token is in the file of the
    /// `At` or of the refusal's `at` (`MacroSite::name_start` is an offset
    /// in the claim's file).
    pub fn name_token(&self, name: Span, spelling: &[u8]) -> Located {
        let sm = self.unit.sm();
        let invocation = self.invocation(name.lo);
        let refused = |reason: Reason, tier: u64, at: Option<(FileId, u32, u32)>| {
            let at = at.filter(|&(f, ..)| self.is_file(f) && self.is_clean(f));
            Located::Refused {
                reason,
                tier,
                at: at.map(|(f, lo, hi)| (f, u64::from(lo), u64::from(hi))),
                macro_site: self.macro_site(invocation, at.map(|(f, ..)| f)),
            }
        };
        let (arg, in_file) = match self.written(name.lo) {
            Written::File(arg, in_file) => (arg, in_file),
            Written::Not(Reason::MacroBody, tier) => {
                let body = ranges::spelled_range(sm, name);
                return refused(Reason::MacroBody, tier, body.or(invocation));
            }
            Written::Not(reason, tier) => return refused(reason, tier, invocation),
        };
        let (tier, want) =
            if arg { (tiers::T1, Tier::MacroArgument) } else { (tiers::T0, Tier::File) };
        let r = match ranges::token_range_or_why(self.unit, name) {
            Ok(r) => r,
            Err(NoRange::Directive) if in_file => {
                return refused(Reason::Directive, tier, invocation);
            }
            Err(NoRange::Directive) => return refused(Reason::NotAFile, tiers::X, invocation),
            Err(_) => return refused(Reason::Crossing, tiers::X, invocation),
        };
        let range = Some((r.file, r.lo, r.hi));
        if !in_file || !self.is_file(r.file) {
            return refused(Reason::NotAFile, tiers::X, invocation);
        }
        if !self.is_owned(r.file) {
            // WP-118 R7.1: a rebuilt file's bytes are the build's.
            let reason = match self.class(r.file) {
                FileClass::Rebuilt => Reason::Rebuilt,
                FileClass::Owned | FileClass::ReadOnly => Reason::NotOwned,
            };
            return refused(reason, tier, range);
        }
        if !self.is_clean(r.file) {
            return refused(Reason::FileRecoded, tier, invocation);
        }
        let bytes = sm.text(r.file).as_bytes().get(r.lo as usize..r.hi as usize);
        if r.tier != want || bytes != Some(spelling) {
            return refused(Reason::NotPlain, tier, range);
        }
        Located::At {
            file: r.file,
            lo: u64::from(r.lo),
            hi: u64::from(r.hi),
            tier,
            macro_site: self.macro_site(invocation, Some(r.file)),
        }
    }

    /// The `FileRef` (real path, size, content digest of the original
    /// bytes) of a file a claim names: the shard's `inputs` entry of the
    /// buffer. `None` for a buffer that is not a file.
    pub fn file_ref(&self, file: FileId) -> Option<FileRef> {
        if !self.is_file(file) {
            return None;
        }
        let path = self.unit.real_path(file)?;
        let inputs = self.unit.inputs();
        let at = inputs.binary_search_by(|f| f.path.as_slice().cmp(path)).ok()?;
        Some(inputs[at].clone())
    }

    /// Whether `file` is a file the transform may edit (`spec/edits.md`
    /// §2.1): a file whose real path the ownership owns.
    pub fn is_owned(&self, file: FileId) -> bool {
        self.is_file(file) && self.unit.real_path(file).is_some_and(|p| self.ownership.is_owned(p))
    }

    /// The class of `file` (`spec/edits.md` §2.1, WP-118): by its real
    /// path; a buffer that is no file, or has no real path, is read-only.
    pub fn class(&self, file: FileId) -> FileClass {
        if !self.is_file(file) {
            return FileClass::ReadOnly;
        }
        match self.unit.real_path(file) {
            Some(p) => self.ownership.class(p),
            None => FileClass::ReadOnly,
        }
    }

    /// Rule 1 (`name_token`).
    fn written(&self, first: Loc) -> Written {
        let sm = self.unit.sm();
        let mut loc = first;
        let mut steps = 0usize;
        while let Some(e) = sm.expansion_info(loc) {
            if e.kind != ExpansionKind::MacroArg {
                break;
            }
            if steps == MAX_STEPS {
                return Written::Not(Reason::Crossing, tiers::X);
            }
            loc = sm.immediate_spelling_loc(loc);
            steps += 1;
        }
        if !loc.is_valid() {
            return Written::Not(Reason::Crossing, tiers::X);
        }
        let at = if loc.is_macro() { sm.spelling_loc(loc) } else { loc };
        if sm.is_written_in_scratch_space(at) {
            return Written::Not(Reason::Pasted, tiers::T3);
        }
        let in_file = sm.offset_of(at).is_some_and(|(f, _)| self.is_file(f));
        if !loc.is_macro() {
            return Written::File(steps > 0, in_file);
        }
        if !in_file {
            return Written::Not(Reason::NotAFile, tiers::X);
        }
        Written::Not(Reason::MacroBody, tiers::T2)
    }

    /// The range of the name token of the outermost invocation that
    /// produced `first`, when `first` is a macro location and that token
    /// is a file's.
    fn invocation(&self, first: Loc) -> Option<(FileId, u32, u32)> {
        if !first.is_macro() {
            return None;
        }
        let at = self.unit.sm().expansion_loc(first);
        let r = ranges::file_range(self.unit, at, at)?;
        (self.is_file(r.file) && self.is_clean(r.file)).then_some((r.file, r.lo, r.hi))
    }

    /// The `MacroSite` of `invocation` when it is in `file`.
    fn macro_site(
        &self,
        invocation: Option<(FileId, u32, u32)>,
        file: Option<FileId>,
    ) -> Option<MacroSite> {
        let (f, lo, hi) = invocation?;
        if Some(f) != file {
            return None;
        }
        let name = self.unit.sm().text(f).as_bytes().get(lo as usize..hi as usize)?;
        Some(MacroSite { name_start: u64::from(lo), name: name.to_vec() })
    }

    /// A buffer read from disk (or lazily, and read) that is one of the
    /// shard's inputs: not the predefines, the command line's macros, a
    /// `<scratch space>` buffer or a snippet's in-memory file.
    fn is_file(&self, file: FileId) -> bool {
        self.unit.sm().file_origin(file) != FileOrigin::Virtual
            && self.unit.real_path(file).is_some()
    }

    /// Kiln changed nothing when it loaded `file`.
    fn is_clean(&self, file: FileId) -> bool {
        self.unit.sm().load_report(file).is_clean()
    }
}

impl std::fmt::Debug for Sites<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sites").field("unit", self.unit).finish_non_exhaustive()
    }
}
