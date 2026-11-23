//! The run driver (`spec/api.md` §2, §12; work package WP-089): compile a
//! transform once, check its headers and its entry, then run `transform()`
//! against one subject unit at a time and classify what happened.
//!
//! The one place that joins the transform session (`slip_kiln::session`,
//! WP-080), the host ([`Host`], WP-081) and a subject ([`Subject`]).
//! Decisions the specification leaves to this module are in
//! `spec/landed/api/WP-089.md`.
//!
//! **What ends the use of a transform.** Three things leave the session in
//! a state no later run may trust, and the driver then *retires* the
//! [`Loaded`]: every later [`Loaded::run`] is `Err` without reaching Kiln,
//! and [`Loaded::retired`] says why. They are a host error (a Slip bug,
//! §2.8 `host-failure`), Kiln's stack guard tripping (sticky in Kiln, see
//! [`slip_kiln::session::Session::run`]), and a session poisoned by a
//! handler's panic. A worker that sees a retired transform ends.

use std::fmt;
use std::path::Path;
use std::time::Instant;

use slip_kiln::session::{
    self, Budget, Cancel, CompileError, Entry, EntryError, ReadFile, RunFailure, RunReport,
    Session, TransformCommand,
};
use slip_model::wire::Encoder;
use slip_model::{Counter, Digest};

use crate::host::{self, Failure, Host, Limits};
use crate::output::RunOutput;
use crate::resource::{self, HeaderError};
use crate::subject::{self, Subject};
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
    /// Not the transform's fault: Kiln panicked while compiling. A Slip
    /// or Kiln bug.
    Internal(String),
}

impl LoadError {
    /// The name of the error in `spec/api.md` §12 (`internal` for
    /// [`LoadError::Internal`], which §12 does not list).
    pub fn name(&self) -> &'static str {
        match self {
            LoadError::Option(_) => "transform-option",
            LoadError::Compile { .. } => "transform-compile",
            LoadError::Header(_) => HeaderError::NAME,
            LoadError::Entry(name) => name,
            LoadError::Internal(_) => "internal",
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
            LoadError::Internal(m) => write!(f, "internal error: {m}"),
        }
    }
}

impl std::error::Error for LoadError {}

/// The condition of §2.3 rule 2 (or rule 3) that the error `name` names.
fn entry_condition(name: &str) -> &'static str {
    match name {
        "entry-missing" => "no declaration named `transform` in the global namespace",
        "entry-overloaded" => "more than one function named `transform`",
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

/// What a run gave (§2.8).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The run output (§7.6), with the evaluator's counters added.
    Completed(RunOutput),
    /// A failed run: no effect of it is kept (§2.8 rule 1).
    Failed {
        /// The failure's name in §2.8.
        kind: &'static str,
        /// One line for a person: the diagnostic's first line, or, for an
        /// uncaught `slip::error`, the refusal (§2.8 rule 3).
        message: String,
        /// For an uncaught `slip::error`: the place in the subject the
        /// refused value denotes, if it denotes one.
        place: Option<subject::Range>,
        /// The run's diagnostics as Kiln renders them.
        rendered: String,
        /// For an uncaught `slip::error`: the refusal the run's failure
        /// list holds (the most recent; see `spec/landed/api/WP-089.md`).
        refusal: Option<Failure>,
    },
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
/// run against subject units.
pub struct Loaded<'s> {
    session: Session<'s>,
    /// The entry, taken from `session` and never given out: no run can be
    /// given an entry of another session (`RunFailure::ForeignEntry`).
    entry: Entry,
    id: Digest,
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
/// ([`slip_kiln::session::with_transform`]).
pub fn with_transform<R>(
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
        Err(CompileError::Panicked(m)) => {
            return Err(LoadError::Internal(format!("Kiln panicked compiling the transform: {m}")));
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
        &cmd.command_line(),
        session.triple(),
        session.predefines(),
        session.files(),
    );
    Ok(Loaded { session, entry, id, retired: None })
}

/// `TransformId` (`spec/api.md` §2.2): `H("slip.transform.v0", v)` where
/// `v` is the record of
///
/// ```text
/// api_level:     uv
/// command_line:  list<str>    the words after the program name
/// target:        str          the triple
/// predefines:    digest       content digest of the predefines buffer
/// files:         list<(path: bytes, content: digest)>   in the order first read
/// ```
pub fn transform_id(
    api_level: u64,
    command_line: &[String],
    triple: &str,
    predefines: &[u8],
    files: &[ReadFile],
) -> Digest {
    let mut e = Encoder::new();
    e.uv(api_level);
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

    /// Why the use of this transform ended, if it has: a host error, Kiln's
    /// stack guard, or a poisoned session (module documentation). Every
    /// later [`Loaded::run`] is then `Err`.
    pub fn retired(&self) -> Option<&str> {
        self.retired.as_deref()
    }

    /// Runs `transform()` once against `subject` (§2.4 rule 1) with a new
    /// [`Host`] (§2.4 rule 3) and `budgets`, and classifies the result
    /// (§2.8).
    ///
    /// # Errors
    ///
    /// An error of the worker, with no outcome for the subject unit: the
    /// transform was retired by an earlier run ([`Loaded::retired`]), the
    /// session was poisoned (an earlier handler panicked), or the run was
    /// cancelled (§2.5 rule 4: a cancelled run has no result at all).
    ///
    /// A host failure is an outcome (`host-failure`, so that the subject
    /// unit's failure is recorded with its diagnostics) *and* retires the
    /// transform: the worker learns it from [`Loaded::retired`] at once and
    /// from the next run.
    pub fn run(
        &mut self,
        subject: &(dyn Subject + Sync),
        budgets: Budgets,
        cancel: &Cancel,
    ) -> Result<(Outcome, RunStats), String> {
        if let Some(why) = &self.retired {
            return Err(format!("the use of this transform has ended: {why}"));
        }
        if self.session.stack_exhausted() {
            let why = "Kiln's stack guard had tripped in the transform session".to_owned();
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
        let report = self.session.run(self.entry, &mut handler, budget, cancel);
        handler.in_run = false;
        let wall_ns = u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX);
        let stack_exhausted = self.session.stack_exhausted();
        self.conclude(report, handler, stack_exhausted, wall_ns)
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
        match classify(report, handler, stack_exhausted) {
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
    /// The host's own error, if a call returned one (§2.8 `host-failure`).
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
        let r = self.host.call(op, ints, bytes, &mut KilnOut(out));
        if let Err(e) = &r
            && self.host_error.is_none()
        {
            self.host_error = Some(e.clone());
        }
        r
    }
}

/// `slip_kiln::session::Out` as the host's [`host::Out`].
struct KilnOut<'o>(&'o mut dyn session::Out);

impl host::Out for KilnOut<'_> {
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

/// The first line of Kiln's rendering, or `fallback`.
fn first_line(rendered: &str, fallback: &str) -> String {
    rendered.lines().find(|l| !l.trim().is_empty()).unwrap_or(fallback).to_owned()
}

fn failed(kind: &'static str, message: String, rendered: String) -> Outcome {
    Outcome::Failed { kind, message, place: None, rendered, refusal: None }
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

/// §2.8: what a run's report means, given whether Kiln's stack guard is
/// tripped after it and what the handler saw.
fn classify(report: RunReport, handler: Calls<'_>, stack_exhausted: bool) -> Classified {
    let RunReport { result, rendered, .. } = report;
    // Kiln's guard is sticky: whatever the run's own failure looks like,
    // the session is not to be run again. Not the transform's fault
    // (§3.5 rule 4): at this pin it is reached by a run away from the
    // compiling thread (K2), and a stack's size is not a budget (§2.5
    // rule 2).
    if stack_exhausted {
        let message = format!(
            "Kiln's stack guard tripped during the run; the transform session is not used again: {}",
            first_line(&rendered, "stack exhausted")
        );
        let why = format!("host-failure: {message}");
        return Classified::Retire(failed("host-failure", message, rendered), why);
    }
    match result {
        Ok(()) => {
            let Calls { host, .. } = handler;
            Classified::Outcome(Outcome::Completed(host.finish()))
        }
        // The host's own error: a Slip bug (WP-089 rule 5).
        Err(RunFailure::Host(_)) if handler.host_error.is_some() => {
            let error = handler.host_error.unwrap_or_default();
            let message = format!("the host could not continue: {error}");
            let why = format!("host-failure: {message}");
            Classified::Retire(failed("host-failure", message, rendered), why)
        }
        // Kiln's own check of a call's arguments (a transform calling the
        // primitive with buffers that do not fit), or a call with no
        // handler: the evaluation did something that is not a constant
        // expression, never a host failure (§3.5 rules 4 and 5).
        Err(RunFailure::Host(note)) => {
            Classified::Outcome(failed("not-constant", first_line(&rendered, &note), rendered))
        }
        Err(RunFailure::NotConstant) => Classified::Outcome(failed(
            "not-constant",
            first_line(&rendered, "the evaluation is not a constant expression"),
            rendered,
        )),
        Err(RunFailure::StepBudget) => Classified::Outcome(failed(
            "step-budget",
            first_line(&rendered, "the step budget is exhausted"),
            rendered,
        )),
        Err(RunFailure::MemoryBudget) => Classified::Outcome(failed(
            "memory-budget",
            first_line(&rendered, "the memory budget is exhausted"),
            rendered,
        )),
        Err(RunFailure::DepthLimit) => Classified::Outcome(failed(
            "depth-limit",
            first_line(&rendered, "the depth limit is reached"),
            rendered,
        )),
        // Kiln names the type as written, unqualified (`error`), so a
        // class of the transform's own named `error` has the same name.
        // The refusal is reported only when the run has one (decision 4 of
        // the landed file).
        Err(RunFailure::Uncaught { type_name }) => {
            match handler.host.last_failure().filter(|_| type_name == ERROR_TYPE) {
                Some(f) => Classified::Outcome(Outcome::Failed {
                    kind: "uncaught",
                    message: format!(
                        "an uncaught slip::error; the most recent refusal was: {} ({})",
                        f.message,
                        status_name(f.errc)
                    ),
                    place: f.place,
                    rendered,
                    refusal: Some(f.clone()),
                }),
                None => Classified::Outcome(failed(
                    "uncaught",
                    first_line(&rendered, &format!("an uncaught exception of type '{type_name}'")),
                    rendered,
                )),
            }
        }
        // §2.5 rule 4: no result at all. The session is usable once the
        // token is reset.
        Err(RunFailure::Cancelled) => Classified::WorkerError(
            "the run was cancelled: a cancelled run has no result (spec/api.md §2.5 rule 4)"
                .to_owned(),
            false,
        ),
        // WP-089 "Decided since" item 4.
        Err(RunFailure::Poisoned) => Classified::WorkerError(
            "internal error: the transform session is unusable after a panic of an earlier \
             run's host"
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
    use std::path::Path;

    use slip_kiln::session::{Cancel, Handler, Out, RunFailure, RunReport};
    use slip_model::Digest;

    use super::{
        Budgets, Calls, Classified, Loaded, OUTSIDE_A_RUN, Outcome, classify, with_transform,
    };
    use crate::fake::FakeSubject;
    use crate::host::{Host, Limits};
    use crate::wire;

    struct NoRoom;

    impl Out for NoRoom {
        fn int_capacity(&self) -> usize {
            0
        }
        fn byte_capacity(&self) -> usize {
            0
        }
        fn write_ints(&mut self, _: &[u64]) {}
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
        RunReport { result, steps: 5, host_calls: 1, peak_bytes: 0, rendered: rendered.to_owned() }
    }

    // WP-089 rule 6: the handler refuses a call while the driver's flag is
    // clear, without reaching the host; with the flag set the call reaches
    // it (an operation the host does not have: a failure of the run).
    #[test]
    fn rule6_the_handler_refuses_a_call_outside_a_run() {
        let s = FakeSubject::new();
        let mut c = calls(&s);
        assert_eq!(c.call(9999, &[], &[], &mut NoRoom), Err(OUTSIDE_A_RUN.to_owned()));
        assert_eq!(c.host.failure_count(), 0, "the host was not reached");
        c.in_run = true;
        let word = c.call(9999, &[], &[], &mut NoRoom).unwrap();
        assert_eq!(wire::status(word), wire::STATUS_UNKNOWN_OP);
        assert_eq!(c.host.failure_count(), 1);
        assert_eq!(c.host_error, None, "a refusal is not a host error");
    }

    // WP-089 "Decided since" item 3: when Kiln's stack guard has tripped,
    // whatever the run's own failure, it is `host-failure` with Kiln's text
    // and the transform is retired; without the flag the same report is
    // `not-constant`.
    #[test]
    fn decided_3_stack_exhausted_is_host_failure_and_retires() {
        let s = FakeSubject::new();
        let text = "t.cpp:1:1: fatal error: kiln: stack exhausted during template instantiation; reduce nesting or raise -fkiln-stack-size\n";
        let r = classify(report(Err(RunFailure::NotConstant), text), calls(&s), true);
        let Classified::Retire(Outcome::Failed { kind, message, rendered, .. }, why) = r else {
            panic!("{r:?}")
        };
        assert_eq!(kind, "host-failure");
        assert!(
            message.contains("kiln: stack exhausted during template instantiation"),
            "{message}"
        );
        assert_eq!(rendered, text);
        assert!(why.starts_with("host-failure: "), "{why}");
        // A run that completed while the flag tripped is not trusted either.
        let r = classify(report(Ok(()), ""), calls(&s), true);
        assert!(
            matches!(r, Classified::Retire(Outcome::Failed { kind: "host-failure", .. }, _)),
            "{r:?}"
        );
        let r = classify(report(Err(RunFailure::NotConstant), text), calls(&s), false);
        assert!(
            matches!(r, Classified::Outcome(Outcome::Failed { kind: "not-constant", .. })),
            "{r:?}"
        );
    }

    // The entry is the driver's: `ForeignEntry` cannot come from a run it
    // makes; were it to, it is an internal error that retires.
    #[test]
    fn a_foreign_entry_is_an_internal_error() {
        let s = FakeSubject::new();
        let r = classify(report(Err(RunFailure::ForeignEntry), ""), calls(&s), false);
        assert!(
            matches!(&r, Classified::WorkerError(m, true) if m.starts_with("internal error:")),
            "{r:?}"
        );
    }

    // WP-089 "Decided since" item 3: after a run that tripped the guard,
    // the transform is never run again: every later run is `Err` without
    // reaching Kiln.
    #[test]
    fn decided_3_a_retired_transform_is_not_run_again() {
        let dir = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .join(format!("driver-unit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("t.cpp");
        std::fs::write(
            &file,
            "#include <slip/meta.h>\nconsteval void transform() { (void)slip::lookup(\"x\"); }\n",
        )
        .unwrap();
        let resource =
            std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../resource"))
                .unwrap();
        let file = std::fs::canonicalize(file).unwrap();
        let results = std::thread::scope(|t| {
            std::thread::Builder::new()
                .stack_size(64 << 20)
                .spawn_scoped(t, || {
                    with_transform(&resource, &[], &file, 63 << 20, |l| {
                        let l: &mut Loaded<'_> = &mut l.unwrap();
                        let s = FakeSubject::new();
                        let before = l.run(&s, Budgets::default(), &Cancel::new()).map(|(o, st)| (o, st.host_calls));
                        let text = "t.cpp:2:1: fatal error: kiln: stack exhausted during constant evaluation\n";
                        let tripped = l.conclude(report(Err(RunFailure::NotConstant), text), calls(&s), true, 0);
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
            matches!(tripped, Ok((Outcome::Failed { kind: "host-failure", .. }, _))),
            "{tripped:?}"
        );
        let retired = retired.expect("retired");
        assert!(retired.contains("stack exhausted during constant evaluation"), "{retired}");
        let e = after.expect_err("not run again");
        assert!(e.contains(&retired), "{e}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
