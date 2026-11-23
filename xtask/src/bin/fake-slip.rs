//! TEST FIXTURE, not part of Slip: a stand-in for the `slip` executable,
//! for the tests of the transform runner (`xtask/tests/transform.rs`,
//! `spec/harness.md` §6), which give it as `--slip` through
//! `CARGO_BIN_EXE_fake-slip`. It does what a script says, per invocation,
//! so that each rule of the runner is shown without Slip.
//!
//! The script is `fake.txt` in the current directory (the runner runs
//! every `slip` in the test's tree, so the script is a file of the test's
//! `pre/`). It is a list of sections; blank lines and lines starting with
//! `#` are skipped:
//!
//! ```text
//! == <command> <n>|* [<text>]   a section: the invocation whose first argument is <command>,
//!                               the n-th of that command in this store (0 first; * any), and
//!                               whose arguments joined by spaces contain <text> if given
//! out <text>                    a line on standard output
//! err <text>                    a line on standard error
//! write <path> <text>           writes the file (relative to the current directory)
//! remove <path>                 removes the file
//! publish edits|result <file>   publishes the container into the store (an EDTS or RSLT)
//! exit <n>                      ends with status <n> (default 0)
//! kill                          dies by SIGKILL
//! ```
//!
//! In `<text>`, `\n`, `\t` and `\\` are escapes, `@CWD@` is the current
//! directory and `@STORE@` the `--store` value. The first section that
//! matches is run, its actions in order. Every invocation opens the store
//! (`--store`) exclusively, as a command that runs a phase does, and
//! appends its arguments, joined by spaces, as one line to
//! `<store>/fake-slip.log`, which counts the invocations. No section: a
//! message and status 99; no `--store`: status 97; a bad script: 98.

#![forbid(unsafe_code)]

use slip_exec::store::{Access, Object, Store};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> =
        std::env::args_os().skip(1).map(|a| a.to_string_lossy().into_owned()).collect();
    match run(&args) {
        Ok(code) => code,
        Err((code, message)) => {
            let _ = writeln!(std::io::stderr(), "fake-slip: {message}");
            ExitCode::from(code)
        }
    }
}

fn unescape(text: &str, cwd: &str, store: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some(o) => out.push(o),
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out.replace("@CWD@", cwd).replace("@STORE@", store)
}

fn run(args: &[String]) -> Result<ExitCode, (u8, String)> {
    let command = args.first().cloned().unwrap_or_default();
    let store_dir = args
        .iter()
        .position(|a| a == "--store")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
        .ok_or((97, "no --store".to_owned()))?;
    let store = Store::open(&store_dir, Access::Exclusive).map_err(|e| (97, e.to_string()))?;
    let log = store.root().join("fake-slip.log");
    let before = std::fs::read_to_string(&log).unwrap_or_default();
    let n = before.lines().filter(|l| l.split(' ').next() == Some(command.as_str())).count();
    let joined = args.join(" ");
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log)
        .map_err(|e| (97, e.to_string()))?;
    writeln!(f, "{joined}").map_err(|e| (97, e.to_string()))?;
    drop(f);

    let script = std::fs::read_to_string("fake.txt").map_err(|e| (98, format!("fake.txt: {e}")))?;
    let cwd = std::env::current_dir().map_err(|e| (98, e.to_string()))?;
    let cwd = cwd.to_string_lossy().into_owned();
    let store_text = store_dir.to_string_lossy().into_owned();

    let mut actions: Option<Vec<&str>> = None;
    let mut current: Option<bool> = None;
    for line in script.lines() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(header) = line.strip_prefix("== ") {
            if actions.is_some() {
                break;
            }
            let mut parts = header.splitn(3, ' ');
            let cmd = parts.next().unwrap_or_default();
            let index = parts.next().unwrap_or_default();
            let text = parts.next();
            let index_ok = index == "*" || index.parse::<usize>().is_ok_and(|i| i == n);
            let matches = cmd == command && index_ok && text.is_none_or(|t| joined.contains(t));
            current = Some(matches);
            if matches {
                actions = Some(Vec::new());
            }
            continue;
        }
        match (current, actions.as_mut()) {
            (None, _) => {
                return Err((98, format!("fake.txt: an action before any section: {line}")));
            }
            (Some(true), Some(a)) => a.push(line),
            _ => {}
        }
    }
    let Some(actions) = actions else {
        return Err((99, format!("no section of fake.txt for `{joined}` (invocation {n})")));
    };
    let mut status = 0u8;
    for action in actions {
        let (verb, rest) = action.split_once(' ').unwrap_or((action, ""));
        match verb {
            "out" => println!("{}", unescape(rest, &cwd, &store_text)),
            "err" => eprintln!("{}", unescape(rest, &cwd, &store_text)),
            "write" => {
                let (path, text) = rest.split_once(' ').unwrap_or((rest, ""));
                std::fs::write(path, unescape(text, &cwd, &store_text))
                    .map_err(|e| (98, format!("write {path}: {e}")))?;
            }
            "remove" => {
                std::fs::remove_file(rest).map_err(|e| (98, format!("remove {rest}: {e}")))?
            }
            "publish" => {
                let (kind, file) = rest.split_once(' ').unwrap_or((rest, ""));
                let object = match kind {
                    "edits" => Object::EditSet,
                    "result" => Object::Result,
                    _ => return Err((98, format!("publish: unknown kind `{kind}`"))),
                };
                let bytes =
                    std::fs::read(Path::new(file)).map_err(|e| (98, format!("{file}: {e}")))?;
                store
                    .publish_object(object, &bytes)
                    .map_err(|e| (98, format!("publish {file}: {e}")))?;
            }
            "exit" => status = rest.parse().map_err(|_| (98, format!("exit: `{rest}`")))?,
            "kill" => {
                let _ = std::io::stdout().flush();
                let pid = std::process::id().to_string();
                let _ = std::process::Command::new("kill").args(["-KILL", &pid]).status();
                std::thread::sleep(std::time::Duration::from_secs(10));
            }
            _ => return Err((98, format!("fake.txt: unknown action `{verb}`"))),
        }
    }
    let _ = std::io::stdout().flush();
    drop(store);
    Ok(ExitCode::from(status))
}
