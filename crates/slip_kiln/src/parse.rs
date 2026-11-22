//! The parse task: one observer through Kiln's front end, to its shard
//! (`spec/model.md` §8, `spec/exec.md` §5.1; the rules beyond the spec are
//! `spec/landed/model/WP-010.md`). Work package WP-010.

use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use kiln_driver::{Io, SharedWriter};
use slip_model::hash::content_digest;
use slip_model::{Counter, FileRef, Observer, Outcome, Phase, Shard, Status};

use crate::check::{DEP_TARGET, front_end_line, overridden, program, utf8_command};
use crate::depfile;
use crate::diags;
use slip_model::formats::clip_message;

/// How the parse task runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseOptions {
    /// A directory for temporary files, unique to the caller. A relative
    /// path is taken against the current directory at the time of the call.
    pub scratch: PathBuf,
}

/// Parses `observer`'s command with Kiln's front end in this process and
/// returns its shard for `phase` and `round` (`spec/model.md` §8).
///
/// The command runs as `argv[0]` (`clang-scan-deps…` as `kiln`) on
/// `-fsyntax-only -MD -MF <scratch>/… -MT slip.d`, the command's own
/// arguments, then `-fdiagnostics-format=json`, in the command's
/// `directory`, with standard output discarded and standard error counted
/// as it streams. A command whose own arguments override any of these is
/// `rejected`, as `KilnCheck` refuses it.
///
/// **This changes the process's current directory** to the command's
/// `directory` and does not restore it: Kiln has no `-working-directory`.
/// That is correct in a worker, which runs one job at a time; anything else
/// in the process that uses relative paths, on any thread, is affected.
/// Callers that run several in one process (tests) must serialise them.
///
/// # Errors
///
/// Only a failure of the worker itself: the scratch directory cannot be
/// written, the directory cannot be entered, a file the front end read can
/// no longer be read, or the front end panicked. Diagnostics about the
/// subject are a shard with status `diagnosed` or `rejected`.
pub fn parse_observer(
    observer: &Observer,
    phase: Phase,
    round: u64,
    options: &ParseOptions,
) -> Result<Shard, String> {
    let shard = |outcome, inputs: Option<Vec<FileRef>>| {
        let counters = match &inputs {
            Some(files) => vec![
                Counter { name: "inputs.bytes".into(), value: files.iter().map(|f| f.size).sum() },
                Counter { name: "inputs.files".into(), value: files.len() as u64 },
            ],
            None => Vec::new(),
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
        }
    };

    let (argv0, rest) = match utf8_command(&observer.command) {
        Ok(words) => words,
        Err(arg) => {
            return Ok(shard(rejected(&format!("argument `{arg}` is not UTF-8")), None));
        }
    };

    let scratch = std::path::absolute(&options.scratch)
        .map_err(|e| format!("scratch directory {}: {e}", options.scratch.display()))?;
    let dep_path = scratch.join(unique_name());
    std::fs::File::create(&dep_path).map_err(|e| {
        format!("cannot write in the scratch directory: {}: {e}", dep_path.display())
    })?;
    let dep_guard = RemoveOnDrop(dep_path.clone());
    let Some(dep_arg) = dep_path.to_str().map(str::to_owned) else {
        return Err(format!("scratch path {} is not UTF-8", dep_path.display()));
    };

    let directory = bytes_path(&observer.command.directory)?;
    std::env::set_current_dir(&directory)
        .map_err(|e| format!("cannot enter {}: {e}", directory.display()))?;

    let line = front_end_line(&rest, &dep_arg);
    let program = program(&argv0);
    let args = kiln_driver::args::Args::parse_as(&program, &line);
    if let Some(argument) = overridden(&args, &rest, &dep_arg) {
        return Ok(shard(
            rejected(&format!("argument `{argument}` overrides an option the parse relies on")),
            None,
        ));
    }
    let counter = diags::Counter::default();
    let status = run_front_end(program, line, counter.clone())?;
    let tally = counter.finish();

    let outcome = if tally.errors == 0 && status == 0 {
        Outcome { status: Status::Ok, errors: 0, warnings: tally.warnings, message: String::new() }
    } else {
        // A failure exit with no error diagnostic counts as one error:
        // never `ok` for a command Kiln failed.
        let errors = tally.errors.max(1);
        let message = tally.first_error.clone().unwrap_or_else(|| {
            tally.first_other.clone().unwrap_or_else(|| {
                format!("the front end exited with status {status} and reported no error")
            })
        });
        let status = if tally.first_error.is_some() && tally.first_error_is_driver {
            Status::Rejected
        } else {
            Status::Diagnosed
        };
        Outcome { status, errors, warnings: tally.warnings, message }
    };

    let inputs = if outcome.status == Status::Rejected {
        None
    } else {
        let text = std::fs::read_to_string(&dep_path)
            .map_err(|e| format!("cannot read the dependency file {dep_arg}: {e}"))?;
        if text.is_empty() {
            // Kiln writes no rule after an `#include` it did not find.
            if outcome.status == Status::Ok {
                return Err("the front end succeeded but wrote no dependency file".into());
            }
            None
        } else {
            let names = depfile::prerequisites(&text, DEP_TARGET)?;
            Some(file_refs(&directory, &names)?)
        }
    };
    drop(dep_guard);
    Ok(shard(outcome, inputs))
}

fn rejected(message: &str) -> Outcome {
    Outcome {
        status: Status::Rejected,
        errors: 1,
        warnings: 0,
        message: clip_message(message).to_owned(),
    }
}

/// Runs `kiln_driver::run_as` on a thread of its own with the stack size
/// the command asks for (`-fkiln-stack-size=`, else Kiln's default), with
/// Kiln's stack guard installed on it, as the `kiln` executable does.
/// Returns the exit status; a panic is an `Err`.
fn run_front_end(argv0: String, line: Vec<String>, counter: diags::Counter) -> Result<i32, String> {
    let size = kiln_driver::templ_opts::stack_size_of(&line);
    let handle = std::thread::Builder::new()
        .name("kiln".into())
        .stack_size(size)
        .spawn(move || {
            // As the `kiln` executable's compile thread does: deep recursion
            // becomes Kiln's fatal diagnostic instead of a stack overflow
            // that aborts the worker.
            kiln_sema::stack_guard::StackGuard::install(size);
            let io = Io {
                stdout: Box::new(io::sink()),
                stderr: SharedWriter::new(counter),
                stdin: Box::new(io::empty()) as Box<dyn Read>,
            };
            kiln_driver::run_as(&argv0, &line, io)
        })
        .map_err(|e| {
            format!("cannot start the front end's thread with a {size}-byte stack: {e}")
        })?;
    handle.join().map_err(|payload| {
        let text = payload
            .downcast_ref::<&str>()
            .map(|s| (*s).to_owned())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "a panic".into());
        format!("the front end panicked: {text}")
    })
}

/// Each listed file once, by real path, sorted, with its size and digest.
fn file_refs(directory: &Path, names: &[String]) -> Result<Vec<FileRef>, String> {
    let mut paths: Vec<(Vec<u8>, PathBuf)> = Vec::with_capacity(names.len());
    for name in names {
        let spelled = directory.join(name);
        let real = std::fs::canonicalize(&spelled)
            .map_err(|e| format!("cannot resolve {}: {e}", spelled.display()))?;
        paths.push((path_bytes(&real), real));
    }
    paths.sort();
    paths.dedup();
    let mut refs: Vec<FileRef> = Vec::with_capacity(paths.len());
    for (key, path) in paths {
        let bytes =
            std::fs::read(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        refs.push(FileRef { path: key, size: bytes.len() as u64, content: content_digest(&bytes) });
    }
    Ok(refs)
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
        .map_err(|_| format!("directory {} is not UTF-8", String::from_utf8_lossy(b)))
}

/// A file name unique within this process and among processes.
fn unique_name() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    format!("slip-parse-{}-{}.d", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed))
}

struct RemoveOnDrop(PathBuf);

impl Drop for RemoveOnDrop {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
