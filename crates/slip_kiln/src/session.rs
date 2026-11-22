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

use kiln_ast::types::TypeView;
use kiln_diag::{DiagOptions, DiagSink, SharedBuffer, TextSink};
use kiln_driver::session::{SessionFrontEnd, SessionRefused, with_eval_session};
use kiln_driver::{Io, SharedWriter};
use kiln_oven::{HostCalls, HostError, HostOut};
use kiln_sema::lookup::LookupKind;
use kiln_sema::session::{
    CancelToken, EntryError as KilnEntryError, RunBudget, RunError, RunValue,
};
use kiln_sema::stack_guard::StackGuard;
use kiln_source::{FileOrigin, SourceManager};
use slip_model::Digest;
use slip_model::hash::content_digest;

/// The options of the command line before the author's (`spec/api.md`
/// §2.1 rule 1), after the program name.
const FIXED: [&str; 3] = ["-std=c++26", "-fsyntax-only", "-fkiln-reproducible-time"];

/// The author's options (§2.1 rule 2). Each takes one value, joined
/// (`-DX=1`) or as the next word (`-D X=1`).
const AUTHOR_OPTIONS: [&str; 5] = ["-isystem", "-include", "-I", "-D", "-U"];

/// The author's options whose value is never joined: clang and Kiln have
/// longer options that begin with them (`-include-pch`, `-isystem-after`).
const SEPARATE_ONLY: [&str; 2] = ["-isystem", "-include"];

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
    /// [`OptionRefused`] for the first word of `author` that is not one of
    /// `-I`, `-isystem`, `-D`, `-U`, `-include` with its value (joined only
    /// to `-I`, `-D` and `-U`), for one of those whose value is missing,
    /// empty or begins with `-`, and for a `file` or
    /// `resource_include` that is empty or not UTF-8 or that begins with
    /// `-` (Kiln would read it as an option).
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
            let value = if word.len() == opt.len() {
                match words.next() {
                    Some(v) => v.clone(),
                    None => return Err(refused("the option's value is missing")),
                }
            } else if SEPARATE_ONLY.contains(&opt) {
                // `-include-pch`, `-isystem-after`: other options.
                return Err(refused("-isystem and -include take their value as the next word"));
            } else {
                word[opt.len()..].to_owned()
            };
            if value.is_empty() {
                return Err(refused("the option's value is empty"));
            }
            if value.starts_with('-') {
                return Err(refused("the option's value begins with `-`"));
            }
            checked.push((opt, value));
        }
        for (what, path) in
            [("the transform file", &file), ("the resource directory", &resource_include)]
        {
            let word = path.to_string_lossy().into_owned();
            let refused = |reason| OptionRefused { word: word.clone(), reason };
            match path.to_str() {
                None => return Err(refused("the path is not UTF-8")),
                Some("") => return Err(refused("the path is empty")),
                Some(p) if p.starts_with('-') => {
                    return Err(refused(if what == "the transform file" {
                        "the transform file's path begins with `-`"
                    } else {
                        "the resource directory's path begins with `-`"
                    }));
                }
                Some(_) => {}
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
    /// Kiln panicked while compiling; the panic's message.
    Panicked(String),
}

/// One file the transform unit read: path as given, content digest.
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

/// A function of the transform unit that may be run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry(kiln_sema::session::Entry);

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
    },
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
}

/// The compiled transform unit (§2.4 rule 5). `Send`: a run may happen on
/// another thread (a subject unit's front end) while the thread of
/// [`with_transform`] waits.
pub struct Session<'a> {
    engine: &'a mut dyn Engine,
    files: Vec<ReadFile>,
}

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

    /// Whether a function or function template named `name` is declared in
    /// the global namespace (§2.3 rule 3).
    pub fn declares(&mut self, name: &str) -> bool {
        self.engine.declares(name)
    }

    /// The function `name` of the global namespace as an entry (§2.3
    /// rule 2).
    ///
    /// # Errors
    ///
    /// The first condition of §2.3 rule 2 it fails. A name that is not an
    /// identifier (`ns::f`) is [`EntryError::Missing`].
    pub fn entry(&mut self, name: &str) -> Result<Entry, EntryError> {
        self.engine.entry(name)
    }

    /// Runs `entry` once: host calls reach `handler`; the run stops at
    /// `budget` or when `cancel` is cancelled. The session is usable after
    /// any outcome.
    pub fn run(
        &mut self,
        entry: Entry,
        handler: &mut dyn Handler,
        budget: Budget,
        cancel: &Cancel,
    ) -> RunReport {
        self.engine.run(entry, handler, budget, &cancel.0)
    }
}

/// Compiles the transform on the calling thread and calls `f` with the
/// session. The caller's thread must have the stack it wants: `stack` is
/// the bytes of stack left below this call, and this installs Kiln's stack
/// guard for it (`kiln/crates/kiln_sema/src/stack_guard.rs`), which stays
/// installed on the thread after the call.
///
/// The unit's diagnostics are captured, not written: they are in
/// [`CompileError`].
pub fn with_transform<R>(
    cmd: &TransformCommand,
    stack: usize,
    f: impl FnOnce(Result<Session<'_>, CompileError>) -> R,
) -> R {
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
                    let files = read_files(fe.sources);
                    f(Ok(Session { engine: &mut fe, files }))
                }
                Err(SessionRefused::TuHasErrors { errors }) => {
                    f(Err(CompileError::Diagnosed { rendered: buffer.contents(), errors }))
                }
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

fn panic_message(p: &(dyn std::any::Any + Send)) -> String {
    p.downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| p.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "a panic without a message".to_owned())
}

/// The buffers read from disk, in the order Kiln created them, each
/// `(path, digest)` once.
fn read_files(sm: &SourceManager) -> Vec<ReadFile> {
    let mut files: Vec<ReadFile> = Vec::new();
    for id in sm.file_ids() {
        let bytes = match sm.file_origin(id) {
            FileOrigin::Virtual => continue,
            // A transform unit reads no module file or snapshot (the
            // options are refused); listed when read, as for a subject.
            FileOrigin::Disk | FileOrigin::Lazy => match sm.original_bytes(id) {
                Some(b) => b,
                None => continue,
            },
        };
        let file =
            ReadFile { path: sm.file_name(id).as_bytes().to_vec(), digest: content_digest(&bytes) };
        if !files.contains(&file) {
            files.push(file);
        }
    }
    files
}

/// The session with its lifetimes erased, so that [`Session`] has one.
trait Engine: Send {
    fn declares(&self, name: &str) -> bool;
    fn entry(&self, name: &str) -> Result<Entry, EntryError>;
    fn run(
        &mut self,
        entry: Entry,
        handler: &mut dyn Handler,
        budget: Budget,
        cancel: &CancelToken,
    ) -> RunReport;
}

impl Engine for SessionFrontEnd<'_, '_> {
    fn declares(&self, name: &str) -> bool {
        let sema = self.session.sema();
        let ast = &sema.ast;
        let found = sema.lookup_qualified(ast.tu, ast.symbols().intern(name), LookupKind::Ordinary);
        found.decls.iter().any(|&d| {
            use kiln_ast::decl::DeclKind;
            matches!(ast.decls.get(d).kind(), DeclKind::Function | DeclKind::FunctionTemplate)
        })
    }

    fn entry(&self, name: &str) -> Result<Entry, EntryError> {
        if name.is_empty() || name.contains(':') {
            return Err(EntryError::Missing);
        }
        let s = &self.session;
        let entry = match s.entry_named(&format!("::{name}")) {
            Ok(e) => e,
            Err(e) => {
                let mine = match e {
                    KilnEntryError::NotFound { .. } => EntryError::Missing,
                    KilnEntryError::Overloaded { .. } => EntryError::Overloaded,
                    KilnEntryError::NotConsteval => EntryError::NotConsteval,
                    KilnEntryError::HasParameters => EntryError::Parameters,
                    KilnEntryError::WrongReturnType => EntryError::Type,
                    // §2.3 puts `entry-type` before `entry-undefined`;
                    // Kiln checks the return type for an integral one only.
                    KilnEntryError::Undefined => match returns_void(self, name) {
                        Some(false) => EntryError::Type,
                        _ => EntryError::Undefined,
                    },
                    // `NotAFunction`, and any reason Kiln adds
                    // (`#[non_exhaustive]`): not an entry.
                    _ => EntryError::NotFunction,
                };
                return Err(mine);
            }
        };
        if returns_void(self, name) == Some(false) {
            return Err(EntryError::Type);
        }
        Ok(Entry(entry))
    }

    fn run(
        &mut self,
        entry: Entry,
        handler: &mut dyn Handler,
        budget: Budget,
        cancel: &CancelToken,
    ) -> RunReport {
        let budget = RunBudget { steps: budget.steps, bytes: budget.bytes, depth: budget.depth };
        let mut calls = Calls { handler };
        let report = self.session.run(entry.0, &mut calls, budget, cancel);
        let result = match report.result {
            Ok(RunValue::Void) => Ok(()),
            Ok(v) => unreachable!("an entry returns void (Session::entry), and returned {v:?}"),
            Err(e) => Err(match e {
                RunError::Failed => RunFailure::NotConstant,
                RunError::StepBudget => RunFailure::StepBudget,
                RunError::MemoryBudget => RunFailure::MemoryBudget,
                RunError::DepthLimit => RunFailure::DepthLimit,
                RunError::Cancelled => RunFailure::Cancelled,
                RunError::Host { message } => RunFailure::Host(message),
                RunError::Uncaught { type_name, .. } => RunFailure::Uncaught { type_name },
                // A kind Kiln adds (`#[non_exhaustive]`): the host cannot
                // say what it means.
                other => RunFailure::Host(format!("an unknown failure of Kiln: {other:?}")),
            }),
        };
        RunReport {
            result,
            steps: report.steps,
            host_calls: report.host_calls,
            peak_bytes: report.peak_heap,
            rendered: render(self.sources, &report.diagnostics),
        }
    }
}

/// Whether the one function `::name` returns `void` (`None`: there is not
/// exactly one function of that name).
fn returns_void(fe: &SessionFrontEnd<'_, '_>, name: &str) -> Option<bool> {
    let sema = fe.session.sema();
    let ast = &sema.ast;
    let found = sema.lookup_qualified(ast.tu, ast.symbols().intern(name), LookupKind::Ordinary);
    let [d] = found.decls.as_slice() else { return None };
    let ty = ast.decls.get(*d).ty()?;
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
