//! The run driver (`spec/api.md` §2, §12; work package WP-089): compile a
//! transform once, check its headers and its entry, then run `transform()`
//! against one subject unit at a time and classify what happened.
//!
//! The one place that joins the transform session (`slip_kiln::session`,
//! WP-080), the host ([`Host`], WP-081) and a subject ([`Subject`]).
//! Decisions the specification leaves to this module are in
//! `spec/landed/api/WP-089.md`.
//!
//! **The thread.** A [`Loaded`] is run only on the thread that loaded it
//! (`spec/api.md` §2.5 rule 5): at this Kiln revision a run on another
//! thread is checked against the loading thread's stack guard, and can
//! fail "stack exhausted" by the history of the threads rather than by
//! the transform. The loading thread must have [`STACK`] bytes of stack:
//! then every recursion a run can drive reaches one of Kiln's own limits
//! before Kiln's stack guard.
//!
//! **What ends the use of a transform.** Four things leave the session in
//! a state no later run may trust, and the driver then *retires* the
//! [`Loaded`]: every later [`Loaded::run`] is `Err` without reaching Kiln,
//! and [`Loaded::retired`] says why. They are a host error (a Slip bug,
//! §2.8 `host-failure`), a panic during a run (of the subject, the host
//! or Kiln), Kiln's stack guard tripping (sticky in Kiln, see
//! [`slip_kiln::session::Session::run`]), and a failure of a kind the host
//! does not know. A worker that sees a retired transform ends.

use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::thread::ThreadId;
use std::time::Instant;

pub use slip_kiln::session::SourcePlace;
use slip_kiln::session::{
    self, Budget, Cancel, CompileError, Entry, EntryError, ReadFile, RunDiagnostic, RunFailure,
    RunReport, Session, Severity, TransformCommand,
};
use slip_model::wire::Encoder;
use slip_model::{Counter, Digest};

use crate::host::{self, Host, Limits};
use crate::output::RunOutput;
use crate::resource::{self, HeaderError};
use crate::subject::Subject;
use crate::wire::{self, Errc};

/// The tag of the transform id (`spec/api.md` §2.2, `spec/model.md` §3).
pub const TRANSFORM_ID_TAG: &str = "slip.transform.v0";

/// The entry point of version 0 (§2.3 rule 1).
pub const ENTRY: &str = "transform";

/// The names reserved for entries of later versions (§2.3 rule 3).
pub const RESERVED: [&str; 3] = ["reduce", "prepare", "repair"];

/// The type of the exception every function of `<slip/meta.h>` throws
/// (`slip::v0::error`), as Kiln names the type of an uncaught exception at
/// this pin: as written, unqualified
/// (`kiln/crates/kiln_oven/src/notes.rs:237`).
pub const ERROR_TYPE: &str = "error";

/// The stack the thread that loads a transform must have, in bytes
/// (`spec/api.md` §2.5 rule 5): reserved address space, touched only as
/// deep as a run recurses. With it and the template depth
/// `slip_kiln::session::TEMPLATE_DEPTH`, each recursion a run can drive
/// reaches Kiln's own limit before Kiln's stack guard in a debug build,
/// with a margin (`spec/landed/api/WP-089.md`, "The stack").
pub const STACK: usize = 256 << 20;

/// The host's own term of the transform id (§2.2): the versions of Slip
/// and of the Kiln it links, as `slip --version` writes them.
pub fn host_version() -> String {
    format!("slip {} (kiln {})", env!("CARGO_PKG_VERSION"), slip_kiln::kiln_version())
}

/// Why a transform did not load (`spec/api.md` §12). [`LoadError::name`]
/// is the error's name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoadError {
    /// `transform-option`: the refusal of `slip_kiln::session`, as text.
    Option(String),
    /// `transform-compile`: Kiln's diagnostics as it renders them, or the
    /// refusal of `#embed`, naming the file and the line.
    Compile { rendered: String },
    /// `transform-header`.
    Header(HeaderError),
    /// One of the `entry-*` errors of §2.3, by its name.
    Entry(&'static str),
    /// `host-failure`: not the transform's fault. Kiln panicked while
    /// compiling, Kiln's stack guard tripped while compiling (the host's
    /// limit), or the thread was given less stack than [`STACK`]. Kiln's
    /// rendering, if it wrote one.
    Host { message: String, rendered: String },
}

impl LoadError {
    /// The name of the error in `spec/api.md` §12.
    pub fn name(&self) -> &'static str {
        match self {
            LoadError::Option(_) => "transform-option",
            LoadError::Compile { .. } => "transform-compile",
            LoadError::Header(_) => HeaderError::NAME,
            LoadError::Entry(name) => name,
            LoadError::Host { .. } => "host-failure",
        }
    }
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::Option(m) => write!(f, "transform-option: {m}"),
            LoadError::Compile { rendered } => write!(f, "transform-compile:\n{rendered}"),
            LoadError::Header(e) => write!(f, "{e}"),
            LoadError::Entry(name) => write!(f, "{name}: {}", entry_condition(name)),
            LoadError::Host { message, rendered } => {
                write!(f, "host-failure: {message}")?;
                if !rendered.is_empty() {
                    write!(f, "\n{rendered}")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for LoadError {}

/// The condition of §2.3 rule 2 (or rule 3) that the error `name` names.
fn entry_condition(name: &str) -> &'static str {
    match name {
        "entry-missing" => "no declaration named `transform` in the global namespace",
        "entry-overloaded" => "the name `transform` designates more than one function",
        "entry-not-function" => "`transform` is not a function",
        "entry-not-consteval" => "`transform` is not consteval",
        "entry-parameters" => "`transform` has a parameter",
        "entry-type" => "`transform` returns something other than void",
        "entry-undefined" => "`transform` is declared and not defined",
        "entry-reserved" => {
            "a function named `reduce`, `prepare` or `repair` is declared in the global \
             namespace; the names are reserved for later versions"
        }
        _ => "an error of the entry",
    }
}

/// The name in §12 of an entry error of `slip_kiln::session`.
fn entry_error_name(e: EntryError) -> &'static str {
    match e {
        EntryError::Missing => "entry-missing",
        EntryError::Overloaded => "entry-overloaded",
        EntryError::NotFunction => "entry-not-function",
        EntryError::NotConsteval => "entry-not-consteval",
        EntryError::Parameters => "entry-parameters",
        EntryError::Type => "entry-type",
        EntryError::Undefined => "entry-undefined",
    }
}

/// The budgets of a run (`spec/api.md` §2.5 rule 1). `Default` is the
/// table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Budgets {
    pub steps: u64,
    pub bytes: u64,
    pub depth: u32,
}

impl Default for Budgets {
    fn default() -> Budgets {
        Budgets { steps: 1 << 32, bytes: 256 << 20, depth: 4096 }
    }
}

/// The kind of a failed run (`spec/api.md` §2.8; `spec/edits.md` §4.6
/// `FailureKind`, whose numbers [`FailureKind::code`] gives).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FailureKind {
    Uncaught,
    NotConstant,
    StepBudget,
    MemoryBudget,
    DepthLimit,
    HostFailure,
}

impl FailureKind {
    /// The name in §2.8.
    pub fn name(self) -> &'static str {
        match self {
            FailureKind::Uncaught => "uncaught",
            FailureKind::NotConstant => "not-constant",
            FailureKind::StepBudget => "step-budget",
            FailureKind::MemoryBudget => "memory-budget",
            FailureKind::DepthLimit => "depth-limit",
            FailureKind::HostFailure => "host-failure",
        }
    }

    /// The number of `spec/edits.md` §4.6.
    pub fn code(self) -> u64 {
        match self {
            FailureKind::Uncaught => 0,
            FailureKind::NotConstant => 1,
            FailureKind::StepBudget => 2,
            FailureKind::MemoryBudget => 3,
            FailureKind::DepthLimit => 4,
            FailureKind::HostFailure => 5,
        }
    }
}

impl fmt::Display for FailureKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// The record of a failed run: `spec/edits.md` §4.6's `Failure`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub kind: FailureKind,
    /// The cause, for a person (§4.6): for an uncaught `slip::error` with
    /// a refusal, the refusal; else the first note of Kiln's that explains
    /// the failure, or the failure's own message when it has none; for a
    /// failure of the host, what failed. Never a place or a path.
    pub message: String,
    /// Where in the transform's source the failure arose (§4.6): the place
    /// of the cause or, when that is in a header of the resource
    /// directory, the first place of the call stack outside them.
    pub place: Option<SourcePlace>,
    /// The run's diagnostics as Kiln renders them, the call stack
    /// included.
    pub rendered: String,
    /// For an uncaught `slip::error`: the refusal of the run's failure
    /// list it reports (the most recent; `spec/api.md` §2.8 rule 3 says
    /// when that is wrong), with the place in the subject the refused
    /// value denotes.
    pub refusal: Option<host::Failure>,
}

/// What a run gave (§2.8).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The run output (§7.6), with the evaluator's counters added.
    Completed(RunOutput),
    /// A failed run: no effect of it is kept (§2.8 rule 1).
    Failed(Failure),
}

/// What a run cost (§7.6 rule 6 for the first three; `wall_ns` is not
/// deterministic and is never a counter).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RunStats {
    pub steps: u64,
    pub host_calls: u64,
    pub peak_bytes: u64,
    pub wall_ns: u64,
}

/// A compiled transform whose headers and entry were checked: ready to
/// run against subject units, on the thread that loaded it.
pub struct Loaded<'s> {
    session: Session<'s>,
    /// The entry, taken from `session` and never given out: no run can be
    /// given an entry of another session (`RunFailure::ForeignEntry`).
    entry: Entry,
    id: Digest,
    /// The thread that loaded the transform: the only one that runs it.
    thread: ThreadId,
    /// `<resource>/include`, whose headers are the host's.
    include: PathBuf,
    /// Why the use of the transform ended, once it has.
    retired: Option<String>,
}

impl fmt::Debug for Loaded<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Loaded")
            .field("id", &self.id)
            .field("retired", &self.retired)
            .finish_non_exhaustive()
    }
}

/// Compiles the transform `file` with the author's options `author` and
/// the resource directory `resource` (`spec/api.md` §2.1) on the calling
/// thread, checks it in the order of `docs/wp/WP-089.md` rule 1 (options,
/// compile, headers, reserved names, the entry; the first that fails is
/// the error), and calls `f`.
///
/// The calling thread needs `stack` bytes of stack below this call, and
/// `stack` is trusted: it installs Kiln's stack guard on the thread
/// ([`slip_kiln::session::with_transform`]). A `stack` below [`STACK`] is
/// `host-failure`, before anything is compiled. The loaded transform is
/// run only on this thread ([`Loaded::run`]).
pub fn with_transform<R>(
    resource: &Path,
    author: &[String],
    file: &Path,
    stack: usize,
    f: impl FnOnce(Result<Loaded<'_>, LoadError>) -> R,
) -> R {
    if stack < STACK {
        return f(Err(LoadError::Host {
            message: format!(
                "the thread was given {stack} bytes of stack and a transform needs {STACK} \
                 (spec/api.md §2.5 rule 5)"
            ),
            rendered: String::new(),
        }));
    }
    with_transform_on_stack(resource, author, file, stack, f)
}

/// [`with_transform`] for any `stack`, [`STACK`] not checked: for tests
/// and measurements of the host's own limit. Below [`STACK`], Kiln's stack
/// guard can trip before Kiln's limits, which is a host failure.
pub fn with_transform_on_stack<R>(
    resource: &Path,
    author: &[String],
    file: &Path,
    stack: usize,
    f: impl FnOnce(Result<Loaded<'_>, LoadError>) -> R,
) -> R {
    let cmd = match TransformCommand::new(resource.join("include"), author, file.to_path_buf()) {
        Ok(cmd) => cmd,
        Err(refused) => return f(Err(LoadError::Option(refused.to_string()))),
    };
    session::with_transform(&cmd, stack, |opened| f(load(&cmd, resource, opened)))
}

/// The checks after the compile, in order.
fn load<'s>(
    cmd: &TransformCommand,
    resource: &Path,
    opened: Result<Session<'s>, CompileError>,
) -> Result<Loaded<'s>, LoadError> {
    let mut session = match opened {
        Ok(s) => s,
        Err(CompileError::Diagnosed { rendered, .. }) => {
            return Err(LoadError::Compile { rendered });
        }
        Err(CompileError::Embed { path, line }) => {
            return Err(LoadError::Compile {
                rendered: format!(
                    "{}:{line}: error: `#embed` is not supported in a transform \
                     (spec/api.md §2.1 rule 6)\n",
                    String::from_utf8_lossy(&path)
                ),
            });
        }
        // Kiln stopped before the unit was compiled: a file that cannot be
        // read, or the front end stopping for a reason of its own. What
        // the author needs is Kiln's text (decision 2 of the landed file).
        Err(CompileError::CommandLine(rendered)) => {
            let rendered = if rendered.is_empty() {
                "kiln stopped before compiling the transform unit, and said nothing\n".to_owned()
            } else {
                rendered
            };
            return Err(LoadError::Compile { rendered });
        }
        // The thread's stack was too small for the unit: the host's limit
        // (§2.5 rule 5), not the author's error.
        Err(CompileError::StackExhausted { rendered }) => {
            return Err(LoadError::Host {
                message: "Kiln's stack guard tripped while compiling the transform: the host's \
                          thread has too little stack for it, which is the host's limit and \
                          not an error of the transform"
                    .to_owned(),
                rendered,
            });
        }
        Err(CompileError::Panicked(m)) => {
            return Err(LoadError::Host {
                message: format!("Kiln panicked compiling the transform: {m}"),
                rendered: String::new(),
            });
        }
    };
    let read: Vec<(Vec<u8>, Digest)> =
        session.files().iter().map(|f| (f.path.clone(), f.digest)).collect();
    resource::check_headers(&read, resource).map_err(LoadError::Header)?;
    if RESERVED.iter().any(|name| session.declares(name)) {
        return Err(LoadError::Entry("entry-reserved"));
    }
    let entry = session.entry(ENTRY).map_err(|e| LoadError::Entry(entry_error_name(e)))?;
    let id = transform_id(
        resource::API_LEVEL,
        &host_version(),
        &cmd.command_line(),
        session.triple(),
        session.predefines(),
        session.files(),
    );
    Ok(Loaded {
        session,
        entry,
        id,
        thread: std::thread::current().id(),
        include: cmd.resource_include().to_path_buf(),
        retired: None,
    })
}

/// `TransformId` (`spec/api.md` §2.2): `H("slip.transform.v0", v)` where
/// `v` is the record of
///
/// ```text
/// api_level:     uv
/// host:          str          the host's versions (host_version)
/// command_line:  list<str>    the words after the program name
/// target:        str          the triple
/// predefines:    digest       content digest of the predefines buffer
/// files:         list<(path: bytes, content: digest)>   in the order first read
/// ```
pub fn transform_id(
    api_level: u64,
    host: &str,
    command_line: &[String],
    triple: &str,
    predefines: &[u8],
    files: &[ReadFile],
) -> Digest {
    let mut e = Encoder::new();
    e.uv(api_level);
    e.str(host);
    e.count(command_line.len());
    for word in command_line {
        e.str(word);
    }
    e.str(triple);
    e.digest(&slip_model::hash::content_digest(predefines));
    e.count(files.len());
    for f in files {
        e.bytes(&f.path);
        e.digest(&f.digest);
    }
    slip_model::hash::tagged_digest(TRANSFORM_ID_TAG, e.as_bytes())
}

/// The refusal of a run on another thread than the loading one.
pub const OTHER_THREAD: &str = "a loaded transform is run only on the thread that loaded it \
     (spec/api.md §2.5 rule 5); the run was refused and did not reach Kiln";

impl Loaded<'_> {
    /// The transform id (§2.2).
    pub fn id(&self) -> Digest {
        self.id
    }

    /// The files the transform unit read (§2.2).
    pub fn files(&self) -> &[ReadFile] {
        self.session.files()
    }

    /// The target's triple, a term of the id (§2.1 rule 3, §2.2).
    pub fn triple(&self) -> &str {
        self.session.triple()
    }

    /// The bytes of the predefines buffer, whose digest is a term of the
    /// id (§2.2).
    pub fn predefines(&self) -> &[u8] {
        self.session.predefines()
    }

    /// Why the use of this transform ended, if it has (module
    /// documentation). Every later [`Loaded::run`] is then `Err`.
    pub fn retired(&self) -> Option<&str> {
        self.retired.as_deref()
    }

    /// Runs `transform()` once against `subject` (§2.4 rule 1) with a new
    /// [`Host`] (§2.4 rule 3) and `budgets`, and classifies the result
    /// (§2.8).
    ///
    /// # Errors
    ///
    /// An error of the worker, with no outcome for the subject unit:
    ///
    /// - the call is not on the thread that loaded the transform (§2.5
    ///   rule 5): [`OTHER_THREAD`]; nothing reaches Kiln and nothing is
    ///   retired;
    /// - the transform was retired by an earlier run ([`Loaded::retired`]);
    /// - the run was cancelled (§2.5 rule 4: a cancelled run has no result
    ///   at all).
    ///
    /// A host failure is an outcome (`host-failure`, so that the subject
    /// unit's failure is recorded with its diagnostics) *and* retires the
    /// transform: the worker learns it from [`Loaded::retired`] at once and
    /// from the next run. A panic during the run (of the subject, the host
    /// or Kiln) is contained and is such a failure.
    pub fn run(
        &mut self,
        subject: &(dyn Subject + Sync),
        budgets: Budgets,
        cancel: &Cancel,
    ) -> Result<(Outcome, RunStats), String> {
        if std::thread::current().id() != self.thread {
            return Err(OTHER_THREAD.to_owned());
        }
        if let Some(why) = &self.retired {
            return Err(format!("the use of this transform has ended: {why}"));
        }
        if self.session.stack_exhausted() {
            let why =
                "host-failure: Kiln's stack guard had tripped in the transform session".to_owned();
            self.retired = Some(why.clone());
            return Err(format!("the use of this transform has ended: {why}"));
        }
        let mut handler = Calls {
            host: Host::new(subject, self.id, Limits::default()),
            in_run: false,
            host_error: None,
        };
        let budget = Budget { steps: budgets.steps, bytes: budgets.bytes, depth: budgets.depth };
        let start = Instant::now();
        handler.in_run = true;
        let (session, entry) = (&mut self.session, self.entry);
        let ran =
            catch_unwind(AssertUnwindSafe(|| session.run(entry, &mut handler, budget, cancel)));
        handler.in_run = false;
        let wall_ns = u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX);
        match ran {
            Ok(report) => {
                let stack_exhausted = self.session.stack_exhausted();
                self.conclude(report, handler, stack_exhausted, wall_ns)
            }
            Err(panic) => Ok(self.panicked(&session::panic_message(panic.as_ref()), wall_ns)),
        }
    }

    /// A run that panicked: the session is not used again (Kiln did not
    /// put its state back), the transform is retired at once, and the
    /// unit's outcome is the host's failure.
    fn panicked(&mut self, message: &str, wall_ns: u64) -> (Outcome, RunStats) {
        let message = format!("the run panicked: {message}");
        self.retired = Some(format!("host-failure: {message}"));
        let failure = Failure {
            kind: FailureKind::HostFailure,
            message,
            place: None,
            rendered: String::new(),
            refusal: None,
        };
        (Outcome::Failed(failure), RunStats { wall_ns, ..RunStats::default() })
    }

    /// Classifies a finished run and retires the transform when the run
    /// left it untrustworthy.
    fn conclude(
        &mut self,
        report: RunReport,
        handler: Calls<'_>,
        stack_exhausted: bool,
        wall_ns: u64,
    ) -> Result<(Outcome, RunStats), String> {
        let stats = RunStats {
            steps: report.steps,
            host_calls: report.host_calls,
            peak_bytes: report.peak_bytes,
            wall_ns,
        };
        match classify(report, handler, stack_exhausted, &self.include) {
            Classified::Outcome(Outcome::Completed(out)) => {
                Ok((Outcome::Completed(with_evaluator_counters(out, &stats)), stats))
            }
            Classified::Outcome(outcome) => Ok((outcome, stats)),
            Classified::Retire(outcome, why) => {
                self.retired = Some(why);
                Ok((outcome, stats))
            }
            Classified::WorkerError(why, retire) => {
                if retire {
                    self.retired = Some(why.clone());
                }
                Err(why)
            }
        }
    }
}

/// The handler a run gives Kiln: the host, behind the flag of
/// `docs/wp/WP-089.md` rule 6.
struct Calls<'s> {
    host: Host<'s>,
    /// Set by the driver for exactly the duration of `Session::run`; a
    /// call while it is clear is refused.
    in_run: bool,
    /// The host's own error, if a call returned one or wrote more than
    /// Kiln gave it room for (§2.8 `host-failure`).
    host_error: Option<String>,
}

/// The text of a call refused because no run was under way.
const OUTSIDE_A_RUN: &str = "a host call outside a run of `transform()` (spec/api.md §2.6)";

impl session::Handler for Calls<'_> {
    fn call(
        &mut self,
        op: u32,
        ints: &[u64],
        bytes: &[u8],
        out: &mut dyn session::Out,
    ) -> Result<u64, String> {
        if !self.in_run {
            return Err(OUTSIDE_A_RUN.to_owned());
        }
        let mut counted = KilnOut { out, ints: 0, bytes: 0 };
        let r = self.host.call(op, ints, bytes, &mut counted);
        let error = match &r {
            Err(e) => Some(e.clone()),
            Ok(_) => counted.overrun(),
        };
        if let Some(e) = error
            && self.host_error.is_none()
        {
            self.host_error = Some(e);
        }
        r
    }
}

/// `slip_kiln::session::Out` as the host's [`host::Out`], counting what
/// the host writes: more than the capacity is the host's error, which Kiln
/// would report as a failed call
/// (`kiln/crates/kiln_oven/src/vm/host.rs:140-148`).
struct KilnOut<'o> {
    out: &'o mut dyn session::Out,
    ints: usize,
    bytes: usize,
}

impl KilnOut<'_> {
    /// The host's error if it wrote more than there was room for.
    fn overrun(&self) -> Option<String> {
        let (ci, cb) = (self.out.int_capacity(), self.out.byte_capacity());
        if self.ints > ci {
            Some(format!("the host wrote {} integers into a capacity of {ci}", self.ints))
        } else if self.bytes > cb {
            Some(format!("the host wrote {} bytes into a capacity of {cb}", self.bytes))
        } else {
            None
        }
    }
}

impl host::Out for KilnOut<'_> {
    fn int_capacity(&self) -> usize {
        self.out.int_capacity()
    }
    fn byte_capacity(&self) -> usize {
        self.out.byte_capacity()
    }
    fn write_ints(&mut self, v: &[u64]) {
        self.ints = self.ints.saturating_add(v.len());
        self.out.write_ints(v);
    }
    fn write_bytes(&mut self, v: &[u8]) {
        self.bytes = self.bytes.saturating_add(v.len());
        self.out.write_bytes(v);
    }
}

/// What the driver makes of a run.
#[derive(Debug)]
enum Classified {
    /// An outcome for the subject unit; the transform stays usable.
    Outcome(Outcome),
    /// An outcome for the subject unit, and the transform is retired.
    Retire(Outcome, String),
    /// No outcome: an error of the worker; whether it retires.
    WorkerError(String, bool),
}

/// Kiln's diagnostic for a call of the entry that is not a constant
/// expression, whose first note of free text is the Oven's cause
/// (`kiln/crates/kiln_diag/src/ids/sema_e_m.rs:214`).
const NOT_A_CONSTANT_CALL: &str = "sema_immediate_call_non_constant";

/// The cause of a failed run and its place in the transform's source
/// (`spec/edits.md` §4.6), from Kiln's structured diagnostics.
///
/// The cause of the first error: when it is Kiln's diagnostic that the
/// entry's call is not a constant expression ([`NOT_A_CONSTANT_CALL`]),
/// its first note of free text (no diagnostic id), the Oven's note of what
/// went wrong, which comes before the notes of the call stack; else, as
/// for a limit of Kiln's reached in an instantiation made at run time, or
/// with no such note, the error itself.
/// The place is the cause's; if that is in a header of `include` (the
/// resource directory's), the first later note's place outside them, then
/// the error's own; if none is outside, the cause's. With no error at all,
/// `fallback` and no place.
fn cause(
    diagnostics: &[RunDiagnostic],
    include: &Path,
    fallback: &str,
) -> (String, Option<SourcePlace>) {
    let Some(primary) =
        diagnostics.iter().find(|d| matches!(d.severity, Severity::Error | Severity::Fatal))
    else {
        return (fallback.to_owned(), None);
    };
    let explained = primary.id == Some(NOT_A_CONSTANT_CALL);
    let Some(i) = primary.notes.iter().position(|n| n.id.is_none()).filter(|_| explained) else {
        return (primary.message.clone(), primary.place.clone());
    };
    let explaining = &primary.notes[i];
    let in_header = |p: &SourcePlace| {
        std::str::from_utf8(&p.file).is_ok_and(|f| Path::new(f).starts_with(include))
    };
    let place = match &explaining.place {
        Some(p) if in_header(p) => primary.notes[i + 1..]
            .iter()
            .filter_map(|n| n.place.as_ref())
            .chain(primary.place.as_ref())
            .find(|p| !in_header(p))
            .unwrap_or(p)
            .clone(),
        Some(p) => p.clone(),
        None => return (explaining.message.clone(), primary.place.clone()),
    };
    (explaining.message.clone(), Some(place))
}

fn failed(
    kind: FailureKind,
    (message, place): (String, Option<SourcePlace>),
    rendered: String,
) -> Outcome {
    Outcome::Failed(Failure { kind, message, place, rendered, refusal: None })
}

/// The name of a status (§3.5 rule 1).
fn status_name(status: u8) -> String {
    let errc = [
        Errc::InvalidHandle,
        Errc::WrongKind,
        Errc::InvalidArgument,
        Errc::NotEditable,
        Errc::Unsupported,
        Errc::LimitExceeded,
    ]
    .into_iter()
    .find(|e| e.status() == status);
    match (errc, status) {
        (Some(e), _) => e.name().to_owned(),
        (None, wire::STATUS_UNKNOWN_OP) => "status 254 (no such operation)".to_owned(),
        (None, wire::STATUS_MALFORMED) => "status 255 (a malformed call)".to_owned(),
        (None, s) => format!("status {s}"),
    }
}

/// A host failure that retires the transform: the unit's outcome and the
/// reason the worker sees.
fn host_failure(message: String, place: Option<SourcePlace>, rendered: String) -> Classified {
    let why = format!("host-failure: {message}");
    let failure =
        Failure { kind: FailureKind::HostFailure, message, place, rendered, refusal: None };
    Classified::Retire(Outcome::Failed(failure), why)
}

/// §2.8: what a run's report means, given whether Kiln's stack guard is
/// tripped after it and what the handler saw. The host's error first,
/// then the stack, then the run's own result (`spec/landed/api/WP-089.md`,
/// decision 3).
fn classify(
    report: RunReport,
    handler: Calls<'_>,
    stack_exhausted: bool,
    include: &Path,
) -> Classified {
    let RunReport { result, rendered, diagnostics, .. } = report;
    let cause = |fallback: &str| cause(&diagnostics, include, fallback);
    // The host's own error, whatever the run's result looks like: a Slip
    // bug (WP-089 rule 5), after which the host's state is not trusted.
    if let Some(error) = handler.host_error {
        return host_failure(format!("the host could not continue: {error}"), None, rendered);
    }
    // Kiln's guard is sticky: whatever the run's own failure looks like,
    // the session is not to be run again. On a thread of `STACK` bytes,
    // the thread that loaded it (§2.5 rule 5), not the transform's fault
    // but a defect of the host.
    if stack_exhausted {
        let (message, place) = cause("stack exhausted");
        return host_failure(
            format!(
                "Kiln's stack guard tripped during the run, and the transform session is not \
                 used again: {message}"
            ),
            place,
            rendered,
        );
    }
    match result {
        Ok(()) => {
            let Calls { host, .. } = handler;
            Classified::Outcome(Outcome::Completed(host.finish()))
        }
        // Without a host error: Kiln's own check of a call's arguments (a
        // transform calling the primitive with buffers that do not fit),
        // or a call with no handler. The evaluation did something that is
        // not a constant expression (§3.5 rules 4 and 5).
        Err(RunFailure::Host(note)) => {
            Classified::Outcome(failed(FailureKind::NotConstant, cause(&note), rendered))
        }
        Err(RunFailure::NotConstant) => Classified::Outcome(failed(
            FailureKind::NotConstant,
            cause("the evaluation is not a constant expression"),
            rendered,
        )),
        Err(RunFailure::StepBudget) => Classified::Outcome(failed(
            FailureKind::StepBudget,
            cause("the step budget is exhausted"),
            rendered,
        )),
        Err(RunFailure::MemoryBudget) => Classified::Outcome(failed(
            FailureKind::MemoryBudget,
            cause("the memory budget is exhausted"),
            rendered,
        )),
        Err(RunFailure::DepthLimit) => Classified::Outcome(failed(
            FailureKind::DepthLimit,
            cause("the depth limit is reached"),
            rendered,
        )),
        // Kiln names the type as written, unqualified (`error`), so a
        // class of the transform's own named `error` has the same name.
        // The refusal is reported only when the run has one, and it is the
        // most recent (decision 4 of the landed file; §2.8 rule 3 says when
        // that is wrong).
        Err(RunFailure::Uncaught { type_name }) => {
            let (own, place) = cause(&format!("an uncaught exception of type '{type_name}'"));
            match handler.host.last_failure().filter(|_| type_name == ERROR_TYPE) {
                Some(f) => Classified::Outcome(Outcome::Failed(Failure {
                    kind: FailureKind::Uncaught,
                    message: format!(
                        "an uncaught slip::error; the most recent refusal was: {} ({})",
                        f.message,
                        status_name(f.errc)
                    ),
                    place,
                    rendered,
                    refusal: Some(f.clone()),
                })),
                None => Classified::Outcome(failed(FailureKind::Uncaught, (own, place), rendered)),
            }
        }
        // A kind of failure this host does not know: it cannot say whose
        // fault it is, so it does not trust the session.
        Err(RunFailure::Unknown(kind)) => {
            let (message, place) = cause(&kind);
            host_failure(
                format!("Kiln reported a failure this host does not know ({kind}): {message}"),
                place,
                rendered,
            )
        }
        // §2.5 rule 4: no result at all. The session is usable once the
        // token is reset.
        Err(RunFailure::Cancelled) => Classified::WorkerError(
            "the run was cancelled: a cancelled run has no result (spec/api.md §2.5 rule 4)"
                .to_owned(),
            false,
        ),
        // A run that panicked retires the transform (`Loaded::run`), so no
        // later run reaches the session; were one to, it is refused.
        Err(RunFailure::Poisoned) => Classified::WorkerError(
            "internal error: the transform session is unusable after a panic of an earlier run"
                .to_owned(),
            true,
        ),
        // The driver owns the entry and takes it from its own session, so
        // this is not reached; were it, the driver is wrong.
        Err(RunFailure::ForeignEntry) => Classified::WorkerError(
            "internal error: the driver gave the session an entry of another session".to_owned(),
            true,
        ),
    }
}

/// Adds the evaluator's counters to a completed run's output (§7.6 rule
/// 6): the host makes the others, and none of these three.
fn with_evaluator_counters(mut out: RunOutput, stats: &RunStats) -> RunOutput {
    for (name, value) in [
        ("api.host_calls", stats.host_calls),
        ("api.peak_bytes", stats.peak_bytes),
        ("api.steps", stats.steps),
    ] {
        out.counters.push(Counter { name: name.to_owned(), value });
    }
    out.counters.sort();
    out
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use slip_kiln::session::{
        Cancel, CompileError, Handler, Out, RunDiagnostic, RunFailure, RunReport, Severity,
        SourcePlace, TransformCommand,
    };
    use slip_model::Digest;

    use super::{
        Budgets, Calls, Classified, Failure, FailureKind, KilnOut, LoadError, Loaded,
        OUTSIDE_A_RUN, Outcome, STACK, cause, classify, load, with_transform,
    };
    use crate::fake::FakeSubject;
    use crate::host::{self, Host, Limits};
    use crate::wire;

    const INCLUDE: &str = "/r/include";

    struct Room(usize, usize, Vec<u64>);

    impl Out for Room {
        fn int_capacity(&self) -> usize {
            self.0
        }
        fn byte_capacity(&self) -> usize {
            self.1
        }
        fn write_ints(&mut self, v: &[u64]) {
            self.2.extend_from_slice(v);
        }
        fn write_bytes(&mut self, _: &[u8]) {}
    }

    fn calls(s: &FakeSubject) -> Calls<'_> {
        Calls {
            host: Host::new(s, Digest([0; 32]), Limits::default()),
            in_run: false,
            host_error: None,
        }
    }

    fn report(result: Result<(), RunFailure>, rendered: &str) -> RunReport {
        RunReport {
            result,
            steps: 5,
            host_calls: 1,
            peak_bytes: 0,
            rendered: rendered.to_owned(),
            diagnostics: Vec::new(),
        }
    }

    #[allow(clippy::unnecessary_wraps, reason = "places are optional where they are used")]
    fn place(file: &str, line: u32) -> Option<SourcePlace> {
        Some(SourcePlace { file: file.as_bytes().to_vec(), line, column: 3 })
    }

    fn diag(
        id: Option<&'static str>,
        severity: Severity,
        message: &str,
        at: Option<SourcePlace>,
        notes: Vec<RunDiagnostic>,
    ) -> RunDiagnostic {
        RunDiagnostic { id, severity, message: message.to_owned(), place: at, notes }
    }

    fn note(id: Option<&'static str>, message: &str, at: Option<SourcePlace>) -> RunDiagnostic {
        diag(id, Severity::Note, message, at, Vec::new())
    }

    fn classify_(r: RunReport, c: Calls<'_>, stack: bool) -> Classified {
        classify(r, c, stack, Path::new(INCLUDE))
    }

    fn failure(c: &Classified) -> &Failure {
        match c {
            Classified::Outcome(Outcome::Failed(f)) | Classified::Retire(Outcome::Failed(f), _) => {
                f
            }
            other => panic!("{other:?}"),
        }
    }

    // WP-089 "Decided since" item 1: a `Host` over a `Sync` subject is
    // `Send` (so the driver's handler satisfies `slip_kiln::session::
    // Handler: Send`), and so is a loaded transform (a worker may move it,
    // though it runs it only on the loading thread). Checked when this
    // compiles.
    #[test]
    fn decided_1_host_and_loaded_are_send() {
        fn send<T: Send>() {}
        send::<Host<'static>>();
        send::<Calls<'static>>();
        send::<Loaded<'static>>();
    }

    // §2.5 rule 1: the default budgets are the table's.
    #[test]
    fn s2_5_rule1_the_default_budgets_are_the_table() {
        assert_eq!(
            Budgets::default(),
            Budgets { steps: 4_294_967_296, bytes: 268_435_456, depth: 4096 }
        );
    }

    // §2.8, `spec/edits.md` §4.6: the kinds' names and numbers.
    #[test]
    fn s2_8_the_kinds_names_and_codes() {
        let all = [
            (FailureKind::Uncaught, "uncaught", 0),
            (FailureKind::NotConstant, "not-constant", 1),
            (FailureKind::StepBudget, "step-budget", 2),
            (FailureKind::MemoryBudget, "memory-budget", 3),
            (FailureKind::DepthLimit, "depth-limit", 4),
            (FailureKind::HostFailure, "host-failure", 5),
        ];
        for (k, name, code) in all {
            assert_eq!((k.name(), k.code(), k.to_string()), (name, code, name.to_owned()));
        }
    }

    // WP-089 rule 6: the handler refuses a call while the driver's flag is
    // clear, without reaching the host; with the flag set the call reaches
    // it (an operation the host does not have: a refusal, not an error).
    #[test]
    fn rule6_the_handler_refuses_a_call_outside_a_run() {
        let s = FakeSubject::new();
        let mut c = calls(&s);
        let mut room = Room(0, 0, Vec::new());
        assert_eq!(c.call(9999, &[], &[], &mut room), Err(OUTSIDE_A_RUN.to_owned()));
        assert_eq!(c.host.failure_count(), 0, "the host was not reached");
        c.in_run = true;
        let word = c.call(9999, &[], &[], &mut room).unwrap();
        assert_eq!(wire::status(word), wire::STATUS_UNKNOWN_OP);
        assert_eq!(c.host.failure_count(), 1);
        assert_eq!(c.host_error, None, "a refusal is not a host error");
    }

    // L2: the host writing more than Kiln gave it room for is the host's
    // error; up to the capacity is not.
    #[test]
    fn l2_an_overrun_is_the_hosts_error() {
        use host::Out as _;
        let mut room = Room(2, 1, Vec::new());
        let mut k = KilnOut { out: &mut room, ints: 0, bytes: 0 };
        k.write_ints(&[1, 2]);
        k.write_bytes(&[7]);
        assert_eq!(k.overrun(), None);
        k.write_ints(&[3]);
        assert_eq!(k.overrun().as_deref(), Some("the host wrote 3 integers into a capacity of 2"));
        let mut room = Room(2, 1, Vec::new());
        let mut k = KilnOut { out: &mut room, ints: 0, bytes: 0 };
        k.write_bytes(&[7, 8]);
        assert_eq!(k.overrun().as_deref(), Some("the host wrote 2 bytes into a capacity of 1"));
    }

    // L1: a host error is checked first, whatever the run's result: a run
    // that completed, or failed another way, is a host failure that retires.
    #[test]
    fn l1_a_host_error_comes_first() {
        let s = FakeSubject::new();
        for result in [
            Ok(()),
            Err(RunFailure::NotConstant),
            Err(RunFailure::Uncaught { type_name: "error".to_owned() }),
            Err(RunFailure::Host("host call failed: x".to_owned())),
        ] {
            let mut c = calls(&s);
            c.host_error = Some("x".to_owned());
            let r = classify_(report(result.clone(), ""), c, false);
            let Classified::Retire(Outcome::Failed(f), why) = &r else {
                panic!("{result:?}: {r:?}")
            };
            assert_eq!(f.kind, FailureKind::HostFailure, "{result:?}");
            assert_eq!(f.message, "the host could not continue: x");
            assert_eq!(why, "host-failure: the host could not continue: x");
        }
        // Without one, Kiln's own note on a call is not-constant.
        let r = classify_(report(Err(RunFailure::Host("bad".to_owned())), ""), calls(&s), false);
        assert!(
            matches!(&r, Classified::Outcome(Outcome::Failed(f)) if f.kind == FailureKind::NotConstant && f.message == "bad"),
            "{r:?}"
        );
    }

    // L2: a failure of a kind this host does not know is a host failure
    // and retires; it is never not-constant.
    #[test]
    fn l2_an_unknown_kiln_failure_is_a_host_failure() {
        let s = FakeSubject::new();
        let r =
            classify_(report(Err(RunFailure::Unknown("Novel".to_owned())), ""), calls(&s), false);
        let Classified::Retire(Outcome::Failed(f), _) = &r else { panic!("{r:?}") };
        assert_eq!(f.kind, FailureKind::HostFailure);
        assert!(f.message.contains("does not know (Novel)"), "{}", f.message);
    }

    // WP-089 "Decided since" item 3: when Kiln's stack guard has tripped,
    // whatever the run's own failure, it is `host-failure` with Kiln's text
    // and the transform is retired; without the flag the same report is
    // `not-constant`.
    #[test]
    fn decided_3_stack_exhausted_is_host_failure_and_retires() {
        let s = FakeSubject::new();
        let text = "t.cpp:1:1: fatal error: kiln: stack exhausted during template instantiation; reduce nesting or raise -fkiln-stack-size\n";
        let mut rep = report(Err(RunFailure::NotConstant), text);
        rep.diagnostics = vec![diag(
            Some("sema_stack_exhausted"),
            Severity::Fatal,
            "kiln: stack exhausted during template instantiation; reduce nesting or raise -fkiln-stack-size",
            place("/t/t.cpp", 4),
            vec![note(
                Some("sema_note_in_instantiation"),
                "in instantiation",
                place("/t/t.cpp", 2),
            )],
        )];
        let r = classify_(rep.clone(), calls(&s), true);
        let Classified::Retire(Outcome::Failed(f), why) = &r else { panic!("{r:?}") };
        assert_eq!(f.kind, FailureKind::HostFailure);
        assert!(f.message.ends_with(": kiln: stack exhausted during template instantiation; reduce nesting or raise -fkiln-stack-size"), "{}", f.message);
        assert_eq!(f.place, place("/t/t.cpp", 4));
        assert_eq!(f.rendered, text);
        assert!(why.starts_with("host-failure: "), "{why}");
        // A run that completed while the flag tripped is not trusted either.
        let r = classify_(report(Ok(()), ""), calls(&s), true);
        assert!(
            failure(&r).kind == FailureKind::HostFailure && matches!(r, Classified::Retire(..))
        );
        let r = classify_(rep, calls(&s), false);
        assert!(
            matches!(&r, Classified::Outcome(Outcome::Failed(f)) if f.kind == FailureKind::NotConstant),
            "{r:?}"
        );
    }

    // M2, M3: the cause is the first note of free text, and its place,
    // when in the resource's headers, gives way to the first frame of the
    // call stack outside them, then to the error's own place.
    #[test]
    fn m2_m3_the_cause_and_its_place() {
        let include = Path::new(INCLUDE);
        let header = "/r/include/slip/meta.h";
        let in_call = |at| note(Some("oven_note_in_call"), "in call to 'f()'", at);
        let primary = |notes| {
            vec![
                diag(
                    Some("sema_warn"),
                    Severity::Warning,
                    "a warning",
                    place("/t/w.cpp", 1),
                    Vec::new(),
                ),
                diag(
                    Some("sema_immediate_call_non_constant"),
                    Severity::Error,
                    "call to consteval function 'transform' is not a constant expression",
                    place("/t/t.cpp", 9),
                    notes,
                ),
            ]
        };
        // A cause in the transform: its own place.
        let d = primary(vec![
            note(None, "division by zero", place("/t/t.cpp", 3)),
            in_call(place("/t/t.cpp", 5)),
        ]);
        assert_eq!(cause(&d, include, "x"), ("division by zero".to_owned(), place("/t/t.cpp", 3)));
        // A cause in a header: the first frame outside.
        let d = primary(vec![
            in_call(place("/t/t.cpp", 1)),
            note(None, "thrown", place(header, 302)),
            in_call(place(header, 290)),
            in_call(place("/t/t.cpp", 7)),
        ]);
        assert_eq!(cause(&d, include, "x"), ("thrown".to_owned(), place("/t/t.cpp", 7)));
        // No frame outside: the error's own place.
        let d = primary(vec![note(None, "thrown", place(header, 302))]);
        assert_eq!(cause(&d, include, "x"), ("thrown".to_owned(), place("/t/t.cpp", 9)));
        // The error too in a header: the cause's place.
        let mut d = primary(vec![note(None, "thrown", place(header, 302))]);
        d[1].place = place(header, 1);
        assert_eq!(cause(&d, include, "x"), ("thrown".to_owned(), place(header, 302)));
        // No note of free text: the error itself.
        let d = primary(vec![in_call(place("/t/t.cpp", 5))]);
        assert_eq!(
            cause(&d, include, "x"),
            (
                "call to consteval function 'transform' is not a constant expression".to_owned(),
                place("/t/t.cpp", 9)
            )
        );
        // No error: the fallback.
        assert_eq!(cause(&d[..1], include, "x"), ("x".to_owned(), None));
        // An error of Kiln's other than the entry's call (a limit reached
        // in an instantiation at run time) is its own cause, though its
        // notes end with the call stack's.
        let d = vec![diag(
            Some("sema_template_recursion_depth"),
            Severity::Fatal,
            "recursive template instantiation exceeded maximum depth of 512",
            place("/t/t.cpp", 2),
            vec![
                in_call(place("/t/t.cpp", 4)),
                note(None, "in call to 'transform()'", place("/t/t.cpp", 4)),
            ],
        )];
        assert_eq!(
            cause(&d, include, "x"),
            (
                "recursive template instantiation exceeded maximum depth of 512".to_owned(),
                place("/t/t.cpp", 2)
            )
        );
    }

    // The entry is the driver's, and a run that panicked retires the
    // transform: `ForeignEntry` and `Poisoned` cannot come from a run it
    // makes; were they to, each is an internal error that retires.
    #[test]
    fn a_foreign_entry_and_a_poisoned_session_are_internal_errors() {
        let s = FakeSubject::new();
        for failure in [RunFailure::ForeignEntry, RunFailure::Poisoned] {
            let r = classify_(report(Err(failure), ""), calls(&s), false);
            assert!(
                matches!(&r, Classified::WorkerError(m, true) if m.starts_with("internal error:")),
                "{r:?}"
            );
        }
    }

    fn command() -> TransformCommand {
        TransformCommand::new(PathBuf::from(INCLUDE), &[], PathBuf::from("/t/t.cpp")).unwrap()
    }

    // N15, N16, N18 of the review: what `load` makes of Kiln's refusals
    // before a session: Kiln's text, a text when Kiln wrote none, and a
    // panic or a stack trip of Kiln's as the host's failure.
    #[test]
    fn s12_what_load_makes_of_kilns_refusals() {
        let r = Path::new("/r");
        let e = load(&command(), r, Err(CompileError::CommandLine("\nkiln: no\n".to_owned())))
            .unwrap_err();
        assert_eq!(e, LoadError::Compile { rendered: "\nkiln: no\n".to_owned() });
        let e = load(&command(), r, Err(CompileError::CommandLine(String::new()))).unwrap_err();
        assert_eq!(
            e,
            LoadError::Compile {
                rendered: "kiln stopped before compiling the transform unit, and said nothing\n"
                    .to_owned()
            }
        );
        let e = load(&command(), r, Err(CompileError::Panicked("boom".to_owned()))).unwrap_err();
        assert_eq!(e.name(), "host-failure");
        assert_eq!(e.to_string(), "host-failure: Kiln panicked compiling the transform: boom");
        let e = load(&command(), r, Err(CompileError::StackExhausted { rendered: "t".to_owned() }))
            .unwrap_err();
        assert_eq!(e.name(), "host-failure");
        assert!(e.to_string().contains("the host's limit"), "{e}");
    }

    // §2.5 rule 5: a stack below `STACK` is refused before anything is
    // compiled (the file does not exist).
    #[test]
    fn s2_5_rule5_too_little_stack_is_refused() {
        let e = with_transform(Path::new("/r"), &[], Path::new("/nowhere.cpp"), STACK - 1, |l| {
            l.unwrap_err()
        });
        assert_eq!(e.name(), "host-failure");
        assert!(e.to_string().contains("a transform needs 268435456"), "{e}");
    }

    fn unit_file(name: &str, text: &str) -> (PathBuf, PathBuf) {
        let dir = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .join(format!("driver-unit-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("t.cpp");
        std::fs::write(&file, text).unwrap();
        let resource =
            std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../resource"))
                .unwrap();
        (resource, std::fs::canonicalize(file).unwrap())
    }

    // WP-089 "Decided since" item 3, and N6/N8 of the review: after a run
    // that tripped the guard, the transform is never run again; every
    // later run is `Err` without reaching Kiln. Here the report of the
    // tripping run is made by hand; `tests/stack.rs` trips the guard.
    #[test]
    fn decided_3_a_retired_transform_is_not_run_again() {
        let (resource, file) = unit_file(
            "retired",
            "#include <slip/meta.h>\nconsteval void transform() { (void)slip::lookup(\"x\"); }\n",
        );
        let results = std::thread::scope(|t| {
            std::thread::Builder::new()
                .stack_size(STACK + (1 << 20))
                .spawn_scoped(t, || {
                    with_transform(&resource, &[], &file, STACK, |l| {
                        let l: &mut Loaded<'_> = &mut l.unwrap();
                        let s = FakeSubject::new();
                        let before = l
                            .run(&s, Budgets::default(), &Cancel::new())
                            .map(|(o, st)| (o, st.host_calls));
                        let text = "t.cpp:2:1: fatal error: kiln: stack exhausted during constant evaluation\n";
                        let tripped = l.conclude(
                            report(Err(RunFailure::NotConstant), text),
                            calls(&s),
                            true,
                            0,
                        );
                        let retired = l.retired().map(str::to_owned);
                        let after = l.run(&s, Budgets::default(), &Cancel::new());
                        (before, tripped, retired, after)
                    })
                })
                .unwrap()
                .join()
                .unwrap()
        });
        let (before, tripped, retired, after) = results;
        assert!(matches!(before, Ok((Outcome::Completed(_), 1))), "{before:?}");
        assert!(
            matches!(&tripped, Ok((Outcome::Failed(f), _)) if f.kind == FailureKind::HostFailure),
            "{tripped:?}"
        );
        let retired = retired.expect("retired");
        assert!(retired.starts_with("host-failure: Kiln's stack guard tripped"), "{retired}");
        let e = after.expect_err("not run again");
        assert!(e.contains(&retired), "{e}");
        let _ = std::fs::remove_dir_all(file.parent().unwrap());
    }

    // N6 and N8 of the review: a guard that tripped in a run the driver
    // did not classify (here the session is run directly, under a guard
    // deliberately too small) is seen before the next run, which is `Err`,
    // does not reach Kiln, and retires the transform.
    #[test]
    fn a_tripped_guard_is_seen_before_a_run_and_retires() {
        let support = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../kiln/tests/support");
        let support = std::fs::canonicalize(support).unwrap();
        let (resource, file) = unit_file(
            "pre-run",
            "#include <slip/meta.h>\n#include \"kiln_meta.h\"\n\
             template <int K> struct R { static constexpr int v = R<K - 1>::v + 1; };\n\
             template <> struct R<0> { static constexpr int v = 0; };\n\
             consteval void transform() {\n\
               auto r = std::meta::substitute(^^R, {std::meta::reflect_constant(300)});\n\
               (void)std::meta::extract<int>(std::meta::static_data_members_of(r, std::meta::access_context::unchecked())[0]);\n\
             }\n",
        );
        let author = vec!["-I".to_owned(), support.to_str().unwrap().to_owned()];
        let got = std::thread::scope(|t| {
            std::thread::Builder::new()
                .stack_size(STACK + (1 << 20))
                .spawn_scoped(t, || {
                    super::with_transform_on_stack(&resource, &author, &file, 4 << 20, |l| {
                        let l: &mut Loaded<'_> = &mut l.unwrap();
                        let s = FakeSubject::new();
                        let mut c = calls(&s);
                        c.in_run = true;
                        let budget = slip_kiln::session::Budget {
                            steps: 1 << 32,
                            bytes: 256 << 20,
                            depth: 4096,
                        };
                        let direct = l.session.run(l.entry, &mut c, budget, &Cancel::new());
                        let tripped = l.session.stack_exhausted();
                        let unretired = l.retired().is_none();
                        let next = l.run(&s, Budgets::default(), &Cancel::new());
                        (direct.result, tripped, unretired, next, l.retired().map(str::to_owned))
                    })
                })
                .unwrap()
                .join()
                .unwrap()
        });
        let (direct, tripped, unretired, next, retired) = got;
        assert!(direct.is_err(), "{direct:?}");
        assert!(tripped && unretired);
        let e = next.expect_err("refused");
        assert_eq!(
            e,
            "the use of this transform has ended: host-failure: Kiln's stack guard had tripped in \
             the transform session"
        );
        assert!(retired.is_some_and(|r| r.contains("stack guard had tripped")));
        let _ = std::fs::remove_dir_all(file.parent().unwrap());
    }
}
