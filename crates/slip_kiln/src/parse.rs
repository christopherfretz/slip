//! The parse task: one observer through Kiln's front-end library entry
//! (`kiln_driver::front_end::run_front_end`) to its shard
//! (`spec/model.md` §8, §8.4, `spec/exec.md` §5.1). Work packages WP-010,
//! WP-025.

use std::path::{Path, PathBuf};

use kiln_driver::front_end::{FrontEnd, FrontEndError, FrontEndOptions, run_front_end};
use slip_model::formats::clip_message;
use slip_model::hash::content_digest;
use slip_model::{Counter, Digest, FileRef, Observer, Outcome, Phase, Shard, Status};

use crate::check::{front_end_line, program, utf8_command};
use crate::kiln::ast::Ast;
use crate::kiln::oven::Oven;
use crate::kiln::source::{FileId, FileOrigin, SourceManager};

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
    main_file: Option<FileId>,
    /// Each buffer that is one of `inputs`, and the index of its entry.
    buffers: &'r [(FileId, usize)],
    inputs: &'r [FileRef],
    errors: u32,
}

impl<'r> Unit<'r> {
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

    /// Errors reported so far by the job's engine (when the closure is
    /// called: the parse's own, without the driver's).
    pub fn errors(&self) -> u32 {
        self.errors
    }
}

impl std::fmt::Debug for Unit<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Unit")
            .field("main_file", &self.main_file)
            .field("inputs", &self.inputs.len())
            .field("errors", &self.errors)
            .finish_non_exhaustive()
    }
}

/// Parses `observer`'s command with Kiln's front end in this process and
/// returns its shard for `phase` and `round` (`spec/model.md` §8.4).
///
/// `parse_observer_with` with a closure that does nothing.
///
/// # Errors
///
/// As [`parse_observer_with`].
pub fn parse_observer(observer: &Observer, phase: Phase, round: u64) -> Result<Shard, String> {
    parse_observer_with(observer, phase, round, |_| ()).map(|(shard, _)| shard)
}

/// Parses `observer`'s command with Kiln's front end in this process;
/// when a parse happened, calls `f` on the finished unit (with errors or
/// without) before anything is dropped. Returns the shard for `phase` and
/// `round` and `f`'s result (`None`: no parse happened).
///
/// The front end runs `argv[0] -fsyntax-only argv[1..]` (a scanner name
/// `clang-scan-deps…` as `kiln`) through
/// `kiln_driver::front_end::run_front_end`, on a thread of its own, with
/// nothing rendered and nothing written. The inputs are the source
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
    f: impl FnOnce(&Unit<'_>) -> R + Send,
) -> Result<(Shard, Option<R>), String> {
    let shard = |outcome, parsed: Option<(Vec<FileRef>, u64)>| {
        let (inputs, counters) = match parsed {
            Some((files, tokens)) => {
                let counters = vec![
                    Counter {
                        name: "inputs.bytes".into(),
                        value: files.iter().map(|f| f.size).sum(),
                    },
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
        }
    };

    let (argv0, rest) = match utf8_command(&observer.command) {
        Ok(words) => words,
        Err(arg) => {
            return Ok((shard(rejected(&format!("argument `{arg}` is not UTF-8")), None), None));
        }
    };

    let directory = bytes_path(&observer.command.directory)?;
    std::env::set_current_dir(&directory)
        .map_err(|e| format!("cannot enter {}: {e}", directory.display()))?;

    let line = front_end_line(&rest);
    let program = program(&argv0);
    let dir: &Path = &directory;
    let see = move |fe: &FrontEnd<'_, '_>| -> Result<(Vec<FileRef>, u64, R), String> {
        let (inputs, buffers) = collect_inputs(fe, dir)?;
        let unit = Unit {
            ast: fe.ast,
            sm: fe.sm,
            oven: fe.oven,
            main_file: fe.main_file,
            buffers: &buffers,
            inputs: &inputs,
            errors: fe.diags.error_count(),
        };
        let value = f(&unit);
        Ok((inputs, fe.tokens, value))
    };
    let report = match run_front_end(&program, &line, FrontEndOptions::default(), see) {
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
