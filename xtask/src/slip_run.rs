//! Running the `slip` executable from xtask (`spec/harness.md` §9, shared
//! with §10): finding it, running it with its standard output read,
//! picking keyed lines out of that output, and removing a tree xtask made
//! under a corpus directory.
//!
//! xtask reaches `slip` only as an executable: it never links the crates
//! that link Kiln.

use std::ffi::{OsStr, OsString};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};

/// What [`find_slip`] tells the user to run when there is no executable.
pub const BUILD: &str = "cargo build --release -p slip_cli";

/// The `slip` executable: `given` (`--slip`) if there is one, else
/// `release/slip` under `target_dir` (the value of `$CARGO_TARGET_DIR`,
/// when set and not empty) or under `target` in the repository `repo`.
///
/// The path must name a regular file (symbolic links followed) with an
/// execute bit; otherwise the error says so, and for the default path
/// that it must be built with [`BUILD`] (xtask does not build it). The
/// result is absolute (canonical), so that it does not depend on the
/// directory `slip` later runs in.
pub fn find_slip(
    repo: &Path,
    given: Option<&Path>,
    target_dir: Option<&OsStr>,
) -> Result<PathBuf, String> {
    use std::os::unix::fs::PermissionsExt;
    let (path, hint) = if let Some(p) = given {
        (p.to_path_buf(), String::new())
    } else {
        let target =
            target_dir.filter(|t| !t.is_empty()).map_or_else(|| repo.join("target"), PathBuf::from);
        (target.join("release").join("slip"), format!(": run `{BUILD}`"))
    };
    let shown = path.display();
    match std::fs::metadata(&path) {
        Ok(m) if m.is_file() && m.permissions().mode() & 0o111 != 0 => {}
        Ok(m) if m.is_file() => return Err(format!("{shown} is not executable{hint}")),
        Ok(_) => return Err(format!("{shown} is not a file{hint}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(format!("{shown} does not exist{hint}"));
        }
        Err(e) => return Err(format!("{shown}: {e}")),
    }
    std::fs::canonicalize(&path).map_err(|e| format!("{shown}: {e}"))
}

/// How a `slip` run ended: its exit status and its standard output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ran {
    pub status: i32,
    pub stdout: String,
}

/// Runs `slip` with `args`, the inherited environment and standard error,
/// standard input closed, and its standard output read. A `slip` that
/// cannot be started, that was killed by a signal, or whose standard
/// output is not UTF-8 is an error; any exit status is returned (the
/// caller decides which it accepts).
pub fn run(slip: &Path, args: &[OsString]) -> Result<Ran, String> {
    run_to(slip, args, Stdio::inherit())
}

/// [`run`], with `slip`'s standard error sent to `stderr` (a log file, so
/// that it does not interleave with what the caller prints).
pub fn run_to(slip: &Path, args: &[OsString], stderr: Stdio) -> Result<Ran, String> {
    use std::os::unix::process::ExitStatusExt;
    let shown = slip.display();
    let out = Command::new(slip)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(stderr)
        .output()
        .map_err(|e| format!("cannot run {shown}: {e}"))?;
    let status = match (out.status.code(), out.status.signal()) {
        (Some(code), _) => code,
        (None, Some(sig)) => return Err(format!("{shown} was killed by signal {sig}")),
        (None, None) => return Err(format!("{shown} ended without an exit status")),
    };
    let stdout = String::from_utf8(out.stdout)
        .map_err(|e| format!("the output of {shown} is not UTF-8: {e}"))?;
    Ok(Ran { status, stdout })
}

/// The rest of the first line of `stdout` that is `<key> <rest>`: the
/// key, one space, and everything after it up to the end of the line.
/// A line that is the key alone, or the key followed by anything but a
/// space, does not match.
pub fn line<'a>(stdout: &'a str, key: &str) -> Option<&'a str> {
    stdout.lines().find_map(|l| l.strip_prefix(key).and_then(|r| r.strip_prefix(' ')))
}

/// Removes `dir` and everything in it, without following a symbolic link
/// (a link is removed, not what it points to); a `dir` that does not exist
/// is not an error.
///
/// Refuses a `dir` that is not strictly under `under`: both must be
/// absolute and free of `.` and `..` components, `dir` must be `under`
/// followed by at least one more component, and the real path of `dir`'s
/// parent must be under the real path of `under`, so that no symbolic link
/// on the way leads out of it. xtask deletes nothing outside a corpus
/// directory (`spec/harness.md` §9.1).
pub fn remove_tree(under: &Path, dir: &Path) -> Result<(), String> {
    let refuse =
        |why: &str| Err(format!("refusing to remove {}: {why} {}", dir.display(), under.display()));
    let plain = |p: &Path| {
        p.is_absolute()
            && p.components().all(|c| matches!(c, Component::RootDir | Component::Normal(_)))
    };
    if !plain(under) || !plain(dir) {
        return refuse(
            "it and the directory it must be under need absolute paths without `.` or `..`:",
        );
    }
    let Ok(rest) = dir.strip_prefix(under) else {
        return refuse("it is not under");
    };
    if rest.as_os_str().is_empty() {
        return refuse("it is");
    }
    let parent = dir.parent().unwrap_or(dir);
    let real_under = std::fs::canonicalize(under)
        .map_err(|e| format!("refusing to remove {}: {}: {e}", dir.display(), under.display()))?;
    match std::fs::canonicalize(parent) {
        Ok(real) if real.starts_with(&real_under) => {}
        Ok(real) => {
            return Err(format!(
                "refusing to remove {}: its directory is really {}, not under {}",
                dir.display(),
                real.display(),
                real_under.display()
            ));
        }
        // No parent: there is nothing to remove.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(format!("{}: {e}", parent.display())),
    }
    match std::fs::symlink_metadata(dir) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("{}: {e}", dir.display())),
        Ok(m) if m.is_dir() => walk_remove(dir),
        Ok(_) => {
            std::fs::remove_file(dir).map_err(|e| format!("cannot remove {}: {e}", dir.display()))
        }
    }
}

/// Removes the directory `dir` (not a link) by walking it depth first.
fn walk_remove(dir: &Path) -> Result<(), String> {
    let entries =
        std::fs::read_dir(dir).map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
        let path = entry.path();
        let kind = entry.file_type().map_err(|e| format!("{}: {e}", path.display()))?;
        if kind.is_dir() {
            walk_remove(&path)?;
        } else {
            std::fs::remove_file(&path)
                .map_err(|e| format!("cannot remove {}: {e}", path.display()))?;
        }
    }
    std::fs::remove_dir(dir).map_err(|e| format!("cannot remove {}: {e}", dir.display()))
}
