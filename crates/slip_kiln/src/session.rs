//! The transform session: the transform unit compiled once, its entry run
//! many times (`spec/api.md` §2.1, §2.2, §2.4 rule 5, §2.5; work package
//! WP-080).
//!
//! A facade over Kiln's evaluation sessions (`with_eval_session`,
//! `EvalSession`, `RunBudget`, `CancelToken`, `HostCalls`;
//! `kiln/spec/eval-session.md`, `kiln/spec/host-calls.md`) that names no
//! Kiln type, so that `slip_script` can drive a transform without naming a
//! Kiln crate (`spec/harness.md` §3 `kiln-boundary`). It knows nothing of
//! the API's operations: a [`Handler`] is any function of an operation
//! number, integers and bytes.
//!
//! The decisions the specification leaves to this module are in
//! `spec/landed/api/WP-080.md`.

use std::io;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use kiln_ast::decl::{ConstexprKind, DeclKind};
use kiln_ast::types::TypeView;
use kiln_ast::{Ast, DeclId};
use kiln_diag::{DiagOptions, DiagSink, SharedBuffer, TextSink};
use kiln_driver::session::{SessionFrontEnd, SessionRefused, with_eval_session};
use kiln_driver::{Io, SharedWriter};
use kiln_oven::{HostCalls, HostError, HostOut};
use kiln_sema::session::{
    CancelToken, Entry as KilnEntry, EntryError as KilnEntryError, FieldKind, IntValue, RunBudget,
    RunError, RunValue,
};
use kiln_sema::stack_guard::StackGuard;
use kiln_source::{FileOrigin, SourceManager};
use slip_model::Digest;
use slip_model::hash::content_digest;

/// The options of the command line before the author's (`spec/api.md`
/// §2.1 rule 1), after the program name.
const FIXED: [&str; 4] =
    ["-std=c++26", "-fsyntax-only", "-fkiln-reproducible-time", "-ftemplate-depth=512"];

/// The template-instantiation depth of a transform unit (the last word of
/// [`FIXED`]), half of Kiln's default: with it, every recursion a run can
/// drive at run time reaches a limit of Kiln's before Kiln's stack guard
/// on a thread of `slip_script::driver::STACK` bytes, in a debug build
/// (`spec/landed/api/WP-089.md`, the stack).
pub const TEMPLATE_DEPTH: u32 = 512;

/// The author's options (§2.1 rule 2). Each takes one value, joined
/// (`-DX=1`) or as the next word (`-D X=1`).
const AUTHOR_OPTIONS: [&str; 5] = ["-isystem", "-include", "-I", "-D", "-U"];

/// The author's options whose value is never joined: clang and Kiln have
/// longer options that begin with them (`-include-pch`, `-isystem-after`).
const SEPARATE_ONLY: [&str; 2] = ["-isystem", "-include"];

/// The author's options whose value is a path, which must be absolute.
const PATH_OPTIONS: [&str; 3] = ["-isystem", "-include", "-I"];

/// Why a value or path given to Kiln as a word of its own is refused:
/// Kiln's driver would read it as something other than a value.
fn refused_word(w: &str) -> Option<&'static str> {
    if w.is_empty() {
        Some("the value is empty")
    } else if w.starts_with('-') {
        Some("the value begins with `-`, which Kiln would read as an option")
    } else if w.starts_with('@') {
        // `kiln/crates/kiln_driver/src/session.rs:81`, `rsp.rs:15-41`.
        Some("the value begins with `@`, which Kiln would read as a response file")
    } else if w.contains('\0') {
        Some("the value holds a NUL byte")
    } else {
        None
    }
}

/// The command line of `spec/api.md` §2.1, already checked (rule 2): the
/// only way to build one is [`TransformCommand::new`], so an option outside
/// rule 2 never reaches Kiln.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransformCommand {
    resource_include: PathBuf,
    /// The author's options, each as `(option, value)`, in order.
    author: Vec<(&'static str, String)>,
    file: PathBuf,
}

/// An author's option that §2.1 rule 2 does not allow (`transform-option`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OptionRefused {
    /// The word refused, as given.
    pub word: String,
    /// Why, in a sentence.
    pub reason: &'static str,
}

impl std::fmt::Display for OptionRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "transform option `{}`: {}", self.word, self.reason)
    }
}

impl std::error::Error for OptionRefused {}

impl TransformCommand {
    /// The command of `<resource>/include` (`resource_include`), the
    /// author's options `author` (words, as on a command line) and the
    /// transform `file`.
    ///
    /// # Errors
    ///
    /// [`OptionRefused`], naming the word, for the first of:
    ///
    /// - a word of `author` that is not one of `-I`, `-isystem`, `-D`,
    ///   `-U`, `-include` with its value (joined only to `-I`, `-D` and
    ///   `-U`);
    /// - a value that is missing, empty, begins with `-` (Kiln would read
    ///   an option) or `@` (Kiln would read a response file), or holds a
    ///   NUL byte;
    /// - a value of `-I`, `-isystem` or `-include` that is not an absolute
    ///   path;
    /// - a `-D` whose replacement (what follows its `=`) holds the letters
    ///   `embed`: the scan for `#embed` and `__has_embed`
    ///   ([`CompileError::Embed`]) reads files, and this is in none;
    /// - a `file` or `resource_include` that is not UTF-8, is empty, holds a
    ///   NUL byte, begins with `-` or `@`, or is not absolute.
    ///
    /// Paths must be absolute because the process's current directory is
    /// not stable in a worker ([`crate::parse_observer_with`] changes it),
    /// so a relative path would name whatever the last subject unit's
    /// directory holds.
    pub fn new(
        resource_include: PathBuf,
        author: &[String],
        file: PathBuf,
    ) -> Result<TransformCommand, OptionRefused> {
        let mut checked = Vec::new();
        let mut words = author.iter();
        while let Some(word) = words.next() {
            let refused = |reason| OptionRefused { word: word.clone(), reason };
            // Longest first: `-isystem` and `-include` before `-I`.
            let Some(opt) = AUTHOR_OPTIONS.into_iter().find(|o| word.starts_with(o)) else {
                return Err(refused(
                    "only -I, -isystem, -D, -U and -include may be given (spec/api.md §2.1)",
                ));
            };
            let (value, word) = if word.len() == opt.len() {
                match words.next() {
                    Some(v) => (v.clone(), v),
                    None => return Err(refused("the option's value is missing")),
                }
            } else if SEPARATE_ONLY.contains(&opt) {
                // `-include-pch`, `-isystem-after`: other options.
                return Err(refused("-isystem and -include take their value as the next word"));
            } else {
                (word[opt.len()..].to_owned(), word)
            };
            let refused = |reason| OptionRefused { word: word.clone(), reason };
            if let Some(reason) = refused_word(&value) {
                return Err(refused(reason));
            }
            if PATH_OPTIONS.contains(&opt) && !Path::new(&value).is_absolute() {
                return Err(refused("the path is not absolute"));
            }
            // A macro of the command line is in no file, so the scan of
            // the files read (`embed_line`) does not see its replacement.
            if opt == "-D" && value.split_once('=').is_some_and(|(_, r)| r.contains("embed")) {
                return Err(refused(
                    "the macro's replacement holds `embed`: `#embed` and `__has_embed` are not supported in a transform (spec/api.md §2.1)",
                ));
            }
            checked.push((opt, value));
        }
        for path in [&file, &resource_include] {
            let word = path.to_string_lossy().into_owned();
            let refused = |reason| OptionRefused { word: word.clone(), reason };
            let Some(text) = path.to_str() else { return Err(refused("the path is not UTF-8")) };
            if let Some(reason) = refused_word(text) {
                return Err(refused(reason));
            }
            if !path.is_absolute() {
                return Err(refused("the path is not absolute"));
            }
        }
        Ok(TransformCommand { resource_include, author: checked, file })
    }

    /// `<resource>/include`.
    pub fn resource_include(&self) -> &Path {
        &self.resource_include
    }

    /// The transform file.
    pub fn file(&self) -> &Path {
        &self.file
    }

    /// The words given to Kiln after the program name: §2.1 rule 1's line,
    /// each author option as two words (option, value), paths as given.
    pub fn command_line(&self) -> Vec<String> {
        let mut line: Vec<String> = FIXED.iter().map(|&w| w.to_owned()).collect();
        line.push("-I".to_owned());
        line.push(utf8(&self.resource_include));
        for (opt, value) in &self.author {
            line.push((*opt).to_owned());
            line.push(value.clone());
        }
        line.push(utf8(&self.file));
        line
    }
}

/// A path checked UTF-8 by [`TransformCommand::new`].
fn utf8(p: &Path) -> String {
    p.to_str().map_or_else(|| unreachable!("a checked path is UTF-8"), str::to_owned)
}

/// Why the transform unit gave no session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompileError {
    /// spec/api.md `transform-compile`; the diagnostics as Kiln renders them.
    Diagnosed { rendered: String, errors: u32 },
    /// Kiln refused the command line Slip built: a Slip bug, or the front
    /// end stopped for another reason (`docs/requests/KS2-amendment-1.md`
    /// §1). What Kiln wrote to standard error.
    CommandLine(String),
    /// Kiln's stack guard tripped while compiling the unit (its fatal
    /// diagnostic `sema_stack_exhausted` is among Kiln's): the stack of the
    /// thread was too small for the unit, which is the host's limit and
    /// not the author's error. The diagnostics as Kiln renders them.
    ///
    /// Recognised in the rendering, by a line whose message is that
    /// diagnostic's (`<place>: fatal error: kiln: stack exhausted during
    /// <phase>; reduce nesting or raise -fkiln-stack-size`): Kiln gives no
    /// structured diagnostics of a unit that failed to compile.
    StackExhausted { rendered: String },
    /// Kiln panicked while compiling; the panic's message.
    Panicked(String),
    /// A file of the unit may use `#embed` or `__has_embed`, which a
    /// transform may not at this Kiln revision: the resources they read
    /// are not in [`Session::files`], so not in the transform's identity.
    /// The file (its path as in [`ReadFile`]) and the line (from 1).
    Embed { path: Vec<u8>, line: u32 },
}

/// One file the transform unit read: path as given (spelled as its
/// directories list it when it is a real path, `slip_spelling::respelled`),
/// content digest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadFile {
    pub path: Vec<u8>,
    pub digest: Digest,
}

/// The limits of one run (`spec/api.md` §2.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Budget {
    pub steps: u64,
    pub bytes: u64,
    pub depth: u32,
}

/// Stops a run from any thread (§2.5 rule 4). Clones share one flag.
#[derive(Clone, Debug, Default)]
pub struct Cancel(CancelToken);

impl Cancel {
    /// A token not cancelled.
    pub fn new() -> Cancel {
        Cancel::default()
    }

    /// Cancels: a run given this token stops at its next check, and every
    /// later run given it fails at its first, until [`Cancel::reset`].
    pub fn cancel(&self) {
        self.0.cancel();
    }

    /// Whether [`Cancel::cancel`] was called since the last reset.
    pub fn is_cancelled(&self) -> bool {
        self.0.is_cancelled()
    }

    /// Makes the token usable again.
    pub fn reset(&self) {
        self.0.reset();
    }
}

/// What a handler writes into (Kiln's `HostOut`, without its type).
pub trait Out {
    fn int_capacity(&self) -> usize;
    fn byte_capacity(&self) -> usize;
    fn write_ints(&mut self, v: &[u64]);
    fn write_bytes(&mut self, v: &[u8]);
}

/// The handler of one run. `Err(text)` is Kiln's uncatchable handler error
/// (spec/api.md §3.5 rule 4).
///
/// `Send` because Kiln's `HostCalls` is (`kiln/crates/kiln_oven/src/host.rs:13`).
/// A handler that borrows a subject's `Ast` and `SourceManager` is `Send`
/// (`tests/session.rs`); one that holds its `Oven` is not.
pub trait Handler: Send {
    /// # Errors
    ///
    /// Whatever the handler decides: the run then fails with
    /// [`RunFailure::Host`].
    fn call(
        &mut self,
        op: u32,
        ints: &[u64],
        bytes: &[u8],
        out: &mut dyn Out,
    ) -> Result<u64, String>;
}

/// A function of the transform unit that may be run, by the session that
/// gave it: [`Session::run`] refuses it in any other session
/// ([`RunFailure::ForeignEntry`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    entry: kiln_sema::session::Entry,
    session: u64,
}

/// Why a name is not an entry (`spec/api.md` §2.3 rule 2, in its order).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryError {
    Missing,
    Overloaded,
    NotFunction,
    NotConsteval,
    Parameters,
    Type,
    Undefined,
}

/// A class or struct entity of a transform unit, identified by its first
/// declaration in that session (Kiln WP-2480). Names no Kiln type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Class {
    decl: u32,
    session: u64,
}

impl Class {
    /// Constructs a synthetic [`Class`] for unit tests that do not run a
    /// session.
    #[doc(hidden)]
    pub const fn for_test(decl: u32, session: u64) -> Class {
        Class { decl, session }
    }
}

/// A class or struct subobject of an uncaught exception's value (Kiln
/// WP-2480): its entity (when the class existed when the session opened),
/// its direct and virtual base subobjects, and its named integral or
/// enumeration members (unsigned values).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThrownRecord {
    pub class: Option<Class>,
    pub bases: Vec<ThrownRecord>,
    pub members: Vec<(String, u128)>,
}

/// How a run failed (`spec/api.md` §2.8).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunFailure {
    NotConstant,
    StepBudget,
    MemoryBudget,
    DepthLimit,
    Cancelled,
    /// The handler's error, or Kiln's note on a host call whose arguments
    /// failed its checks.
    Host(String),
    Uncaught {
        type_name: String,
        /// The thrown object when it is a class or struct whose value Kiln
        /// froze and decoded (Kiln WP-2480).
        record: Option<ThrownRecord>,
    },
    /// An earlier run's handler panicked: the session is unusable (Kiln
    /// did not put its state back), and this run did not reach Kiln. A
    /// worker ends.
    Poisoned,
    /// The entry was given by another session; this run did not reach
    /// Kiln.
    ForeignEntry,
    /// A kind of failure this crate does not know (Kiln's `RunError` is
    /// `#[non_exhaustive]`), as Kiln debug-prints it: the host cannot say
    /// what it means.
    Unknown(String),
}

/// What one run produced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunReport {
    pub result: Result<(), RunFailure>,
    pub steps: u64,
    pub host_calls: u64,
    pub peak_bytes: u64,
    /// The run's diagnostics, notes and call stack included, as Kiln's text
    /// sink renders them (empty for a run that completed without one).
    pub rendered: String,
    /// The same diagnostics as Kiln gives them, in order.
    pub diagnostics: Vec<RunDiagnostic>,
}

/// One diagnostic of a run with its notes, as Kiln gives it
/// (`kiln_diag::Diagnostic`), its place resolved in the transform unit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunDiagnostic {
    /// Kiln's name of the diagnostic (`DiagId::name`, as `sema_stack_exhausted`);
    /// `None` for a note of free text.
    pub id: Option<&'static str>,
    pub severity: Severity,
    /// The message, without place or option.
    pub message: String,
    /// Where its caret is, if it has a place in a file.
    pub place: Option<SourcePlace>,
    /// Its notes, in order.
    pub notes: Vec<RunDiagnostic>,
}

/// The level of a [`RunDiagnostic`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Note,
    Remark,
    Warning,
    Error,
    Fatal,
}

/// A place in a file of the transform unit: the file's path as Kiln
/// names it (as in [`ReadFile`]: the path as given), the physical line
/// and column (from 1, the column in bytes). `#line` does not move it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourcePlace {
    pub file: Vec<u8>,
    pub line: u32,
    pub column: u32,
}

/// The compiled transform unit (§2.4 rule 5). `Send`, so that a run may
/// be called from another thread while the thread of [`with_transform`]
/// waits; see [`Session::run`] for why `slip_script` does not, and the hazard
/// of doing so at this Kiln revision.
pub struct Session<'a> {
    engine: &'a mut dyn Engine,
    files: Vec<ReadFile>,
    /// The target's triple and the bytes of the predefines buffer.
    triple: String,
    predefines: Vec<u8>,
    /// This session's number, which its entries carry.
    id: u64,
    /// Set while a run is inside Kiln: still set after a run unwound.
    in_run: bool,
    /// Set once a run ended in Kiln's `RunError::StackExhausted`: since
    /// Kiln WP-2410 the guard's flag is put back after each run, and the
    /// host keeps it set (`Session::stack_exhausted`).
    tripped: bool,
}

/// The number of the next session in this process.
static NEXT_SESSION: AtomicU64 = AtomicU64::new(0);

impl std::fmt::Debug for Session<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session").field("files", &self.files.len()).finish_non_exhaustive()
    }
}

impl Session<'_> {
    /// Every file the transform unit read, in the order first read
    /// (§2.2): its path as Kiln was given or found it, the digest of the
    /// bytes Kiln read.
    pub fn files(&self) -> &[ReadFile] {
        &self.files
    }

    /// The target the unit was compiled for (`spec/api.md` §2.1 rule 3):
    /// its normalised triple, as Kiln writes it
    /// (`kiln/crates/kiln_abi/src/target.rs`, `Triple`'s `Display`).
    pub fn triple(&self) -> &str {
        &self.triple
    }

    /// The bytes of the unit's predefines buffer, the virtual buffer
    /// `<built-in>` that Kiln reads before the transform file
    /// (`kiln/crates/kiln_pp/src/preprocessor.rs:661`): the target's
    /// predefined macros, then one `#define` or `#undef` line per `-D` or
    /// `-U`. Not in [`Session::files`], which lists what was read from
    /// disk. Empty if Kiln made no such buffer.
    pub fn predefines(&self) -> &[u8] {
        &self.predefines
    }

    /// Whether Kiln's stack guard tripped in this unit: its flag
    /// `Sema::stack_exhausted` (`kiln/crates/kiln_sema/src/stack_guard.rs:213`),
    /// set by compiling, or a run of this session that ended in Kiln's
    /// `RunError::StackExhausted`. Kiln reports the diagnostic only once
    /// (`stack_guard.rs:220-222`). Since Kiln WP-2410 the flag is put back
    /// after each run (`Sema::guard_leave_thread`), so a run's exhaustion
    /// no longer stays in it; this keeps it set, and the session is not
    /// run again (`slip_script::driver`, `spec/api.md` §2.8).
    pub fn stack_exhausted(&self) -> bool {
        self.tripped || self.engine.stack_exhausted()
    }

    /// Whether a function or function template named `name` is a member
    /// of the global namespace (§2.3 rule 3), members as `spec/api.md`
    /// §6.1 rules 3 and 5 define them: inline and unnamed namespaces (and
    /// linkage specifications) are transparent; using-directives and
    /// using-declarations are not followed. A `name` that is not an
    /// identifier is declared by nothing.
    pub fn declares(&mut self, name: &str) -> bool {
        self.engine.declares(name)
    }

    /// The function `name`, a member of the global namespace as for
    /// [`Session::declares`], as an entry (§2.3 rule 2).
    ///
    /// # Errors
    ///
    /// The first condition of §2.3 rule 2 it fails. A name that is not
    /// exactly an identifier (`ns::f`, ` transform`) is
    /// [`EntryError::Missing`]; a deleted function is
    /// [`EntryError::Undefined`].
    pub fn entry(&mut self, name: &str) -> Result<Entry, EntryError> {
        let entry = self.engine.entry(name)?;
        Ok(Entry { entry, session: self.id })
    }

    /// Finds the unique class or struct entity named `name` inside the
    /// nested namespaces `namespaces` (e.g. `&["slip", "v0"]`), optionally
    /// requiring its first declaration to lie under `in_dir`.
    pub fn class(&self, namespaces: &[&str], name: &str, in_dir: Option<&Path>) -> Option<Class> {
        self.engine.class(namespaces, name, in_dir).map(|decl| Class { decl, session: self.id })
    }

    /// Runs `entry` once: host calls reach `handler`; the run stops at
    /// `budget` or when `cancel` is cancelled. The session is usable after
    /// any outcome of the run itself.
    ///
    /// A panic of `handler` unwinds out of this call, and the session is
    /// then unusable: every later run is [`RunFailure::Poisoned`] (Kiln
    /// has not put its state back). A worker ends.
    ///
    /// **The stack guard.** Before Kiln WP-2410, Kiln checked the stack
    /// against the guard of the thread that called [`with_transform`],
    /// copied when the unit was compiled, so on another thread a run that
    /// reached a stack check (an instantiation made at run time through
    /// reflection) failed with "stack exhausted", sticky for every later
    /// run. Since WP-2410 it checks the running thread's guard and such a
    /// run completes (KN1 of WP-089, `tests/session.rs` `kn1_…`).
    /// `slip_script::driver` still runs a session only on the thread that
    /// compiled it (`spec/api.md` §2.5 rule 5).
    pub fn run(
        &mut self,
        entry: Entry,
        handler: &mut dyn Handler,
        budget: Budget,
        cancel: &Cancel,
    ) -> RunReport {
        let refused = |failure| RunReport {
            result: Err(failure),
            steps: 0,
            host_calls: 0,
            peak_bytes: 0,
            rendered: String::new(),
            diagnostics: Vec::new(),
        };
        if entry.session != self.id {
            return refused(RunFailure::ForeignEntry);
        }
        if self.in_run {
            return refused(RunFailure::Poisoned);
        }
        self.in_run = true;
        let (report, tripped) = self.engine.run(entry.entry, handler, budget, &cancel.0, self.id);
        self.in_run = false;
        self.tripped |= tripped;
        report
    }
}

/// Compiles the transform on the calling thread and calls `f` with the
/// session.
///
/// The caller's thread must have the stack it wants: `stack` is the bytes
/// of stack left below this call, and is trusted. This installs Kiln's
/// stack guard for it on the calling thread
/// (`kiln/crates/kiln_sema/src/stack_guard.rs:92`), overwriting the guard
/// the thread had, and leaves it installed after the call: Kiln has no way
/// to put the old one back. That holds also when this is called from
/// inside a handler or inside `parse_observer_with`'s closure, whose
/// thread's guard it then replaces. The compiled unit keeps a copy of this
/// guard for every run ([`Session::run`]).
///
/// The unit's diagnostics are captured, not written: they are in
/// [`CompileError`].
pub fn with_transform<R>(
    cmd: &TransformCommand,
    stack: usize,
    f: impl FnOnce(Result<Session<'_>, CompileError>) -> R,
) -> R {
    if let Some(var) = crate::check::kiln_switch_in_environment() {
        return f(Err(CompileError::CommandLine(crate::parse::switch_message(var))));
    }
    StackGuard::install(stack);
    let buffer = SharedBuffer::new();
    let io = Io {
        stdout: Box::new(io::sink()),
        stderr: SharedWriter::new(buffer.clone()),
        stdin: Box::new(io::empty()),
    };
    let line = cmd.command_line();
    let mut f = Some(f);
    let done = catch_unwind(AssertUnwindSafe(|| {
        with_eval_session("kiln", &line, io, |opened| {
            let f = f.take().unwrap_or_else(|| unreachable!("the session's closure runs once"));
            match opened {
                Ok(mut fe) => {
                    let (files, embed) = read_files(fe.sources);
                    if let Some((path, line)) = embed {
                        return f(Err(CompileError::Embed { path, line }));
                    }
                    let id = NEXT_SESSION.fetch_add(1, Ordering::Relaxed);
                    let triple = fe.session.sema().abi.triple.to_string();
                    let predefines = predefines(fe.sources);
                    f(Ok(Session {
                        engine: &mut fe,
                        files,
                        triple,
                        predefines,
                        id,
                        in_run: false,
                        tripped: false,
                    }))
                }
                Err(SessionRefused::TuHasErrors { errors, .. }) => {
                    f(Err(diagnosed(buffer.contents(), errors)))
                }
                // A fatal error other than the error limit's, or no tree
                // (Kiln WP-2322). Before it Kiln reported a unit that
                // stopped with errors as `TuHasErrors` and one with no
                // tree as `CommandLine`: the errors among the unit's
                // diagnostics tell the two apart, as they did
                // (`spec/landed/digest-kinds/WP-066.md`).
                Err(SessionRefused::Stopped { diagnostics, .. }) => {
                    let errors = diagnostics
                        .iter()
                        .filter(|d| {
                            matches!(d.level, kiln_diag::Level::Error | kiln_diag::Level::Fatal)
                        })
                        .count();
                    match u32::try_from(errors).unwrap_or(u32::MAX) {
                        0 => f(Err(CompileError::CommandLine(buffer.contents()))),
                        errors => f(Err(diagnosed(buffer.contents(), errors))),
                    }
                }
                // Kiln catches a panic of the front end since its WP-2322;
                // before, `catch_unwind` below caught it.
                Err(SessionRefused::Panicked(message)) => f(Err(CompileError::Panicked(message))),
                // `CommandLine`, and any reason Kiln adds (`#[non_exhaustive]`).
                Err(_) => f(Err(CompileError::CommandLine(buffer.contents()))),
            }
        })
    }));
    match done {
        Ok(r) => r,
        // Kiln panicked before `f` was called; a panic of `f` (or after
        // it) is the caller's and goes on.
        Err(panic) => match f.take() {
            Some(f) => f(Err(CompileError::Panicked(panic_message(panic.as_ref())))),
            None => resume_unwind(panic),
        },
    }
}

/// The refusal of a unit that reported `errors` errors, `rendered` as Kiln
/// wrote them: [`CompileError::StackExhausted`] when Kiln's stack guard
/// tripped, else [`CompileError::Diagnosed`].
fn diagnosed(rendered: String, errors: u32) -> CompileError {
    if stack_exhausted_in(&rendered) {
        CompileError::StackExhausted { rendered }
    } else {
        CompileError::Diagnosed { rendered, errors }
    }
}

/// Whether `rendered` holds Kiln's fatal "stack exhausted" diagnostic: a
/// line `<file>:<line>:<column>: fatal error: kiln: stack exhausted during
/// <phase>; reduce nesting or raise -fkiln-stack-size`, with one of
/// Kiln's three phases (`kiln/crates/kiln_diag/src/ids/sema_s_z.rs:97`).
/// No other diagnostic Kiln writes is a fatal error with this message: a
/// transform's own text reaches a fatal error's message only quoted (a
/// file not found).
fn stack_exhausted_in(rendered: &str) -> bool {
    const SUFFIX: &str = "; reduce nesting or raise -fkiln-stack-size";
    rendered.lines().any(|l| {
        ["parsing", "template instantiation", "constant evaluation"].iter().any(|phase| {
            l.strip_suffix(SUFFIX).is_some_and(|l| {
                l.strip_suffix(phase).is_some_and(|l| {
                    l.strip_suffix(": fatal error: kiln: stack exhausted during ").is_some_and(
                        |place| {
                            let mut parts = place.rsplitn(3, ':');
                            let col = parts.next().unwrap_or("");
                            let line = parts.next().unwrap_or("");
                            !col.is_empty()
                                && !line.is_empty()
                                && col.bytes().all(|b| b.is_ascii_digit())
                                && line.bytes().all(|b| b.is_ascii_digit())
                                && parts.next().is_some()
                        },
                    )
                })
            })
        })
    })
}

/// The message of a panic's payload.
pub fn panic_message(p: &(dyn std::any::Any + Send)) -> String {
    p.downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| p.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "a panic without a message".to_owned())
}

/// The bytes of the first virtual buffer named `<built-in>` (the
/// predefines, `kiln/crates/kiln_pp/src/preprocessor.rs:661`); empty if
/// there is none.
fn predefines(sm: &SourceManager) -> Vec<u8> {
    sm.file_ids()
        .find(|&id| sm.file_origin(id) == FileOrigin::Virtual && sm.file_name(id) == "<built-in>")
        .map(|id| sm.text(id).as_bytes().to_vec())
        .unwrap_or_default()
}

/// The buffers read from disk, in the order Kiln created them, and the
/// first place (path, line) that may use `#embed` or `__has_embed`.
///
/// Kiln has one buffer per file read (identified by device and inode,
/// `kiln/crates/kiln_source/src/manager.rs:780-787`, `:864-868`), so no
/// file is listed twice; a lazy buffer whose file changed gets a second
/// buffer, with other bytes.
fn read_files(sm: &SourceManager) -> (Vec<ReadFile>, Option<(Vec<u8>, u32)>) {
    let mut files: Vec<ReadFile> = Vec::new();
    let mut embed = None;
    for id in sm.file_ids() {
        let bytes = match sm.file_origin(id) {
            FileOrigin::Virtual => continue,
            // A transform unit reads no module file or snapshot (the
            // options are refused); listed when read, as for a subject.
            FileOrigin::Disk | FileOrigin::Lazy | FileOrigin::Overlay => {
                match sm.original_bytes(id) {
                    Some(b) => b,
                    None => continue,
                }
            }
        };
        // The path as given, spelled as its directories list it when it
        // is a real path (`spec/edits.md` §2.1, "One spelling"): one name
        // for a file whatever case an `#include` wrote it in.
        let path = slip_spelling::respelled(sm.file_name(id).as_bytes());
        if embed.is_none()
            && let Some(line) = embed_line(&bytes)
        {
            embed = Some((path.clone(), line));
        }
        files.push(ReadFile { path, digest: content_digest(&bytes) });
    }
    (files, embed)
}

/// The physical line (from 1) of the first place in `bytes` that may be a
/// `#embed` directive or a use of `__has_embed`, scanning conservatively:
/// after line splices (a backslash, blanks, a new-line), the identifier
/// `__has_embed` anywhere, comments and literals included, and the
/// identifier `embed` on a logical line where `#`, `%:` or `*/` comes
/// before it (a directive's `#` is then before it on the line, or before a
/// comment that ends on the line). It may refuse what is no directive, and
/// never misses a directive spelled in the file. It cannot see an
/// identifier made by `##` or spelled with universal character names.
fn embed_line(bytes: &[u8]) -> Option<u32> {
    let ident = |b: u8| b.is_ascii_alphanumeric() || b == b'_' || b == b'$' || b >= 0x80;
    // Fast path: `embed` is not spelled, and no splice joins two
    // identifier characters (a chain of splices that does has one after
    // an identifier character at its start).
    let slow = bytes.iter().enumerate().any(|(i, &b)| match b {
        b'e' => bytes[i..].starts_with(b"embed"),
        b'\\' => i > 0 && ident(bytes[i - 1]),
        _ => false,
    });
    if !slow {
        return None;
    }
    // The bytes with splices removed, and where each physical line starts
    // in them.
    let mut text = Vec::with_capacity(bytes.len());
    let mut starts: Vec<usize> = vec![0];
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'\\' {
            let mut j = i + 1;
            while j < bytes.len() && matches!(bytes[j], b' ' | b'\t' | b'\x0b' | b'\x0c') {
                j += 1;
            }
            let end = match (bytes.get(j), bytes.get(j + 1)) {
                (Some(b'\r'), Some(b'\n')) => Some(j + 2),
                (Some(b'\n' | b'\r'), _) => Some(j + 1),
                _ => None,
            };
            if let Some(end) = end {
                i = end;
                starts.push(text.len());
                continue;
            }
        }
        text.push(b);
        i += 1;
        if b == b'\n' {
            starts.push(text.len());
        }
    }
    if !text.windows(5).any(|w| w == b"embed") {
        return None;
    }
    let mut marked = false;
    let mut i = 0;
    while i < text.len() {
        let b = text[i];
        if ident(b) {
            let from = i;
            while i < text.len() && ident(text[i]) {
                i += 1;
            }
            let word = &text[from..i];
            if word == b"__has_embed" || (marked && word == b"embed") {
                let line = starts.partition_point(|&s| s <= from);
                return Some(u32::try_from(line).unwrap_or(u32::MAX));
            }
            continue;
        }
        match (b, text.get(i + 1)) {
            (b'\n', _) => marked = false,
            (b'#', _) | (b'%', Some(b':')) | (b'*', Some(b'/')) => marked = true,
            _ => {}
        }
        i += 1;
    }
    None
}

/// The session with its lifetimes erased, so that [`Session`] has one.
trait Engine: Send {
    fn declares(&self, name: &str) -> bool;
    fn stack_exhausted(&self) -> bool;
    fn entry(&self, name: &str) -> Result<KilnEntry, EntryError>;
    fn class(&self, namespaces: &[&str], name: &str, in_dir: Option<&Path>) -> Option<u32>;
    fn run(
        &mut self,
        entry: KilnEntry,
        handler: &mut dyn Handler,
        budget: Budget,
        cancel: &CancelToken,
        session_id: u64,
    ) -> (RunReport, bool);
}

/// Whether `name` is exactly an identifier of the basic character set
/// (`spec/api.md` §6.1 rule 1, a plain component; keywords are not
/// excluded: no function has one as its name).
fn identifier(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes.next().is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// The members of the global namespace named `name` (`spec/api.md` §6.1
/// rules 3 and 5), each entity once by its first declaration, in unit
/// order: the declarations whose semantic context is the translation unit
/// or a namespace it contains transparently (inline, unnamed, through
/// linkage specifications and `export`). Using-declarations and
/// using-directives are not followed, so neither they nor what they name
/// are members.
fn global_members(ast: &Ast, name: &str) -> Vec<DeclId> {
    fn walk(ast: &Ast, ctx: DeclId, name: &str, scopes: &mut Vec<DeclId>, out: &mut Vec<DeclId>) {
        for d in ast.members(ctx) {
            let node = ast.decls.get(d);
            match node.kind() {
                DeclKind::LinkageSpec | DeclKind::Export => walk(ast, d, name, scopes, out),
                DeclKind::Namespace
                    if ast.is_anonymous_namespace(d)
                        || ast
                            .decls
                            .namespace(d)
                            .is_some_and(kiln_ast::decl::NamespaceData::is_inline) =>
                {
                    let primary = ast.primary_context(d);
                    if !scopes.contains(&primary) {
                        scopes.push(primary);
                    }
                    walk(ast, d, name, scopes, out);
                }
                DeclKind::UsingDirective
                | DeclKind::Using
                | DeclKind::UsingEnum
                | DeclKind::UsingShadow
                | DeclKind::ConstructorUsingShadow
                | DeclKind::UsingPack
                | DeclKind::UnresolvedUsingValue
                | DeclKind::UnresolvedUsingTypename
                | DeclKind::UnresolvedUsingIfExists => {}
                _ => {
                    let named =
                        node.name().as_ident().is_some_and(|n| n.as_str(ast.symbols()) == name);
                    let semantic = node.ctx().map(|c| ast.primary_context(ast.redecl_context(c)));
                    let first = ast.first(d);
                    if named
                        && semantic.is_some_and(|c| scopes.contains(&c))
                        && !out.contains(&first)
                    {
                        out.push(first);
                    }
                }
            }
        }
    }
    let mut scopes = vec![ast.tu];
    let mut out = Vec::new();
    if identifier(name) {
        walk(ast, ast.tu, name, &mut scopes, &mut out);
    }
    out
}

fn is_function(ast: &Ast, d: DeclId) -> bool {
    matches!(ast.decls.get(d).kind(), DeclKind::Function | DeclKind::FunctionTemplate)
}

impl Engine for SessionFrontEnd<'_, '_> {
    fn stack_exhausted(&self) -> bool {
        self.session.sema().stack_exhausted()
    }

    fn declares(&self, name: &str) -> bool {
        let ast = &self.session.sema().ast;
        global_members(ast, name).into_iter().any(|d| is_function(ast, d))
    }

    fn entry(&self, name: &str) -> Result<KilnEntry, EntryError> {
        let ast = &self.session.sema().ast;
        let found = global_members(ast, name);
        let functions = found.iter().filter(|&&d| is_function(ast, d)).count();
        let d = match found.as_slice() {
            [] => return Err(EntryError::Missing),
            _ if functions > 1 => return Err(EntryError::Overloaded),
            [d] if ast.decls.get(*d).kind() == DeclKind::Function => ast.latest(*d),
            _ => return Err(EntryError::NotFunction),
        };
        let void = returns_void(ast, d);
        // `entry-not-consteval` (`spec/api.md` §2.3): a function declared
        // `consteval`, as Kiln's own check was until its WP-2072, which
        // accepts a `constexpr` entry too and checks it after the kind.
        if ast
            .decls
            .function(ast.latest(d))
            .is_some_and(|f| f.spec.constexpr_kind() != ConstexprKind::Consteval)
        {
            return Err(EntryError::NotConsteval);
        }
        match self.session.entry(d) {
            Ok(_) if void == Some(false) => Err(EntryError::Type),
            Ok(e) => Ok(e),
            Err(e) => Err(match e {
                KilnEntryError::NotFound { .. } => EntryError::Missing,
                KilnEntryError::Overloaded { .. } => EntryError::Overloaded,
                KilnEntryError::NotConstexpr => EntryError::NotConsteval,
                KilnEntryError::HasParameters => EntryError::Parameters,
                KilnEntryError::WrongReturnType => EntryError::Type,
                // §2.3 puts `entry-type` before `entry-undefined`; Kiln
                // checks the return type for an integral one only.
                KilnEntryError::Undefined if void == Some(false) => EntryError::Type,
                KilnEntryError::Undefined => EntryError::Undefined,
                // `NotAFunction`, and any reason Kiln adds
                // (`#[non_exhaustive]`): not an entry.
                _ => EntryError::NotFunction,
            }),
        }
    }

    fn class(&self, namespaces: &[&str], name: &str, in_dir: Option<&Path>) -> Option<u32> {
        let ast = &self.session.sema().ast;
        find_class(ast, self.sources, namespaces, name, in_dir).map(DeclId::index)
    }

    fn run(
        &mut self,
        entry: KilnEntry,
        handler: &mut dyn Handler,
        budget: Budget,
        cancel: &CancelToken,
        session_id: u64,
    ) -> (RunReport, bool) {
        // `result_bytes` (Kiln WP-2502) bounds a result's tree and an
        // uncaught exception's value: Kiln's program profile.
        let budget = RunBudget {
            steps: budget.steps,
            bytes: budget.bytes,
            depth: budget.depth,
            ..RunBudget::PROGRAM
        };
        let mut calls = Calls { handler };
        let report = self.session.run(entry, &mut calls, budget, cancel);
        let tripped = matches!(report.result, Err(RunError::StackExhausted));
        let result = match report.result {
            Ok(RunValue::Void) => Ok(()),
            Ok(v) => unreachable!("an entry returns void (Session::entry), and returned {v:?}"),
            Err(e) => Err(match e {
                // Kiln WP-2500 says a stack exhausted apart; before, a run
                // that exhausted the stack failed as not constant with the
                // fatal `sema_stack_exhausted` among its diagnostics. The
                // host keeps the session's flag set (`tripped`), which
                // `slip_script::driver` reads before the result.
                RunError::Failed | RunError::StackExhausted => RunFailure::NotConstant,
                RunError::StepBudget => RunFailure::StepBudget,
                RunError::MemoryBudget => RunFailure::MemoryBudget,
                RunError::DepthLimit => RunFailure::DepthLimit,
                RunError::Cancelled => RunFailure::Cancelled,
                // Kiln's note on a call whose arguments failed its
                // checks, apart from the handler's error since Kiln WP-2480.
                RunError::Host { message } | RunError::HostArgument { message } => {
                    RunFailure::Host(message)
                }
                // The thrown object's type and frozen value (Kiln WP-2480).
                RunError::Uncaught { ty, value, .. } => RunFailure::Uncaught {
                    type_name: ty.display.to_string(),
                    record: value.ok().and_then(|tree| thrown_record(&tree.root, session_id)),
                },
                // A kind Kiln adds (`#[non_exhaustive]`): the host cannot
                // say what it means.
                other => RunFailure::Unknown(format!("{other:?}")),
            }),
        };
        let report = RunReport {
            result,
            steps: report.steps,
            host_calls: report.host_calls,
            peak_bytes: report.peak_heap,
            rendered: render(self.sources, &report.diagnostics),
            diagnostics: report.diagnostics.iter().map(|d| diagnostic(self.sources, d)).collect(),
        };
        (report, tripped)
    }
}

/// Extracts a [`ThrownRecord`] from a Kiln [`RunValue`] when it is a
/// class or struct value (Kiln WP-2480). Direct and virtual base
/// subobjects go into `bases`; named non-static data members with an
/// integral or enumeration value go into `members`.
fn thrown_record(v: &RunValue, session: u64) -> Option<ThrownRecord> {
    let RunValue::Record(r) = v else { return None };
    let class = r.ty.decl.map(|d| Class { decl: d.index(), session });
    let mut bases = Vec::new();
    let mut members = Vec::new();
    for f in &r.fields {
        match f.kind {
            FieldKind::Base => {
                if let Some(base) = thrown_record(&f.value, session) {
                    bases.push(base);
                }
            }
            FieldKind::Member => {
                let val = match &f.value {
                    RunValue::Bool(b) => Some(u128::from(*b)),
                    RunValue::Unsigned(u) => Some(*u),
                    RunValue::Signed(s) => u128::try_from(*s).ok(),
                    RunValue::Enum(e) => match e.value {
                        IntValue::Unsigned(u) => Some(u),
                        IntValue::Signed(s) => u128::try_from(s).ok(),
                    },
                    _ => None,
                };
                if let Some(u) = val {
                    members.push((f.name.to_string(), u));
                }
            }
            FieldKind::Anonymous => {}
        }
    }
    for vb in &r.virtual_bases {
        if let Some(base) = thrown_record(&vb.value, session) {
            bases.push(base);
        }
    }
    Some(ThrownRecord { class, bases, members })
}

/// Finds the unique class or struct entity named `name` inside the nested
/// namespaces `namespaces`, optionally requiring its first declaration to
/// be in a file under `in_dir`.
fn find_class(
    ast: &Ast,
    sm: &SourceManager,
    namespaces: &[&str],
    name: &str,
    in_dir: Option<&Path>,
) -> Option<DeclId> {
    fn collect_ns(ast: &Ast, ctx: DeclId, target: &str, out: &mut Vec<DeclId>) {
        for d in ast.members(ctx) {
            let node = ast.decls.get(d);
            match node.kind() {
                DeclKind::LinkageSpec | DeclKind::Export => collect_ns(ast, d, target, out),
                DeclKind::Namespace => {
                    let is_target =
                        node.name().as_ident().is_some_and(|n| n.as_str(ast.symbols()) == target);
                    if is_target {
                        out.push(d);
                    }
                }
                _ => {}
            }
        }
    }
    fn collect_records(ast: &Ast, ctx: DeclId, target: &str, out: &mut Vec<DeclId>) {
        for d in ast.members(ctx) {
            let node = ast.decls.get(d);
            match node.kind() {
                DeclKind::LinkageSpec | DeclKind::Export => collect_records(ast, d, target, out),
                DeclKind::CXXRecord => {
                    if node.name().as_ident().is_some_and(|n| n.as_str(ast.symbols()) == target) {
                        let first = ast.first(d);
                        if !out.contains(&first) {
                            out.push(first);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    if !identifier(name) || namespaces.iter().any(|n| !identifier(n)) {
        return None;
    }
    let mut ctxs = vec![ast.tu];
    for &ns in namespaces {
        let mut next = Vec::new();
        for ctx in ctxs {
            collect_ns(ast, ctx, ns, &mut next);
        }
        if next.is_empty() {
            return None;
        }
        ctxs = next;
    }
    let mut found = Vec::new();
    for ctx in ctxs {
        collect_records(ast, ctx, name, &mut found);
    }
    if let Some(dir) = in_dir {
        found.retain(|&d| {
            let loc = ast.decls.get(d).loc();
            sm.offset_of(sm.spelling_loc(loc))
                .is_some_and(|(f, _)| Path::new(sm.file_name(f)).starts_with(dir))
        });
    }
    match found.as_slice() {
        [only] => Some(*only),
        _ => None,
    }
}

/// `d` with its place resolved in `sm`.
fn diagnostic(sm: &SourceManager, d: &kiln_diag::Diagnostic) -> RunDiagnostic {
    let place = sm.line_col(d.span.lo).map(|lc| SourcePlace {
        file: sm.file_name(lc.file).as_bytes().to_vec(),
        line: lc.line,
        column: lc.col,
    });
    RunDiagnostic {
        id: d.id.map(kiln_diag::DiagId::name),
        severity: match d.level {
            kiln_diag::Level::Note => Severity::Note,
            kiln_diag::Level::Remark => Severity::Remark,
            kiln_diag::Level::Warning => Severity::Warning,
            kiln_diag::Level::Error => Severity::Error,
            kiln_diag::Level::Fatal => Severity::Fatal,
        },
        message: d.message.clone(),
        place,
        notes: d.notes.iter().map(|n| diagnostic(sm, n)).collect(),
    }
}

/// Whether function `d` returns `void` (`None`: it has no function type).
fn returns_void(ast: &Ast, d: DeclId) -> Option<bool> {
    let ty = ast.decls.get(d).ty()?;
    match ast.types.canonical_view(ty) {
        TypeView::Function(p) => Some(ast.types.is_void(p.ret())),
        _ => None,
    }
}

/// `diagnostics` as Kiln's text sink writes them to standard error, with
/// the default options (no colour).
fn render(sm: &SourceManager, diagnostics: &[kiln_diag::Diagnostic]) -> String {
    let buffer = SharedBuffer::new();
    let mut sink = TextSink::new(buffer.clone(), &DiagOptions::default());
    for d in diagnostics {
        sink.emit(sm, d);
    }
    sink.flush();
    buffer.contents()
}

/// A [`Handler`] as Kiln's handler.
struct Calls<'h> {
    handler: &'h mut dyn Handler,
}

impl HostCalls for Calls<'_> {
    fn call(
        &mut self,
        op: u32,
        ints: &[u64],
        bytes: &[u8],
        out: &mut HostOut<'_>,
    ) -> Result<u64, HostError> {
        self.handler.call(op, ints, bytes, &mut KilnOut(out)).map_err(HostError::new)
    }
}

/// Kiln's `HostOut` as an [`Out`].
struct KilnOut<'o, 'k>(&'o mut HostOut<'k>);

impl Out for KilnOut<'_, '_> {
    fn int_capacity(&self) -> usize {
        self.0.int_capacity()
    }
    fn byte_capacity(&self) -> usize {
        self.0.byte_capacity()
    }
    fn write_ints(&mut self, v: &[u64]) {
        self.0.write_ints(v);
    }
    fn write_bytes(&mut self, v: &[u8]) {
        self.0.write_bytes(v);
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use kiln_source::{LazyFile, SourceManager};
    use slip_model::hash::content_digest;

    use super::{ReadFile, embed_line, read_files};

    /// A file of this crate, by absolute path.
    fn own(name: &str) -> String {
        format!("{}/src/{name}", env!("CARGO_MANIFEST_DIR"))
    }

    fn read(path: &str) -> ReadFile {
        ReadFile {
            path: path.as_bytes().to_vec(),
            digest: content_digest(&std::fs::read(path).unwrap()),
        }
    }

    // WP-080 fix round, M16 and M18: one entry per file read (Kiln's buffer
    // identity, however the file is spelled); a virtual buffer is not
    // listed; a lazy buffer is listed once read, not before.
    #[test]
    fn files_are_each_buffer_read_once() {
        let sm = SourceManager::new();
        let session = own("session.rs");
        let parse = own("parse.rs");
        let a = sm.load_file(Path::new(&session)).unwrap();
        let again = format!("{}/src/../src/session.rs", env!("CARGO_MANIFEST_DIR"));
        assert_eq!(sm.load_file(Path::new(&again)).unwrap(), a);
        sm.add_virtual("<built-in>", "#define X 1\n".into()).unwrap();
        let bytes = std::fs::read(&parse).unwrap();
        let len = u32::try_from(bytes.len()).unwrap();
        let hash = kiln_source::hash::content_hash(&bytes);
        let LazyFile::Added(lazy) = sm.add_lazy_file(&parse, Path::new(&parse), len, hash, None)
        else {
            panic!("a lazy buffer");
        };
        assert_eq!(read_files(&sm).0, [read(&session)], "the lazy buffer is not read yet");
        assert!(!sm.text(lazy).is_empty());
        assert_eq!(read_files(&sm).0, [read(&session), read(&parse)], "now it is");
    }

    // WP-080 fix round, F1: what the scan refuses, by line.
    #[test]
    fn embed_scan() {
        let cases: [(&str, Option<u32>); 14] = [
            ("int embed = 0;\n", None),
            ("#embed <x>\n", Some(1)),
            ("int a;\n  #  embed \"x\"\n", Some(2)),
            ("%:embed <x>\n", Some(1)),
            ("/* c */ # /* c */ embed <x>\n", Some(1)),
            ("#em\\\nbed <x>\n", Some(1)),
            ("#\\\n\\\nembed <x>\n", Some(3)),
            ("#\\  \r\nembed <x>\n", Some(2)),
            ("#if __has_embed(<x>)\n#endif\n", Some(1)),
            ("int a;\nauto s = \"__has_embed\";\n", Some(2)),
            ("// __has_embed\n", Some(1)),
            ("// # embed\n", Some(1)),
            ("#define E embed\n", Some(1)),
            ("int x; // embed\nembed y;\n", None),
        ];
        for (text, line) in cases {
            assert_eq!(embed_line(text.as_bytes()), line, "{text:?}");
        }
    }

    // WP-080 fix round, F1: the cost of the scan over the files of a
    // transform that includes `<vector>` and `<string>`.
    #[test]
    #[ignore = "a measure: --ignored --nocapture"]
    fn embed_scan_cost() {
        let dir = std::env::current_exe().unwrap().parent().unwrap().join("embed-scan-cost");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("t.cpp");
        std::fs::write(
            &file,
            "#include <vector>\n#include <string>\nconsteval void transform() {}\n",
        )
        .unwrap();
        let resource = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../resource/include");
        let cmd = super::TransformCommand::new(
            std::fs::canonicalize(resource).unwrap(),
            &[],
            file.clone(),
        )
        .unwrap();
        let files = std::thread::scope(|s| {
            std::thread::Builder::new()
                .stack_size(64 << 20)
                .spawn_scoped(s, || {
                    super::with_transform(&cmd, 63 << 20, |o| o.unwrap().files().to_vec())
                })
                .unwrap()
                .join()
                .unwrap()
        });
        let texts: Vec<Vec<u8>> = files
            .iter()
            .map(|f| std::fs::read(String::from_utf8_lossy(&f.path).as_ref()).unwrap())
            .collect();
        std::fs::remove_file(&file).unwrap();
        std::fs::remove_dir(&dir).unwrap();
        let bytes: usize = texts.iter().map(Vec::len).sum();
        let start = std::time::Instant::now();
        let hits = texts.iter().filter(|t| embed_line(t).is_some()).count();
        let took = start.elapsed();
        let start = std::time::Instant::now();
        let found = texts.iter().filter(|t| t.windows(5).any(|w| w == b"embed")).count();
        let substring = start.elapsed();
        eprintln!(
            "{} files, {bytes} bytes, {hits} hits, scan {took:?}; `embed` in {found}, a substring search alone {substring:?}",
            texts.len()
        );
    }
}
