//! The parse task of WP-010, as small as it can be made: Kiln's driver
//! (`kiln_driver::run_as`) as a black box on
//! `argv[0] -fsyntax-only -MD -MF <dep file> -MT slip.d argv[1..]
//! -fdiagnostics-format=json`, the diagnostics read from the JSON on
//! standard error, the inputs from the dependency file, each input read a
//! second time for its size and digest. This is what `spec/model.md` §8.4
//! said before WP-025; the parity test compares the new shard with it.
//!
//! Left out, since no fixture needs them: the program name of a scanner
//! (`clang-scan-deps…`), the check that the command does not override the
//! parse's own options, the 64 KiB line cap.

use std::io;
use std::path::{Path, PathBuf};

use kiln_diag::SharedBuffer;
use kiln_driver::{Io, SharedWriter};
use slip_model::formats::clip_message;
use slip_model::hash::content_digest;
use slip_model::{Counter, FileRef, Observer, Outcome, Phase, Shard, Status};

use super::depfile;

/// The shard the parse task of WP-010 gave `observer`. The dependency
/// file is made in `scratch` and removed.
pub fn old_shard(
    observer: &Observer,
    phase: Phase,
    round: u64,
    scratch: &Path,
) -> Result<Shard, String> {
    let words: Vec<String> = observer
        .command
        .argv
        .iter()
        .map(|a| String::from_utf8(a.clone()).map_err(|_| "not UTF-8".to_owned()))
        .collect::<Result<_, _>>()?;
    let (argv0, rest) = words.split_first().ok_or("no argv[0]")?;
    let directory = PathBuf::from(String::from_utf8(observer.command.directory.clone()).unwrap());
    let dep = scratch.join(format!("oracle-{}.d", std::process::id()));
    std::fs::write(&dep, b"").map_err(|e| e.to_string())?;
    std::env::set_current_dir(&directory).map_err(|e| format!("cannot enter: {e}"))?;

    let dep_arg = dep.to_str().unwrap().to_owned();
    let mut line: Vec<String> =
        ["-fsyntax-only", "-MD", "-MF", &dep_arg, "-MT", "slip.d"].map(str::to_owned).to_vec();
    line.extend(rest.iter().cloned());
    line.push("-fdiagnostics-format=json".to_owned());

    let stderr = SharedBuffer::new();
    let status = run(argv0.clone(), line, stderr.clone())?;
    let tally = tally(&stderr.contents());

    let outcome = if tally.errors == 0 && status == 0 {
        Outcome { status: Status::Ok, errors: 0, warnings: tally.warnings, message: String::new() }
    } else {
        let message =
            tally.first_error.clone().or(tally.first_other.clone()).unwrap_or_else(|| {
                format!("the front end exited with status {status} and reported no error")
            });
        let status = if tally.first_error.is_some() && tally.first_error_is_driver {
            Status::Rejected
        } else {
            Status::Diagnosed
        };
        Outcome { status, errors: tally.errors.max(1), warnings: tally.warnings, message }
    };

    let text = std::fs::read_to_string(&dep).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&dep);
    let inputs = if outcome.status == Status::Rejected || text.is_empty() {
        if outcome.status == Status::Ok {
            return Err("the front end succeeded but wrote no dependency file".into());
        }
        None
    } else {
        Some(file_refs(&directory, &depfile::prerequisites(&text, "slip.d")?)?)
    };
    let counters = match &inputs {
        Some(files) => vec![
            Counter { name: "inputs.bytes".into(), value: files.iter().map(|f| f.size).sum() },
            Counter { name: "inputs.files".into(), value: files.len() as u64 },
        ],
        None => Vec::new(),
    };
    Ok(Shard {
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
    })
}

/// `run_as` on a thread with the command's stack and Kiln's stack guard.
fn run(argv0: String, line: Vec<String>, stderr: SharedBuffer) -> Result<i32, String> {
    let size = kiln_driver::templ_opts::stack_size_of(&line);
    std::thread::Builder::new()
        .stack_size(size)
        .spawn(move || {
            kiln_sema::stack_guard::StackGuard::install(size);
            let io = Io {
                stdout: Box::new(io::sink()),
                stderr: SharedWriter::new(stderr),
                stdin: Box::new(io::empty()),
            };
            kiln_driver::run_as(&argv0, &line, io)
        })
        .map_err(|e| e.to_string())?
        .join()
        .map_err(|_| "the front end panicked".to_owned())
}

/// Each listed file once, by real path, sorted, read for size and digest.
fn file_refs(directory: &Path, names: &[String]) -> Result<Vec<FileRef>, String> {
    let mut paths: Vec<PathBuf> = names
        .iter()
        .map(|n| std::fs::canonicalize(directory.join(n)).map_err(|e| format!("{n}: {e}")))
        .collect::<Result<_, _>>()?;
    paths.sort_by(|a, b| a.as_os_str().as_encoded_bytes().cmp(b.as_os_str().as_encoded_bytes()));
    paths.dedup();
    paths
        .iter()
        .map(|p| {
            let bytes = std::fs::read(p).map_err(|e| format!("{}: {e}", p.display()))?;
            Ok(FileRef {
                path: p.as_os_str().as_encoded_bytes().to_vec(),
                size: bytes.len() as u64,
                content: content_digest(&bytes),
            })
        })
        .collect()
}

/// What the diagnostics said.
#[derive(Debug, Default)]
struct Tally {
    errors: u64,
    warnings: u64,
    first_error: Option<String>,
    first_error_is_driver: bool,
    first_other: Option<String>,
}

/// Counts the JSON lines of Kiln's standard error. Kiln writes the fields
/// `id`, `severity`, `file`, `line`, `col`, `message` first, in that order
/// (`kiln_diag::json`), so they are read in that order and the rest of the
/// line is ignored.
fn tally(text: &str) -> Tally {
    let mut t = Tally::default();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let Some(d) = Fields::read(line) else {
            t.first_other.get_or_insert_with(|| clip_message(line).to_owned());
            continue;
        };
        match d.severity.as_str() {
            "error" | "fatal" => {
                t.errors += 1;
                if t.first_error.is_none() {
                    t.first_error_is_driver =
                        d.id.as_deref().is_some_and(|i| i.starts_with("drv_"));
                    let located = match (d.file, d.line, d.col) {
                        (Some(file), Some(line), Some(col)) => {
                            format!("{file}:{line}:{col}: {}", d.message)
                        }
                        _ => d.message,
                    };
                    t.first_error = Some(clip_message(&located).to_owned());
                }
            }
            "warning" => t.warnings += 1,
            _ => {}
        }
    }
    t
}

struct Fields {
    id: Option<String>,
    severity: String,
    file: Option<String>,
    line: Option<String>,
    col: Option<String>,
    message: String,
}

impl Fields {
    fn read(line: &str) -> Option<Fields> {
        let mut s = line.strip_prefix("{")?;
        let mut field = |name: &str| -> Option<Option<String>> {
            s = s.strip_prefix(',').unwrap_or(s).strip_prefix(&format!("\"{name}\":"))?;
            if let Some(r) = s.strip_prefix("null") {
                s = r;
                return Some(None);
            }
            if s.starts_with('"') {
                let (v, r) = json_string(s)?;
                s = r;
                return Some(Some(v));
            }
            let end = s.find(|c: char| !c.is_ascii_digit())?;
            let (v, r) = s.split_at(end);
            s = r;
            Some(Some(v.to_owned()))
        };
        let id = field("id")?;
        let severity = field("severity")??;
        let file = field("file")?;
        let line = field("line")?;
        let col = field("col")?;
        let message = field("message")??;
        Some(Fields { id, severity, file, line, col, message })
    }
}

/// A JSON string at the start of `s` as `kiln_diag::escape_json` writes
/// it, and the rest of `s`.
fn json_string(s: &str) -> Option<(String, &str)> {
    let mut out = String::new();
    let mut chars = s.strip_prefix('"')?.char_indices();
    let body = &s[1..];
    while let Some((i, c)) = chars.next() {
        match c {
            '"' => return Some((out, &body[i + 1..])),
            '\\' => {
                let (_, e) = chars.next()?;
                out.push(match e {
                    'n' => '\n',
                    'r' => '\r',
                    't' => '\t',
                    'b' => '\u{8}',
                    'f' => '\u{c}',
                    'u' => {
                        let hex: String =
                            (0..4).filter_map(|_| chars.next().map(|(_, h)| h)).collect();
                        char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?
                    }
                    other => other,
                });
            }
            c => out.push(c),
        }
    }
    None
}
