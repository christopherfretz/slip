//! The parse task: one observer through Kiln's front-end library entry
//! (`kiln_driver::front_end::run_front_end`) to its shard
//! (`spec/model.md` §8, §8.4, `spec/exec.md` §5.1). Work packages WP-010,
//! WP-025; the preprocessing record on request, WP-101; an overlay, WP-066.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use kiln_driver::args::Args;
use kiln_driver::front_end::{Content, FrontEnd, FrontEndError, FrontEndOptions, run_front_end};
use slip_model::formats::clip_message;
use slip_model::hash::content_digest;
use slip_model::{Counter, Digest, FileRef, Observer, Outcome, Phase, Shard, Status};

use crate::check::{
    front_end_line, kiln_switch_in_environment, not_modelled, program, utf8_command,
};
use crate::kiln::ast::Ast;
use crate::kiln::lex::{Interner, LexOptions};
use crate::kiln::oven::Oven;
use crate::kiln::pp::PpRecord;
use crate::kiln::source::fx::FxHashSet;
use crate::kiln::source::{
    DirCache, FileId, FileOrigin, Loc, Overlay, Resolved, SourceManager, WorkDir,
};
use crate::lookups::{Driver, Lookups};

/// What the front end of [`parse_observer_with`] keeps beyond what the
/// parse task needs. Every option is off by default, and the shard does not
/// depend on any of them: they only change what the closure can read
/// (`docs/wp/WP-101.md`, "Nothing is turned on").
///
/// `#[non_exhaustive]`: start from `ParseOptions::default()` and set the
/// fields wanted.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ParseOptions<'a> {
    /// Keep Kiln's preprocessing record (Kiln `FrontEndOptions::pp_record`,
    /// Kiln `spec/pp.md` "Preprocessing record"), which the closure reads
    /// as [`Unit::pp_record`].
    pub pp_record: bool,
    /// Record Kiln's written-reference table (Kiln `spec/ast.md` §18) and
    /// its declaration-extent table (§19), which the closure reads through
    /// [`crate::written`]. How the front end is asked is
    /// [`written_words`]'s to say.
    pub written: bool,
    /// Record Kiln's lookup log (Kiln `FrontEndOptions::record_lookups`,
    /// Kiln `spec/pp.md` "Lookup log"), which the closure reads as
    /// [`Unit::lookups`] (`spec/verify.md` §5.3, guard L1).
    pub lookups: bool,
    /// Replacement contents, by real path (`spec/edits.md` §7.1): the
    /// unit reads each file named here as these bytes, never as the
    /// disk's (Kiln's overlay, `kiln_source::Overlay`, given to the front
    /// end as `FrontEndOptions::overlay`). The paths must be absolute
    /// and exist. Sorted by path, no two equal. An entry is matched to
    /// the file the unit reads by file identity, not by its text: a
    /// consumer never matches an input to an entry by path, it asks
    /// [`Unit::overlaid`]. Empty (the default): the parse is the parse
    /// without one. The shard's `inputs` give an overlaid file's size and
    /// digest from these bytes; [`Unit::overlaid`] says which buffers are
    /// the overlay's.
    pub overlay: &'a [(&'a [u8], Arc<[u8]>)],
}

/// The words `options` adds to the front end's command line, right after
/// `-fsyntax-only` (so before any `--` of the command's own). They are
/// words of the line the front end is given, never of the plan: the
/// observer and its id do not change, and neither does the shard.
///
/// [`ParseOptions::written`] enables Kiln's written-reference table via
/// `FrontEndOptions::written_refs` (Kiln WP-2320, `front_end.rs:81`) and
/// its declaration-extent table via `-fkiln-decl-extents` (`args.rs:2182`,
/// which records the table and prints nothing; `FrontEndOptions` has no
/// `decl_extents` switch at Kiln `60fb5ab9d`).
fn written_words(options: ParseOptions<'_>) -> &'static [&'static str] {
    if options.written { &["-fkiln-decl-extents"] } else { &[] }
}

/// The token starts of the buffers a unit lexed ([`Unit::is_token_start`]).
#[derive(Default)]
struct TokenStarts(std::sync::Mutex<std::collections::HashMap<FileId, Arc<[u32]>>>);

impl TokenStarts {
    /// The starts of `file`, made by `lex` the first time.
    fn of(&self, file: FileId, lex: impl FnOnce() -> Vec<u32>) -> Arc<[u32]> {
        let mut map = self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        Arc::clone(map.entry(file).or_insert_with(|| lex().into()))
    }
}

/// The tokens of a unit as Kiln's lexer sees them: [`Unit::token_end`] and
/// [`Unit::is_token_start`] on a value that is `Sync` ([`Unit::tokens`]).
/// It borrows the unit; the token starts it lexes are kept in the unit.
#[derive(Clone, Copy)]
pub struct Tokens<'u> {
    sm: &'u SourceManager,
    interner: &'u Interner,
    lex: LexOptions,
    split_greaters: &'u FxHashSet<Loc>,
    starts: &'u TokenStarts,
}

impl std::fmt::Debug for Tokens<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tokens").finish_non_exhaustive()
    }
}

impl<'u> Tokens<'u> {
    /// The unit's source manager.
    pub fn sm(&self) -> &'u SourceManager {
        self.sm
    }

    /// The unit's identifier interner.
    pub fn interner(&self) -> &'u Interner {
        self.interner
    }

    /// The unit's lexer options.
    pub fn lex_options(&self) -> LexOptions {
        self.lex
    }

    /// [`Unit::token_end`]: Kiln's `kiln_lex::token_end_split` with the
    /// unit's `Ast::split_greaters` (Kiln WP-2542).
    pub fn token_end(&self, at: Loc) -> Option<Loc> {
        crate::kiln::lex::token_end_split(self.sm, self.interner, at, self.lex, self.split_greaters)
    }

    /// [`Unit::is_token_start`].
    pub fn is_token_start(&self, at: Loc) -> bool {
        if !at.is_valid() {
            return false;
        }
        // A `>` split off a `>>`, `>=` or `>>=` by the parser (Kiln
        // WP-2542), or the remainder token immediately following one in
        // the same entry (`>` of `>>`, `=` of `>=`, `>=` or `=` of `>>=`).
        if self.split_greaters.contains(&at) {
            return true;
        }
        if let Some(prev) = at.to_raw().checked_sub(1).map(Loc::from_raw)
            && self.split_greaters.contains(&prev)
            && self.sm.same_entry_distance(prev, at) == Some(1)
        {
            return true;
        }
        let at = if at.is_macro() { self.sm.spelling_loc(at) } else { at };
        if !at.is_valid() || at.is_macro() {
            return false;
        }
        let Some((file, pos)) = self.sm.offset_of(at) else { return false };
        self.starts.of(file, || self.lex_starts(file)).binary_search(&pos).is_ok()
    }

    /// The kinds of the tokens around bytes `lo..hi` of `file`, as Kiln's
    /// lexer lexes the buffer from its start (comments skipped): the last
    /// token that starts before `lo`, and the first that starts at or
    /// after `hi`. `None` at either end of the buffer.
    pub fn kinds_around(
        &self,
        file: FileId,
        lo: u32,
        hi: u32,
    ) -> (Option<crate::kiln::lex::TokKind>, Option<crate::kiln::lex::TokKind>) {
        let starts = self.starts.of(file, || self.lex_starts(file));
        let before = starts.partition_point(|&s| s < lo).checked_sub(1).map(|i| starts[i]);
        let after = starts.get(starts.partition_point(|&s| s < hi)).copied();
        (before.map(|s| self.kind_at(file, s)), after.map(|s| self.kind_at(file, s)))
    }

    /// The kind of the token at `at`, a token's location (a macro
    /// location is lexed where it is spelled). `None` when `at` is not
    /// the start of a token Kiln's lexer finds.
    pub fn kind_of(&self, at: Loc) -> Option<crate::kiln::lex::TokKind> {
        if !self.is_token_start(at) {
            return None;
        }
        let at = if at.is_macro() { self.sm.spelling_loc(at) } else { at };
        let (file, pos) = self.sm.offset_of(at)?;
        Some(self.kind_at(file, pos))
    }

    /// The kinds of the tokens just before `first` and just after `end`
    /// (a token's location and a token's exclusive end) in the token
    /// stream the parser read, after macro expansion: Kiln's expansion
    /// entries (`SourceManager::expansion_info`) are followed outward
    /// from a location that begins (or an end that ends) its entry, as
    /// Kiln's `file_loc_at_macro_start` and `file_loc_at_macro_end` do,
    /// to the parameter an argument replaced or the invocation a body
    /// replaced; elsewhere the neighbour is the token beside the spelled
    /// token. `None` at an end where that cannot be known: a neighbour
    /// that is a macro parameter's replacement (the spelled neighbour in
    /// the body is then the parameter's name, an identifier, which no
    /// caller takes for punctuation), an argument split over several
    /// entries, an unknown location, a buffer's end.
    pub fn expanded_kinds_around(
        &self,
        first: Loc,
        end: Loc,
    ) -> (Option<crate::kiln::lex::TokKind>, Option<crate::kiln::lex::TokKind>) {
        (self.expanded_before(first), self.expanded_after(end))
    }

    /// The bound on macro nesting the two walks follow.
    const EXPANSION_DEPTH: usize = 1024;

    /// [`Tokens::expanded_kinds_around`] before `at`.
    fn expanded_before(&self, mut at: Loc) -> Option<crate::kiln::lex::TokKind> {
        use crate::kiln::source::ExpansionKind;
        for _ in 0..Self::EXPANSION_DEPTH {
            if !at.is_macro() {
                let (file, pos) = self.sm.offset_of(at)?;
                return self.kinds_around(file, pos, pos).0;
            }
            let e = self.sm.expansion_info(at)?;
            if at != e.base {
                // Inside the entry: the token spelled before it, which the
                // entry holds too (its spelling is one run).
                let (file, pos) = self.sm.offset_of(self.sm.spelling_loc(at))?;
                let lo = pos.checked_sub(at.to_raw() - e.base.to_raw())?;
                let starts = self.starts.of(file, || self.lex_starts(file));
                let i = starts.partition_point(|&s| s < pos).checked_sub(1)?;
                return (starts[i] >= lo).then(|| self.kind_at(file, starts[i]));
            }
            if e.kind == ExpansionKind::MacroArg
                && let Some(prev) = e.base.to_raw().checked_sub(1).map(Loc::from_raw)
                && self.sm.expansion_info(prev).is_some_and(|p| p.expansion.lo == e.expansion.lo)
            {
                // An argument continued from the previous entry.
                return None;
            }
            // The first token of an argument stands where its parameter
            // was; of a body, where its invocation was.
            at = e.expansion.lo;
        }
        None
    }

    /// [`Tokens::expanded_kinds_around`] after `end`.
    fn expanded_after(&self, mut end: Loc) -> Option<crate::kiln::lex::TokKind> {
        use crate::kiln::source::ExpansionKind;
        for _ in 0..Self::EXPANSION_DEPTH {
            if !end.is_macro() {
                let (file, pos) = self.sm.offset_of(end)?;
                return self.kinds_around(file, pos, pos).1;
            }
            // An exclusive end belongs to the entry of its last byte.
            let e = self.sm.expansion_info(Loc::from_raw(end.to_raw().checked_sub(1)?))?;
            let last = e.base.with_offset(e.len);
            if end != last {
                let (file, pos) = self.sm.offset_of(self.sm.spelling_loc(end))?;
                let hi = pos + (last.to_raw() - end.to_raw());
                let starts = self.starts.of(file, || self.lex_starts(file));
                let next = *starts.get(starts.partition_point(|&s| s < pos))?;
                return (next < hi).then(|| self.kind_at(file, next));
            }
            if e.kind == ExpansionKind::MacroArg
                && let Some(next) = last.to_raw().checked_add(1).map(Loc::from_raw)
                && self.sm.expansion_info(next).is_some_and(|n| n.expansion.lo == e.expansion.lo)
            {
                // An argument continued in the next entry.
                return None;
            }
            end = e.expansion.hi;
        }
        None
    }

    /// The kind of the token that starts at byte `at` of `file` (a start
    /// of [`Tokens::lex_starts`]).
    fn kind_at(&self, file: FileId, at: u32) -> crate::kiln::lex::TokKind {
        use crate::kiln::lex::{LexMode, Lexer};
        let len = u32::try_from(self.sm.text(file).len()).unwrap_or(u32::MAX);
        let mut o = self.lex;
        o.keep_comments = false;
        let mut lx =
            Lexer::new_in_range(self.sm, self.interner, file, at..len, self.sm.loc(file, 0), o);
        lx.set_mode(LexMode::Normal);
        lx.next().kind
    }

    /// The offsets of the tokens of `file`, lexed from its start.
    fn lex_starts(&self, file: FileId) -> Vec<u32> {
        use crate::kiln::lex::{LexMode, Lexer, TokKind};
        let len = u32::try_from(self.sm.text(file).len()).unwrap_or(u32::MAX);
        let base = self.sm.loc(file, 0);
        let mut o = self.lex;
        o.keep_comments = false;
        let mut lx = Lexer::new_in_range(self.sm, self.interner, file, 0..len, base, o);
        lx.set_mode(LexMode::Normal);
        let mut out = Vec::new();
        loop {
            let t = lx.next();
            if t.kind == TokKind::Eof {
                break;
            }
            if let Some(d) = self.sm.same_entry_distance(base, t.span.lo) {
                out.push(d);
            }
        }
        out
    }
}

/// The finished front end of one observer, as the closure of
/// [`parse_observer_with`] sees it: valid inside the call only.
///
/// Its methods name Kiln's types (through [`crate::kiln`]): only the crates
/// that may name `slip_kiln::kiln` call them (`slip_digest`,
/// `slip_script`); any crate may pass a `&Unit` on.
pub struct Unit<'r> {
    ast: &'r Ast,
    sm: &'r SourceManager,
    oven: &'r Oven,
    pp_record: Option<&'r PpRecord>,
    lookups: Option<&'r Lookups>,
    /// The interner of the unit's identifiers and the lexer options of
    /// its preprocessor: what Kiln's lexer needs, read by
    /// [`Unit::token_end`] and [`Unit::is_token_start`] alone
    /// (`docs/wp/WP-084.md`).
    interner: &'r Interner,
    lex: LexOptions,
    /// The token starts of each buffer [`Unit::is_token_start`] lexed, by
    /// offset, ascending.
    token_starts: TokenStarts,
    main_file: Option<FileId>,
    /// Each buffer that is one of `inputs`, and the index of its entry.
    buffers: &'r [(FileId, usize)],
    inputs: &'r [FileRef],
    errors: u32,
}

impl<'r> Unit<'r> {
    /// The unit of a parsed snippet (`snippet::Parsed::unit`): no
    /// preprocessing record, no lookups, no inputs (so [`Unit::real_path`] is `None`
    /// for every buffer).
    pub(crate) fn of_snippet(
        ast: &'r Ast,
        sm: &'r SourceManager,
        oven: &'r Oven,
        interner: &'r Interner,
        lex: LexOptions,
        main_file: FileId,
        errors: u32,
    ) -> Unit<'r> {
        Unit {
            ast,
            sm,
            oven,
            pp_record: None,
            lookups: None,
            interner,
            lex,
            token_starts: TokenStarts::default(),
            main_file: Some(main_file),
            buffers: &[],
            inputs: &[],
            errors,
        }
    }

    /// The AST.
    pub fn ast(&self) -> &'r Ast {
        self.ast
    }

    /// The unit's source manager: every buffer read, every expansion.
    pub fn sm(&self) -> &'r SourceManager {
        self.sm
    }

    /// The constant evaluator's state.
    pub fn oven(&self) -> &'r Oven {
        self.oven
    }

    /// The preprocessing record (each macro definition, each expansion
    /// with its arguments), when [`ParseOptions::pp_record`] asked for it;
    /// `None` otherwise.
    ///
    /// It is the preprocessor's (Kiln `Preprocessor::record`), lent for the
    /// call like everything else here: the preprocessor and its record are
    /// dropped when the closure returns, and the record holds its
    /// definitions as `Rc`s, so it is neither `Send` nor `Sync`. Its
    /// locations are this unit's ([`Unit::sm`]) and its names are symbols
    /// of the AST's interner ([`Ast::symbols`]).
    pub fn pp_record(&self) -> Option<&'r PpRecord> {
        self.pp_record
    }

    /// What header search looked at ([`Lookups`]), when
    /// [`ParseOptions::lookups`] asked for it; `None` otherwise. Names no
    /// Kiln type: any crate may call it.
    pub fn lookups(&self) -> Option<&'r Lookups> {
        self.lookups
    }

    /// Kiln's `kiln_lex::token_end` with this unit's interner and lexer
    /// options: the end of the token that starts at `at`, or `None`
    /// (Kiln `spec/lex.md` "End of a token": `None` when no token starts
    /// at `at`, and for a macro location whose token would end outside
    /// its own expansion entry).
    pub fn token_end(&self, at: Loc) -> Option<Loc> {
        self.tokens().token_end(at)
    }

    /// What [`Unit::token_end`] and [`Unit::is_token_start`] read, as a
    /// value that is `Sync` (a `Unit` is not: its preprocessing record
    /// holds `Rc`s), for a reader that outlives the closure's borrow of
    /// the preprocessor but not the unit (`slip_script`'s subject, work
    /// package WP-086). The same two functions; the token starts it lexes
    /// are the unit's.
    pub fn tokens(&self) -> Tokens<'_> {
        Tokens {
            sm: self.sm,
            interner: self.interner,
            lex: self.lex,
            split_greaters: &self.ast.split_greaters,
            starts: &self.token_starts,
        }
    }

    /// Whether a token of the buffer starts at `at` as Kiln's lexer lexes
    /// the buffer: for a file location, `at`'s offset is the start of a
    /// token when the buffer is lexed from its start with this unit's
    /// lexer options in normal mode (comments skipped); for a macro
    /// location, the same of where it is spelled
    /// (`SourceManager::spelling_loc`). False for the invalid location.
    ///
    /// This is how the preprocessor lexes the buffer outside directive
    /// lines (Kiln's lexer lexes a skipped group to the same following
    /// tokens, `Lexer::skip_plain_line_rest`); inside a directive line the
    /// preprocessor lexes a header name, which normal mode does not.
    /// Each buffer is lexed once per unit, when first asked about.
    pub fn is_token_start(&self, at: Loc) -> bool {
        self.tokens().is_token_start(at)
    }

    /// The main file, once the front end entered it.
    pub fn main_file(&self) -> Option<FileId> {
        self.main_file
    }

    /// For a buffer that is one of the shard's `inputs`: that entry's
    /// `path` (the real path). `None` for any other buffer (the
    /// predefines, the command line's macros, a lazy buffer never read).
    pub fn real_path(&self, file: FileId) -> Option<&'r [u8]> {
        let at = self.buffers.binary_search_by_key(&file, |&(f, _)| f).ok()?;
        Some(&self.inputs[self.buffers[at].1].path)
    }

    /// The shard's `inputs` (every file read, real path, size, digest),
    /// for the unit's lifetime, so a handler may hold them.
    pub fn inputs(&self) -> &'r [FileRef] {
        self.inputs
    }

    /// Whether the unit read `file` from the overlay of
    /// [`ParseOptions::overlay`] (Kiln's `FileOrigin::Overlay`): its bytes
    /// are the overlay's, and so are the offsets of every location in it.
    ///
    /// Kiln matches an entry to a file by its identity, and the shard's
    /// input keeps the path the unit reached the file by, which need not
    /// be the entry's text (`#include "h.h"` reaching an entry `…/H.h` on
    /// a case-insensitive volume). So this is the only test of whether an
    /// input is overlaid: a consumer never compares an input's path with
    /// an entry's (`spec/edits.md` §7.1).
    pub fn overlaid(&self, file: FileId) -> bool {
        self.sm.file_origin(file) == FileOrigin::Overlay
    }

    /// Errors reported so far by the job's engine (when the closure is
    /// called: the parse's own, without the driver's).
    pub fn errors(&self) -> u32 {
        self.errors
    }

    /// The stack left on the calling thread under Kiln's stack guard
    /// (`kiln_sema::stack_guard::StackGuard::current().remaining()`), in
    /// bytes; `None` where no guard is installed (`spec/model.md` §8.4).
    /// Inside the closure of [`parse_observer_with`] the calling thread is
    /// the front end's, which has the guard.
    pub fn stack_remaining(&self) -> Option<usize> {
        kiln_sema::stack_guard::StackGuard::current().remaining()
    }
}

impl std::fmt::Debug for Unit<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Unit")
            .field("main_file", &self.main_file)
            .field("pp_record", &self.pp_record.is_some())
            .field("inputs", &self.inputs.len())
            .field("errors", &self.errors)
            .finish_non_exhaustive()
    }
}

/// Parses `observer`'s command with Kiln's front end in this process and
/// returns its shard for `phase` and `round` (`spec/model.md` §8.4).
///
/// `parse_observer_with` with the default options and a closure that does
/// nothing.
///
/// # Errors
///
/// As [`parse_observer_with`].
pub fn parse_observer(observer: &Observer, phase: Phase, round: u64) -> Result<Shard, String> {
    parse_observer_with(observer, phase, round, ParseOptions::default(), |_| ())
        .map(|(shard, _)| shard)
}

/// What a parse counts besides its inputs.
#[derive(Clone, Copy, Debug)]
struct Tally {
    /// The preprocessed tokens.
    tokens: u64,
    /// The inputs reached under a spelling other than their own on disk.
    respelled: u64,
}

/// The shard of `observer`'s parse: `parsed` is its inputs and its tally,
/// when it parsed. `inputs.respelled` is there only when it is not 0, so
/// that a shard on a volume that does not fold is what it was before it
/// (`spec/model.md` §8.4).
fn new_shard(
    observer: &Observer,
    phase: Phase,
    round: u64,
    outcome: Outcome,
    parsed: Option<(Vec<FileRef>, Tally)>,
) -> Shard {
    let (inputs, counters) = match parsed {
        Some((files, tally)) => {
            let mut counters = vec![
                Counter { name: "inputs.bytes".into(), value: files.iter().map(|f| f.size).sum() },
                Counter { name: "inputs.files".into(), value: files.len() as u64 },
            ];
            if tally.respelled != 0 {
                counters.push(Counter { name: "inputs.respelled".into(), value: tally.respelled });
            }
            counters.push(Counter { name: "pp.tokens".into(), value: tally.tokens });
            (Some(files), counters)
        }
        None => (None, Vec::new()),
    };
    Shard {
        observer: observer.id,
        phase,
        round,
        outcome,
        inputs,
        counters,
        digests: None,
        verdict: None,
        edits: None,
        binding: None,
        rule_use: None,
        spellings: None,
        names: None,
        negatives: None,
        found: None,
        rebuilt_use: None,
    }
}

/// Parses `observer`'s command with Kiln's front end in this process;
/// when a parse happened, calls `f` on the finished unit (with errors or
/// without) before anything is dropped. Returns the shard for `phase` and
/// `round` and `f`'s result (`None`: no parse happened).
///
/// The front end runs `argv[0] -fsyntax-only -fkiln-reproducible-time
/// argv[1..]` (`spec/cdb.md` §6.1; a scanner name
/// `clang-scan-deps…` as `kiln`) through
/// `kiln_driver::front_end::run_front_end`, on a thread of its own, with
/// nothing rendered and nothing written, keeping what `options` asks for
/// and nothing else (the shard is the same whatever they are). The inputs are the source
/// manager's buffers read from disk and the paths the unit consulted
/// without entering them (`spec/model.md` §8.4 "Inputs").
///
/// The command's `directory` is passed per run to Kiln
/// (`FrontEndOptions::working_directory`, Kiln WP-2121): the process's
/// current directory is never changed, so several parses in different
/// directories may run concurrently in one process.
///
/// # Errors
///
/// Only a failure of the worker itself: an entry of the overlay names no
/// regular file, or names the file of another entry (before the parse);
/// the directory does not exist or is not a directory, an input cannot be
/// resolved to its real path, a consulted file cannot be read, a file
/// changed during the parse, the overlay failed the run (Kiln's
/// `FrontEndReport::overlay_failures`), the front end or `f` panicked, or
/// the front end's thread could not be started. Diagnostics about the
/// subject are a shard with status `diagnosed` or `rejected`.
pub fn parse_observer_with<R: Send>(
    observer: &Observer,
    phase: Phase,
    round: u64,
    options: ParseOptions<'_>,
    f: impl FnOnce(&Unit<'_>) -> R + Send,
) -> Result<(Shard, Option<R>), String> {
    parse_observer_hooked(observer, phase, round, options, f, |_| {})
}

/// [`parse_observer_with`] with a hook run on the finished front end
/// immediately before [`collect_inputs`] (used by tests to mutate a file
/// between parsing and input collection).
fn parse_observer_hooked<R: Send>(
    observer: &Observer,
    phase: Phase,
    round: u64,
    options: ParseOptions<'_>,
    f: impl FnOnce(&Unit<'_>) -> R + Send,
    before_collect: impl FnOnce(&FrontEnd<'_, '_>) + Send,
) -> Result<(Shard, Option<R>), String> {
    let shard = |outcome, parsed| new_shard(observer, phase, round, outcome, parsed);

    let (argv0, rest) = match utf8_command(&observer.command) {
        Ok(words) => words,
        Err(arg) => {
            return Ok((shard(rejected(&format!("argument `{arg}` is not UTF-8")), None), None));
        }
    };

    let (overlay, overlaid_real) = match overlay(options.overlay)? {
        Some((ovl, real)) => (Some(ovl), real),
        None => (None, Vec::new()),
    };
    let work_dir_path = resolve_work_dir(&observer.command.directory)?;

    let mut line = front_end_line(&rest);
    let words = written_words(options);
    line.splice(1..1, words.iter().map(|&w| w.to_owned()));
    let program = program(&argv0);
    // `fe_options` disables response-file expansion (`response_files = false`),
    // so the driver sees `line` verbatim without reading any `@file`.
    let args = Args::parse_as(&program, &line);
    if let Some(flag) = not_modelled(&args) {
        return Ok((shard(rejected(&not_modelled_message(flag)), None), None));
    }
    if let Some(var) = kiln_switch_in_environment() {
        return Ok((shard(rejected(&switch_message(var)), None), None));
    }
    let dir: &Path = &work_dir_path;
    let (program_ref, line_ref) = (&program, &line);
    let overlay_ref = overlay.as_deref();
    let overlaid_real_ref: &[(Vec<u8>, Arc<[u8]>)] = &overlaid_real;
    let see = move |fe: &FrontEnd<'_, '_>| -> Result<(Vec<FileRef>, Tally, R), String> {
        before_collect(fe);
        let effective_dir = fe.sm.work_dir().dir().unwrap_or(dir);
        let (inputs, buffers, respelled) =
            collect_inputs(fe, effective_dir, overlay_ref, overlaid_real_ref)?;
        let lookups = Lookups::of(fe, || driver(program_ref, line_ref));
        let unit = Unit {
            ast: fe.ast,
            sm: fe.sm,
            oven: fe.oven,
            pp_record: fe.pp.record(),
            lookups: lookups.as_ref(),
            interner: fe.interner,
            lex: *fe.pp.lex_options(),
            token_starts: TokenStarts::default(),
            main_file: fe.main_file,
            buffers: &buffers,
            inputs: &inputs,
            errors: fe.diags.error_count(),
        };
        let value = f(&unit);
        Ok((inputs, Tally { tokens: fe.tokens, respelled }, value))
    };
    let report = match run_front_end(
        &program,
        &line,
        fe_options(options, overlay.clone(), work_dir_path.clone()),
        see,
    ) {
        Ok(report) => report,
        Err(e @ (FrontEndError::NotSyntaxOnly | FrontEndError::NotOneInput)) => {
            return Ok((shard(rejected(&e.to_string()), None), None));
        }
        // `Panicked`, `Thread`, and any reason Kiln adds (`FrontEndError`
        // is `#[non_exhaustive]` at Kiln `4a83f6e3c`): a failure of the
        // worker until it is classified here.
        Err(e) => return Err(e.to_string()),
    };
    // "A run with any is void" (Kiln `FrontEndReport::overlay_failures`):
    // a lazy entry's content failed to load. Slip's entries are bytes, so
    // none is expected; a run with one is never a shard.
    if let Some((path, why)) = report.overlay_failures.first() {
        return Err(format!("the overlay's {} failed to load: {why}", path.display()));
    }
    let parsed = report.value.transpose()?;

    let errors = u64::from(report.errors);
    let warnings = u64::from(report.warnings);
    let outcome = if errors == 0 {
        if parsed.is_none() {
            return Err("the front end did not parse and reported no error".into());
        }
        Outcome { status: Status::Ok, errors, warnings, message: String::new() }
    } else {
        let first = report.first_error.as_ref();
        let driver = first.and_then(|e| e.id).is_some_and(|id| id.name().starts_with("drv_"));
        let message = match first {
            Some(e) => match &e.at {
                Some(at) => format!("{}:{}:{}: {}", at.file, at.line, at.column, e.message),
                None => e.message.clone(),
            },
            None => format!("the front end reported {errors} errors and kept none"),
        };
        Outcome {
            status: if driver { Status::Rejected } else { Status::Diagnosed },
            errors,
            warnings,
            message: clip_message(&message).to_owned(),
        }
    };
    match parsed {
        Some((inputs, tally, value)) => Ok((shard(outcome, Some((inputs, tally))), Some(value))),
        None => Ok((shard(outcome, None), None)),
    }
}

/// Verifies that `directory` exists and is a directory and resolves it to an
/// absolute path for [`FrontEndOptions::working_directory`].
fn resolve_work_dir(directory: &[u8]) -> Result<PathBuf, String> {
    let dir = bytes_path(directory)?;
    let meta =
        std::fs::metadata(&dir).map_err(|e| format!("cannot enter {}: {e}", dir.display()))?;
    if !meta.is_dir() {
        return Err(format!("cannot enter {}: not a directory", dir.display()));
    }
    let work_dir = WorkDir::at(&dir).map_err(|e| format!("cannot enter {}: {e}", dir.display()))?;
    Ok(work_dir.dir().unwrap_or(&dir).to_path_buf())
}

/// Kiln's options for `options`, with `overlay` (that of
/// `options.overlay`) and the run's `working_directory`.
///
/// `response_files` is turned off (`false`, Kiln WP-2411) so that a parse
/// never reads an `@file` response file that would be absent from the
/// shard's `inputs` (`slip_cdb::normalise` already expands response files
/// when building a plan).
fn fe_options(
    options: ParseOptions<'_>,
    overlay: Option<Arc<Overlay>>,
    working_directory: PathBuf,
) -> FrontEndOptions {
    let mut fe = FrontEndOptions::default();
    fe.response_files = false;
    fe.pp_record = options.pp_record;
    fe.written_refs = options.written;
    fe.record_lookups = options.lookups;
    fe.working_directory = Some(working_directory);
    fe.overlay = overlay;
    fe
}

/// Kiln's overlay of `entries` (`None` when there are none: the parse is
/// the parse without one). Each path must name an existing regular file
/// (Kiln `Overlay::replace`); two entries naming one file (one identity,
/// whatever the spelling: a symbolic or a hard link) are refused here,
/// since Kiln lets the second replace the first. Entries out of order or
/// repeated are refused too (the interface asks for them sorted, no two
/// equal).
type OverlaidReal = Vec<(Vec<u8>, Arc<[u8]>)>;

fn overlay(entries: &[(&[u8], Arc<[u8]>)]) -> Result<Option<(Arc<Overlay>, OverlaidReal)>, String> {
    if entries.is_empty() {
        return Ok(None);
    }
    let mut overlay = Overlay::new();
    let mut overlaid_real = Vec::with_capacity(entries.len());
    for (i, (path, bytes)) in entries.iter().enumerate() {
        let shown = String::from_utf8_lossy(path);
        if i > 0 && entries[i - 1].0 >= *path {
            return Err(format!(
                "overlay: {shown} is not after the entry before it (sorted, no two equal)"
            ));
        }
        let at = bytes_path(path)?;
        // A relative entry would be resolved against the process's
        // directory, not the command's, and could overlay a file the unit
        // never reads under that name (second review F7).
        if !at.is_absolute() {
            return Err(format!("overlay: {shown} is not an absolute path"));
        }
        overlay
            .replace(&at, Arc::clone(bytes))
            .map_err(|e| format!("overlay: cannot replace {shown}: {e}"))?;
        if overlay.len() != i + 1 {
            return Err(format!("overlay: {shown} and an entry before it are one file"));
        }
        let on_disk = slip_spelling::real_path(&at)
            .map_err(|e| format!("overlay: cannot resolve {shown}: {}", e.detail()))?;
        overlaid_real.push((path_bytes(&on_disk), Arc::clone(bytes)));
    }
    overlaid_real.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(Some((Arc::new(overlay), overlaid_real)))
}

/// What the lookup log does not name (the `--embed-dir` directories, the
/// command's own search directories), from the front end's line as Kiln's
/// driver parses it, as `run_front_end` reads it (with `response_files` off).
fn driver(program: &str, line: &[String]) -> Driver {
    Driver::of(&Args::parse_as(program, line))
}

/// The message of a unit refused by [`kiln_switch_in_environment`].
pub(crate) fn switch_message(var: &str) -> String {
    format!("{var} is set: Slip does not parse under a Kiln check it does not model")
}

/// The message of a unit refused by [`not_modelled`].
fn not_modelled_message(flag: &str) -> String {
    format!("{flag} is not supported by Slip (spec/cdb.md §6.2)")
}

fn rejected(message: &str) -> Outcome {
    Outcome {
        status: Status::Rejected,
        errors: 1,
        warnings: 0,
        message: clip_message(message).to_owned(),
    }
}

/// One candidate input: its real path, size and digest, and the buffer it
/// came from (`None` for a consulted path read here).
type Candidate = (Vec<u8>, u64, Digest, Option<FileId>);

/// Each buffer that is an input, and the index of its entry.
type Buffers = Vec<(FileId, usize)>;

/// The unit's inputs (`spec/model.md` §8.4 "Inputs"), sorted by path and
/// unique, and each buffer among them with the index of its entry, sorted
/// by buffer. Every buffer's size and digest come directly from the bytes
/// Kiln read (`SourceManager::original_bytes`). For a consulted path that
/// is not a buffer (`#embed`, `__has_embed`), Kiln retains only its
/// 128-bit FNV-1a [`Content`] (`Consulted::content`), not the raw bytes:
/// an overlaid path uses the overlay's bytes and a disk path is read once
/// here for BLAKE3, and both are verified against `c.content` so that a
/// file changed between the parse and collection is rejected (`Err`)
/// rather than recorded with bytes the parse never saw (WP-148 item 2).
fn collect_inputs(
    fe: &FrontEnd<'_, '_>,
    directory: &Path,
    overlay: Option<&Overlay>,
    overlaid_real: &[(Vec<u8>, Arc<[u8]>)],
) -> Result<(Vec<FileRef>, Buffers, u64), String> {
    let sm = fe.sm;
    let mut found: Vec<Candidate> = Vec::new();
    let mut respelled_paths: std::collections::BTreeSet<Vec<u8>> =
        std::collections::BTreeSet::new();
    for file in sm.file_ids() {
        let bytes = match sm.file_origin(file) {
            FileOrigin::Virtual => continue,
            FileOrigin::Disk | FileOrigin::Overlay => sm.original_bytes(file).ok_or_else(|| {
                format!("buffer {} has no bytes although read from disk", sm.file_name(file))
            })?,
            // Only from a module file or a precompiled header: listed when
            // its file was read, else nothing read it.
            FileOrigin::Lazy => match sm.original_bytes(file) {
                Some(bytes) => bytes,
                None => continue,
            },
        };
        let (real, respelled) = real_path(directory, sm.file_name(file))?;
        if respelled {
            respelled_paths.insert(real.clone());
        }
        found.push((real, bytes.len() as u64, content_digest(&bytes), Some(file)));
    }
    let buffers_end = found.len();
    for c in fe.consulted {
        let (real, respelled) = real_path(directory, &c.path)?;
        if respelled {
            respelled_paths.insert(real.clone());
        }
        // A `__has_include` hit is a buffer; an `#embed` resource is not.
        // Note: this cross-check against `found[..buffers_end]` is dead code
        // at Kiln `60fb5ab9d` and becomes live only once Kiln reports every
        // read (`kiln_driver/src/front_end.rs:506-510, :519, :525` drops
        // `#embed` of a file the preprocessor entered and keeps only the first
        // content of a path read twice). Kept as a defensive check for then.
        if let Some((_, _, _, Some(buf))) = found[..buffers_end].iter().find(|(p, ..)| *p == real) {
            if let Some(bytes) = sm.original_bytes(*buf) {
                verify_consulted_content(&real, &bytes, c.content)?;
            }
            continue;
        }
        let (size, content) = match overlaid_bytes(overlay, overlaid_real, &real)? {
            Some(bytes) => {
                verify_consulted_content(&real, &bytes, c.content)?;
                (bytes.len() as u64, content_digest(&bytes))
            }
            None => read_consulted(&real, c.content)?,
        };
        found.push((real, size, content, None));
    }
    let (inputs, buffers) = merge_inputs(found)?;
    Ok((inputs, buffers, respelled_paths.len() as u64))
}

/// Checks that `bytes` collected for consulted path `real` match the
/// [`Content`] (size and 128-bit FNV-1a digest) that Kiln recorded when
/// it read the file during parsing.
fn verify_consulted_content(
    real: &[u8],
    bytes: &[u8],
    seen: Option<Content>,
) -> Result<(), String> {
    if let Some(seen) = seen {
        let got = Content::of(bytes);
        if got != seen {
            return Err(format!(
                "{} changed during the parse: the front end read {} bytes (digest {:032x}), \
                 collection read {} bytes (digest {:032x})",
                String::from_utf8_lossy(real),
                seen.size,
                seen.digest,
                got.size,
                got.digest,
            ));
        }
    }
    Ok(())
}

/// The overlay's bytes for the file at `real`, if the overlay replaces it.
/// Looks up `real` in `overlaid_real` (the canonical on-disk real paths
/// resolved when the overlay was constructed before the parse) first so
/// that an atomic replacement on disk — even one that reuses another
/// overlaid file's freed inode — still maps `real` to its own overlaid
/// bytes; falls back to Kiln's identity-based `Overlay::resolve` for
/// distinct real paths that share an inode (hard links).
fn overlaid_bytes(
    overlay: Option<&Overlay>,
    overlaid_real: &[(Vec<u8>, Arc<[u8]>)],
    real: &[u8],
) -> Result<Option<Arc<[u8]>>, String> {
    let Some(overlay) = overlay else { return Ok(None) };
    if let Ok(i) = overlaid_real.binary_search_by_key(&real, |(p, _)| p.as_slice()) {
        return Ok(Some(Arc::clone(&overlaid_real[i].1)));
    }
    match overlay.resolve(&bytes_path(real)?, &mut DirCache::default()) {
        Resolved::Disk { replaced: Some(c), .. } | Resolved::Created { content: c, .. } => {
            c.bytes().map(Some).map_err(|e| {
                format!("the overlay's {} failed to load: {e}", String::from_utf8_lossy(real))
            })
        }
        _ => Ok(None),
    }
}

/// `found` sorted by path and made unique: two candidates of one path
/// that differ are a file that changed during the parse.
fn merge_inputs(mut found: Vec<Candidate>) -> Result<(Vec<FileRef>, Buffers), String> {
    found.sort_by(|a, b| a.0.cmp(&b.0));

    let mut inputs: Vec<FileRef> = Vec::with_capacity(found.len());
    let mut buffers: Buffers = Vec::new();
    for (path, size, content, file) in found {
        match inputs.last() {
            Some(last) if last.path == path => {
                if (last.size, last.content) != (size, content) {
                    return Err(format!(
                        "{} changed during the parse: two buffers of it differ",
                        String::from_utf8_lossy(&path)
                    ));
                }
            }
            _ => inputs.push(FileRef { path, size, content }),
        }
        if let Some(file) = file {
            buffers.push((file, inputs.len() - 1));
        }
    }
    buffers.sort_unstable();
    Ok((inputs, buffers))
}

/// The size and digest of a consulted file (`real`: its real path), read
/// here once and verified against Kiln's recorded `seen` [`Content`]: Kiln
/// keeps only the 128-bit FNV-1a [`Content`] of an `#embed` resource or a
/// `__has_embed` hit, not its raw bytes. Failing to read it or a content
/// mismatch is `Err`.
fn read_consulted(real: &[u8], seen: Option<Content>) -> Result<(u64, Digest), String> {
    let bytes = std::fs::read(bytes_path(real)?)
        .map_err(|e| format!("cannot read {}: {e}", String::from_utf8_lossy(real)))?;
    verify_consulted_content(real, &bytes, seen)?;
    Ok((bytes.len() as u64, content_digest(&bytes)))
}

/// `name` resolved against `directory`, then to its real path, spelled
/// as its directories list it (`spec/model.md` §8.4 "Inputs",
/// `spec/edits.md` §2.1 "One spelling"); and whether that spelling is not
/// the one `canonicalize` kept (the name reached the file under another
/// case or normalisation: `inputs.respelled`).
fn real_path(directory: &Path, name: &str) -> Result<(Vec<u8>, bool), String> {
    let spelled = directory.join(name);
    // canonicalize: first half of real_path to count inputs.respelled (spec/edits.md §2.1)
    let real = std::fs::canonicalize(&spelled)
        .map_err(|e| format!("cannot resolve {}: {e}", spelled.display()))?;
    let real = path_bytes(&real);
    let on_disk = slip_spelling::disk()
        .on_disk(&real)
        .map_err(|e| format!("cannot resolve {}: {}", spelled.display(), e.detail()))?;
    let respelled = on_disk != real;
    Ok((on_disk, respelled))
}

#[cfg(unix)]
fn path_bytes(p: &Path) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    p.as_os_str().as_bytes().to_vec()
}

#[cfg(not(unix))]
fn path_bytes(p: &Path) -> Vec<u8> {
    p.to_string_lossy().into_owned().into_bytes()
}

#[cfg(unix)]
#[allow(clippy::unnecessary_wraps, reason = "fallible on other platforms")]
fn bytes_path(b: &[u8]) -> Result<PathBuf, String> {
    use std::os::unix::ffi::OsStrExt;
    Ok(PathBuf::from(std::ffi::OsStr::from_bytes(b)))
}

#[cfg(not(unix))]
fn bytes_path(b: &[u8]) -> Result<PathBuf, String> {
    std::str::from_utf8(b)
        .map(PathBuf::from)
        .map_err(|_| format!("path {} is not UTF-8", String::from_utf8_lossy(b)))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};

    use slip_model::hash::content_digest;
    use slip_model::{Command, Digest, FileRef, Language, Observer, Origin, Phase, Status};

    use super::{ParseOptions, merge_inputs, parse_observer_hooked, path_bytes, read_consulted};

    struct TempDir(PathBuf);

    impl std::ops::Deref for TempDir {
        type Target = std::path::Path;
        fn deref(&self) -> &std::path::Path {
            &self.0
        }
    }

    impl AsRef<std::path::Path> for TempDir {
        fn as_ref(&self) -> &std::path::Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn test_dir(name: &str) -> TempDir {
        static N: AtomicU64 = AtomicU64::new(0);
        let root =
            std::env::var_os("SLIP_TEST_ROOT").map_or_else(std::env::temp_dir, PathBuf::from);
        let dir = root.join(format!(
            "slip-kiln-parse-unit-{name}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        TempDir(std::fs::canonicalize(&dir).unwrap())
    }

    fn test_observer(dir: &std::path::Path, argv: &[&str]) -> Observer {
        Observer {
            id: Digest([7; 32]),
            command: Command {
                directory: path_bytes(dir),
                argv: argv.iter().map(|a| a.as_bytes().to_vec()).collect(),
                env: Vec::new(),
            },
            language: Language::Cxx,
            main: FileRef { path: Vec::new(), size: 0, content: Digest([0; 32]) },
            cost_hint: 1,
            origins: vec![Origin { configuration: 0, entry: 0 }],
        }
    }

    // spec/model.md §8.4 "Inputs" (WP-066): two buffers of one real path
    // that differ are `Err`; the same bytes twice are one entry. A file
    // cannot be made to change between two reads of one parse from
    // outside Kiln, so the rule is checked here, on the candidates.
    #[test]
    fn s8_4_inputs_a_file_changed_during_the_parse_is_err() {
        let one = content_digest(b"one");
        let two = content_digest(b"two");
        let path = b"/t/h.h".to_vec();
        let same = vec![(path.clone(), 3, one, None), (path.clone(), 3, one, None)];
        let (inputs, _) = merge_inputs(same).expect("one file read twice");
        assert_eq!(inputs.len(), 1);
        let changed = vec![(path.clone(), 3, one, None), (path, 3, two, None)];
        let e = merge_inputs(changed).unwrap_err();
        assert!(e.contains("changed during the parse"), "{e}");
    }

    // spec/model.md §8.4 "Inputs": a consulted path that cannot be read
    // here (a directory, a file gone since Kiln read it) is `Err`.
    #[test]
    fn s8_4_inputs_unreadable_consulted_path_is_err() {
        let dir = std::env::temp_dir();
        let e = read_consulted(&path_bytes(&dir), None).unwrap_err();
        assert!(e.starts_with("cannot read "), "{e}");
        let gone = dir.join(format!("slip-kiln-gone-{}", std::process::id()));
        let e = read_consulted(&path_bytes(&gone), None).unwrap_err();
        assert!(e.starts_with("cannot read "), "{e}");
    }

    // WP-173 item 4 (Kiln WP-3640, image format 101 -> 102): a module
    // declaration image written at the old pin's format version (101) is
    // refused by Kiln's reader and never read, while one at the current
    // format version (102) imports cleanly.
    #[test]
    fn wp173_old_module_image_version_is_refused() {
        use std::io;

        use kiln_driver::{Io, SharedWriter};

        let base = test_dir("old-image");
        let iface = base.join("m.cppm");
        let user = base.join("u.cpp");
        let pcm = base.join("m.pcm");
        let old_pcm = base.join("m_old.pcm");
        std::fs::write(&iface, "export module m;\nexport int v = 42;\n").unwrap();
        std::fs::write(&user, "import m;\nint f() { return v; }\n").unwrap();

        let precompile_args: Vec<String> = [
            "-std=c++20",
            "-fkiln-bmi-image",
            "-fkiln-reproducible-time",
            "--precompile",
            iface.to_str().unwrap(),
            "-o",
            pcm.to_str().unwrap(),
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        let io = Io {
            stdout: Box::new(io::sink()),
            stderr: SharedWriter::new(io::sink()),
            stdin: Box::new(io::empty()),
        };
        let code = kiln_driver::run_as("kiln++", &precompile_args, io);
        assert_eq!(code, 0, "precompiling module interface failed");

        let mut capsule = kiln_pp::bmi::read(&std::fs::read(&pcm).unwrap()).unwrap();
        let img = capsule
            .sections
            .iter_mut()
            .find(|s| s.tag == kiln_pp::bmi::SECTION_IMAGE)
            .expect("precompiled module carries SECTION_IMAGE");
        assert_eq!(&img.bytes[..4], &102u32.to_le_bytes(), "expected current image format 102");

        let parse_with_pcm = |pcm_path: &std::path::Path| {
            let fmod = format!("-fmodule-file=m={}", pcm_path.display());
            let obs =
                test_observer(&base, &["c++", "-std=c++20", "-fkiln-bmi-image", &fmod, "u.cpp"]);
            super::parse_observer(&obs, Phase::Map, 3).expect("parse_observer")
        };

        // Current format (102) imports cleanly.
        let good = parse_with_pcm(&pcm);
        assert_eq!(good.outcome.status, Status::Ok, "{:?}", good.outcome);
        assert_eq!(good.outcome.errors, 0, "{:?}", good.outcome);

        // Old pin's format (101) is refused as unsupported, never read.
        img.bytes[..4].copy_from_slice(&101u32.to_le_bytes());
        std::fs::write(&old_pcm, kiln_pp::bmi::write(&capsule)).unwrap();
        let bad = parse_with_pcm(&old_pcm);
        assert_eq!(bad.outcome.status, Status::Diagnosed, "{:?}", bad.outcome);
        assert!(bad.outcome.errors >= 1, "{:?}", bad.outcome);
        assert!(
            bad.outcome.message.contains("unsupported version"),
            "expected unsupported version refusal, got {:?}",
            bad.outcome
        );
    }

    // WP-148 item 2 (F9): modifying an included header on disk between the
    // parse and `collect_inputs` leaves the shard recording the parsed bytes
    // (from `SourceManager::original_bytes`), never the mutated disk bytes.
    #[test]
    fn s8_4_inputs_buffer_mutated_before_collect_records_parsed_bytes() {
        let dir = test_dir("buf-mut");
        let header = dir.join("h.h");
        let parsed_bytes = b"int x = 1;\n";
        let mutated_bytes = b"int x = 99999;\n";
        std::fs::write(&header, parsed_bytes).unwrap();
        std::fs::write(dir.join("main.cpp"), "#include \"h.h\"\nint y = x;\n").unwrap();
        let obs = test_observer(&dir, &["c++", "-std=c++23", "main.cpp"]);
        let header_hook = header.clone();
        let (shard, _) = parse_observer_hooked(
            &obs,
            Phase::Map,
            0,
            ParseOptions::default(),
            |_| (),
            move |_| {
                std::fs::write(&header_hook, mutated_bytes).unwrap();
            },
        )
        .expect("buffer bytes come from SourceManager");
        assert_eq!(shard.outcome.status, Status::Ok);
        let inputs = shard.inputs.expect("inputs");
        let h_path = path_bytes(&header);
        let h_ref = inputs.iter().find(|f| f.path == h_path).expect("header input");
        assert_eq!(h_ref.size, parsed_bytes.len() as u64);
        assert_eq!(h_ref.content, content_digest(parsed_bytes));
        assert_ne!(h_ref.content, content_digest(mutated_bytes));
        let _ = std::fs::remove_dir_all(&dir);
    }

    // WP-148 item 2 (F9): modifying an `#embed` consulted file on disk
    // between the parse and `collect_inputs` is detected via Kiln's
    // `Consulted::content` and fails closed (`Err`), never recording the
    // mutated bytes in the shard.
    #[test]
    fn s8_4_inputs_consulted_embed_mutated_before_collect_fails_closed() {
        let dir = test_dir("embed-mut");
        let data = dir.join("data.bin");
        let parsed_bytes = b"ABCD";
        let mutated_bytes = b"WXYZ_CHANGED";
        std::fs::write(&data, parsed_bytes).unwrap();
        std::fs::write(
            dir.join("main.cpp"),
            "const unsigned char d[] = {\n#embed \"data.bin\"\n};\n",
        )
        .unwrap();
        let obs = test_observer(&dir, &["c++", "-std=c++26", "main.cpp"]);
        let data_hook = data.clone();
        let err = parse_observer_hooked(
            &obs,
            Phase::Map,
            0,
            ParseOptions::default(),
            |_| (),
            move |_| {
                std::fs::write(&data_hook, mutated_bytes).unwrap();
            },
        )
        .expect_err("mutated consulted file must be rejected");
        assert!(err.contains("changed during the parse"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // WP-148 item 2 (F9): through an overlay, modifying the underlying disk
    // files (both an `#include` buffer and an `#embed` consulted file, both
    // in-place and by atomic rename replacing the inode) between parse and
    // `collect_inputs` still records the overlaid bytes in the shard.
    #[test]
    fn s8_4_inputs_overlaid_files_mutated_before_collect_record_overlaid_bytes() {
        for atomic_replace in [false, true] {
            let dir = test_dir(if atomic_replace { "ovl-rename" } else { "ovl-inplace" });
            let header = dir.join("h.h");
            let data = dir.join("data.bin");
            std::fs::write(&header, b"int x = 0;\n").unwrap();
            std::fs::write(&data, b"DISK").unwrap();
            std::fs::write(
                dir.join("main.cpp"),
                "#include \"h.h\"\nconst unsigned char d[] = {\n#embed \"data.bin\"\n};\nint y = x;\n",
            )
            .unwrap();
            let ovl_header: Arc<[u8]> = Arc::from(&b"int x = 42;\n"[..]);
            let ovl_data: Arc<[u8]> = Arc::from(&b"OVL!"[..]);
            let mut entries = [
                (path_bytes(&header), Arc::clone(&ovl_header)),
                (path_bytes(&data), Arc::clone(&ovl_data)),
            ];
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            let slice: Vec<(&[u8], Arc<[u8]>)> =
                entries.iter().map(|(p, b)| (p.as_slice(), Arc::clone(b))).collect();
            let opts = ParseOptions { overlay: &slice, ..ParseOptions::default() };

            let obs = test_observer(&dir, &["c++", "-std=c++26", "main.cpp"]);
            let h_hook = header.clone();
            let d_hook = data.clone();
            let tmp_h = dir.join("h.h.tmp");
            let tmp_d = dir.join("data.bin.tmp");
            let (shard, _) = parse_observer_hooked(
                &obs,
                Phase::Map,
                0,
                opts,
                |_| (),
                move |_| {
                    if atomic_replace {
                        std::fs::write(&tmp_h, b"int x = 777;\n").unwrap();
                        std::fs::rename(&tmp_h, &h_hook).unwrap();
                        std::fs::write(&tmp_d, b"REPLACED_ON_DISK").unwrap();
                        std::fs::rename(&tmp_d, &d_hook).unwrap();
                    } else {
                        std::fs::write(&h_hook, b"int x = 777;\n").unwrap();
                        std::fs::write(&d_hook, b"MUTATED_ON_DISK").unwrap();
                    }
                },
            )
            .expect("overlaid inputs must record overlaid bytes");
            assert_eq!(shard.outcome.status, Status::Ok);
            let inputs = shard.inputs.expect("inputs");
            let h_ref = inputs.iter().find(|f| f.path == path_bytes(&header)).expect("header");
            assert_eq!(h_ref.size, ovl_header.len() as u64);
            assert_eq!(h_ref.content, content_digest(&ovl_header));
            let d_ref = inputs.iter().find(|f| f.path == path_bytes(&data)).expect("data");
            assert_eq!(d_ref.size, ovl_data.len() as u64);
            assert_eq!(d_ref.content, content_digest(&ovl_data));
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    // WP-148 fix round 1 (item 4): `fe_options` sets `response_files = false`,
    // so an `@file.rsp` word in `argv` is never expanded as a response file
    // (and thus never reads an unlisted input from disk).
    #[test]
    fn s8_4_response_file_in_argv_is_not_expanded() {
        let dir = test_dir("no-rsp");
        std::fs::write(dir.join("flags.rsp"), "-DFROM_RSP=1\n").unwrap();
        std::fs::write(dir.join("imacros.rsp"), "-imacros m.h\n").unwrap();
        std::fs::write(dir.join("m.h"), "#define FROM_IMACROS 1\n").unwrap();
        std::fs::write(dir.join("main.cpp"), "int x = FROM_RSP;\n").unwrap();
        // Bare `@flags.rsp` beside `main.cpp`: treated as a second literal
        // input (not expanded), so Kiln refuses it with `NotOneInput`.
        for rsp_arg in ["@flags.rsp", "@imacros.rsp"] {
            let obs = test_observer(&dir, &["c++", "-std=c++20", rsp_arg, "main.cpp"]);
            let shard = super::parse_observer(&obs, Phase::Map, 0).expect("parse_observer");
            assert_eq!(
                shard.outcome.status,
                Status::Rejected,
                "{rsp_arg} must not be expanded: {:?}",
                shard.outcome
            );
            assert!(
                shard.outcome.message.contains("has not one source input"),
                "{rsp_arg}: {:?}",
                shard.outcome
            );
        }
        // `@flags.rsp` passed as a separate value to `-I`: kept as a literal
        // directory name, never expanded to `-DFROM_RSP=1`, so `FROM_RSP` stays
        // undeclared (`Diagnosed`, not `Ok`).
        let obs = test_observer(&dir, &["c++", "-std=c++20", "-I", "@flags.rsp", "main.cpp"]);
        let shard = super::parse_observer(&obs, Phase::Map, 0).expect("parse_observer");
        assert_eq!(
            shard.outcome.status,
            Status::Diagnosed,
            "-I @flags.rsp must not expand flags.rsp: {:?}",
            shard.outcome
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // WP-148 fix round 1 (M11): a `Command.directory` that names an existing
    // regular file (not a directory) is refused with `Err("cannot enter ...")`
    // even when the source path is absolute.
    #[test]
    fn s8_4_directory_that_is_a_regular_file_is_refused() {
        let dir = test_dir("dir-is-file");
        let not_a_dir = dir.join("not_a_dir");
        let main = dir.join("main.cpp");
        std::fs::write(&not_a_dir, "plain file\n").unwrap();
        std::fs::write(&main, "int x = 1;\n").unwrap();
        let obs = test_observer(&not_a_dir, &["c++", "-std=c++20", main.to_str().unwrap()]);
        let err = super::parse_observer(&obs, Phase::Map, 0)
            .expect_err("regular file as Command.directory must be rejected");
        assert!(err.contains("not a directory"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
