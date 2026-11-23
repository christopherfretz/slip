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

use kiln_ast::decl::DeclKind;
use kiln_ast::types::TypeView;
use kiln_ast::{Ast, DeclId};
use kiln_diag::{DiagOptions, DiagSink, SharedBuffer, TextSink};
use kiln_driver::session::{SessionFrontEnd, SessionRefused, with_eval_session};
use kiln_driver::{Io, SharedWriter};
use kiln_oven::{HostCalls, HostError, HostOut};
use kiln_sema::session::{
    CancelToken, Entry as KilnEntry, EntryError as KilnEntryError, RunBudget, RunError, RunValue,
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
    /// Kiln panicked while compiling; the panic's message.
    Panicked(String),
    /// A file of the unit may use `#embed` or `__has_embed`, which a
    /// transform may not at this Kiln revision: the resources they read
    /// are not in [`Session::files`], so not in the transform's identity.
    /// The file (its path as in [`ReadFile`]) and the line (from 1).
    Embed { path: Vec<u8>, line: u32 },
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
    /// An earlier run's handler panicked: the session is unusable (Kiln
    /// did not put its state back), and this run did not reach Kiln. A
    /// worker ends.
    Poisoned,
    /// The entry was given by another session; this run did not reach
    /// Kiln.
    ForeignEntry,
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

/// The compiled transform unit (§2.4 rule 5). `Send`, so that a run may
/// be called from another thread (a subject unit's front end) while the
/// thread of [`with_transform`] waits; see [`Session::run`] for the hazard
/// of doing so at this Kiln revision.
pub struct Session<'a> {
    engine: &'a mut dyn Engine,
    files: Vec<ReadFile>,
    /// This session's number, which its entries carry.
    id: u64,
    /// Set while a run is inside Kiln: still set after a run unwound.
    in_run: bool,
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

    /// Runs `entry` once: host calls reach `handler`; the run stops at
    /// `budget` or when `cancel` is cancelled. The session is usable after
    /// any outcome of the run itself.
    ///
    /// A panic of `handler` unwinds out of this call, and the session is
    /// then unusable: every later run is [`RunFailure::Poisoned`] (Kiln
    /// has not put its state back). A worker ends.
    ///
    /// **The stack guard.** Kiln checks the stack against the guard of the
    /// thread that called [`with_transform`], copied when the unit was
    /// compiled (`kiln/crates/kiln_sema/src/templ/mod.rs:617`), not against
    /// the calling thread's. On another thread that guard can read as
    /// exhausted. A run that reaches a stack check (an instantiation, a
    /// nested constant evaluation) then fails with "stack exhausted", and
    /// the flag is sticky: every later run of the session fails the same
    /// way. A run that reaches no check is not affected. Kiln is asked to
    /// lift this (K2, `docs/wp/WP-080.md`).
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
        };
        if entry.session != self.id {
            return refused(RunFailure::ForeignEntry);
        }
        if self.in_run {
            return refused(RunFailure::Poisoned);
        }
        self.in_run = true;
        let report = self.engine.run(entry.entry, handler, budget, &cancel.0);
        self.in_run = false;
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
                    f(Ok(Session { engine: &mut fe, files, id, in_run: false }))
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
            FileOrigin::Disk | FileOrigin::Lazy => match sm.original_bytes(id) {
                Some(b) => b,
                None => continue,
            },
        };
        let path = sm.file_name(id).as_bytes().to_vec();
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
    fn entry(&self, name: &str) -> Result<KilnEntry, EntryError>;
    fn run(
        &mut self,
        entry: KilnEntry,
        handler: &mut dyn Handler,
        budget: Budget,
        cancel: &CancelToken,
    ) -> RunReport;
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
        match self.session.entry(d) {
            Ok(_) if void == Some(false) => Err(EntryError::Type),
            Ok(e) => Ok(e),
            Err(e) => Err(match e {
                KilnEntryError::NotFound { .. } => EntryError::Missing,
                KilnEntryError::Overloaded { .. } => EntryError::Overloaded,
                KilnEntryError::NotConsteval => EntryError::NotConsteval,
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

    fn run(
        &mut self,
        entry: KilnEntry,
        handler: &mut dyn Handler,
        budget: Budget,
        cancel: &CancelToken,
    ) -> RunReport {
        let budget = RunBudget { steps: budget.steps, bytes: budget.bytes, depth: budget.depth };
        let mut calls = Calls { handler };
        let report = self.session.run(entry, &mut calls, budget, cancel);
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
