//! The parse task: one observer through Kiln's front-end library entry
//! (`kiln_driver::front_end::run_front_end`) to its shard
//! (`spec/model.md` §8, §8.4, `spec/exec.md` §5.1). Work packages WP-010,
//! WP-025; the preprocessing record on request, WP-101.

use std::path::{Path, PathBuf};

use kiln_driver::args::Args;
use kiln_driver::front_end::{FrontEnd, FrontEndError, FrontEndOptions, run_front_end};
use slip_model::formats::clip_message;
use slip_model::hash::content_digest;
use slip_model::{Counter, Digest, FileRef, Observer, Outcome, Phase, Shard, Status};

use crate::check::{front_end_line, program, utf8_command};
use crate::kiln::ast::Ast;
use crate::kiln::lex::{Interner, LexOptions};
use crate::kiln::oven::Oven;
use crate::kiln::pp::PpRecord;
use crate::kiln::source::{FileId, FileOrigin, Loc, SourceManager};
use crate::lookups::Lookups;

/// What the front end of [`parse_observer_with`] keeps beyond what the
/// parse task needs. Every option is off by default, and the shard does not
/// depend on any of them: they only change what the closure can read
/// (`docs/wp/WP-101.md`, "Nothing is turned on").
///
/// `#[non_exhaustive]`: start from `ParseOptions::default()` and set the
/// fields wanted.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ParseOptions {
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
}

/// The words `options` adds to the front end's command line, right after
/// `-fsyntax-only` (so before any `--` of the command's own). They are
/// words of the line the front end is given, never of the plan: the
/// observer and its id do not change, and neither does the shard.
///
/// At Kiln `913a490bb` [`ParseOptions::written`] is two driver flags:
///
/// - `-fkiln-decl-extents`, which records the declaration extents and
///   prints nothing (Kiln `kiln_driver/src/args.rs:2057`);
/// - `-fkiln-dump-written-refs`, the only switch of the written-reference
///   table (`args.rs:2056`; Kiln's `FrontEndOptions` has none until
///   Kiln WP-2320, `docs/requests/KR1-amendment-1.md` §1). Through the
///   front-end library entry it records the table and formats no dump:
///   the entry returns before any dump is made
///   (`kiln_driver/src/syntax_cmd.rs:692-696`; Kiln `spec/driver.md`
///   "Front-end library entry": "no dumps"). So at this pin it is in
///   effect a recording-only switch, spelled as the dump flag.
///
/// When Kiln gives the entry a recording switch, this function is the one
/// place that changes.
fn written_words(options: ParseOptions) -> &'static [&'static str] {
    if options.written { &["-fkiln-dump-written-refs", "-fkiln-decl-extents"] } else { &[] }
}

/// The token starts of the buffers a unit lexed ([`Unit::is_token_start`]).
#[derive(Default)]
struct TokenStarts(std::sync::Mutex<std::collections::HashMap<FileId, std::sync::Arc<[u32]>>>);

impl TokenStarts {
    /// The starts of `file`, made by `lex` the first time.
    fn of(&self, file: FileId, lex: impl FnOnce() -> Vec<u32>) -> std::sync::Arc<[u32]> {
        let mut map = self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        std::sync::Arc::clone(map.entry(file).or_insert_with(|| lex().into()))
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

    /// [`Unit::token_end`].
    pub fn token_end(&self, at: Loc) -> Option<Loc> {
        crate::kiln::lex::token_end(self.sm, self.interner, at, self.lex)
    }

    /// [`Unit::is_token_start`].
    pub fn is_token_start(&self, at: Loc) -> bool {
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
        Tokens { sm: self.sm, interner: self.interner, lex: self.lex, starts: &self.token_starts }
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

/// The shard of `observer`'s parse: `parsed` is its inputs and its count
/// of preprocessed tokens, when it parsed.
fn new_shard(
    observer: &Observer,
    phase: Phase,
    round: u64,
    outcome: Outcome,
    parsed: Option<(Vec<FileRef>, u64)>,
) -> Shard {
    let (inputs, counters) = match parsed {
        Some((files, tokens)) => {
            let counters = vec![
                Counter { name: "inputs.bytes".into(), value: files.iter().map(|f| f.size).sum() },
                Counter { name: "inputs.files".into(), value: files.len() as u64 },
                Counter { name: "pp.tokens".into(), value: tokens },
            ];
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
/// **This changes the process's current directory** to the command's
/// `directory` and does not restore it: Kiln has no working-directory
/// parameter. That is correct in a worker, which runs one job at a time;
/// anything else in the process that uses relative paths, on any thread,
/// is affected. Callers that run several in one process (tests) must
/// serialise them.
///
/// # Errors
///
/// Only a failure of the worker itself: the directory cannot be entered,
/// an input cannot be resolved to its real path, a consulted file cannot
/// be read, a file changed during the parse, the front end or `f`
/// panicked, or the front end's thread could not be started. Diagnostics
/// about the subject are a shard with status `diagnosed` or `rejected`.
pub fn parse_observer_with<R: Send>(
    observer: &Observer,
    phase: Phase,
    round: u64,
    options: ParseOptions,
    f: impl FnOnce(&Unit<'_>) -> R + Send,
) -> Result<(Shard, Option<R>), String> {
    let shard = |outcome, parsed| new_shard(observer, phase, round, outcome, parsed);

    let (argv0, rest) = match utf8_command(&observer.command) {
        Ok(words) => words,
        Err(arg) => {
            return Ok((shard(rejected(&format!("argument `{arg}` is not UTF-8")), None), None));
        }
    };

    let directory = bytes_path(&observer.command.directory)?;
    std::env::set_current_dir(&directory)
        .map_err(|e| format!("cannot enter {}: {e}", directory.display()))?;

    let mut line = front_end_line(&rest);
    let words = written_words(options);
    line.splice(1..1, words.iter().map(|&w| w.to_owned()));
    let program = program(&argv0);
    let dir: &Path = &directory;
    let (program_ref, line_ref) = (&program, &line);
    let see = move |fe: &FrontEnd<'_, '_>| -> Result<(Vec<FileRef>, u64, R), String> {
        let (inputs, buffers) = collect_inputs(fe, dir)?;
        let lookups = Lookups::of(fe, || embed_dirs(program_ref, line_ref));
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
        Ok((inputs, fe.tokens, value))
    };
    let report = match run_front_end(&program, &line, fe_options(options), see) {
        Ok(report) => report,
        Err(e @ (FrontEndError::NotSyntaxOnly | FrontEndError::NotOneInput)) => {
            return Ok((shard(rejected(&e.to_string()), None), None));
        }
        // `Panicked`, `Thread`, and any reason Kiln adds (`FrontEndError`
        // is `#[non_exhaustive]` at Kiln `83566cc47`): a failure of the
        // worker until it is classified here.
        Err(e) => return Err(e.to_string()),
    };
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
        Some((inputs, tokens, value)) => Ok((shard(outcome, Some((inputs, tokens))), Some(value))),
        None => Ok((shard(outcome, None), None)),
    }
}

/// Kiln's options for `options`.
fn fe_options(options: ParseOptions) -> FrontEndOptions {
    let mut fe = FrontEndOptions::default();
    fe.pp_record = options.pp_record;
    fe.record_lookups = options.lookups;
    fe
}

/// The `--embed-dir` directories of the front end's line as Kiln's driver
/// parses them (the lookup log does not name them), from the line as
/// `run_front_end` reads it.
fn embed_dirs(program: &str, line: &[String]) -> Vec<String> {
    Args::parse_as(program, &kiln_driver::rsp::expand(line)).embed_dirs
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
/// by buffer.
fn collect_inputs(
    fe: &FrontEnd<'_, '_>,
    directory: &Path,
) -> Result<(Vec<FileRef>, Buffers), String> {
    let sm = fe.sm;
    let mut found: Vec<Candidate> = Vec::new();
    for file in sm.file_ids() {
        let bytes = match sm.file_origin(file) {
            FileOrigin::Virtual => continue,
            FileOrigin::Disk => sm.original_bytes(file).ok_or_else(|| {
                format!("buffer {} has no bytes although read from disk", sm.file_name(file))
            })?,
            // Only from a module file or a precompiled header: listed when
            // its file was read, else nothing read it.
            FileOrigin::Lazy => match sm.original_bytes(file) {
                Some(bytes) => bytes,
                None => continue,
            },
        };
        let real = real_path(directory, sm.file_name(file))?;
        found.push((real, bytes.len() as u64, content_digest(&bytes), Some(file)));
    }
    let buffers_end = found.len();
    for c in fe.consulted {
        let real = real_path(directory, &c.path)?;
        // A `__has_include` hit is a buffer; an `#embed` resource is not.
        if found[..buffers_end].iter().any(|(p, ..)| *p == real) {
            continue;
        }
        let (size, content) = read_consulted(&real)?;
        found.push((real, size, content, None));
    }
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
/// here, once: Kiln keeps no bytes of an `#embed` resource or a
/// `__has_embed` hit. Failing to read it is `Err`.
fn read_consulted(real: &[u8]) -> Result<(u64, Digest), String> {
    let bytes = std::fs::read(bytes_path(real)?)
        .map_err(|e| format!("cannot read {}: {e}", String::from_utf8_lossy(real)))?;
    Ok((bytes.len() as u64, content_digest(&bytes)))
}

/// `name` resolved against `directory`, then to its real path.
fn real_path(directory: &Path, name: &str) -> Result<Vec<u8>, String> {
    let spelled = directory.join(name);
    let real = std::fs::canonicalize(&spelled)
        .map_err(|e| format!("cannot resolve {}: {e}", spelled.display()))?;
    Ok(path_bytes(&real))
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
    use super::{path_bytes, read_consulted};

    // spec/model.md §8.4 "Inputs": a consulted path that cannot be read
    // here (a directory, a file gone since Kiln read it) is `Err`.
    #[test]
    fn s8_4_inputs_unreadable_consulted_path_is_err() {
        let dir = std::env::temp_dir();
        let e = read_consulted(&path_bytes(&dir)).unwrap_err();
        assert!(e.starts_with("cannot read "), "{e}");
        let gone = dir.join(format!("slip-kiln-gone-{}", std::process::id()));
        let e = read_consulted(&path_bytes(&gone)).unwrap_err();
        assert!(e.starts_with("cannot read "), "{e}");
    }
}
