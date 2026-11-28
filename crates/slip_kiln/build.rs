//! Records the pinned `kiln` submodule commit (`SLIP_KILN_PIN`, 9 lowercase
//! hex digits) at build time so Slip's tool identity (`slip_kiln::kiln_version`)
//! distinguishes builds across Kiln pin moves even when `kiln_driver`'s semver
//! (`MAJOR.MINOR.PATCH`) does not change.
#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

/// Length of the short Kiln pin embedded in `tool_kiln` (matches the 9-hex
/// pin spelling used across Slip's specs and work packages).
const PIN_LEN: usize = 9;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let Some(manifest) = std::env::var_os("CARGO_MANIFEST_DIR").map(PathBuf::from) else {
        return;
    };
    // crates/slip_kiln -> workspace root -> kiln submodule.
    let Some(root) = manifest.parent().and_then(Path::parent) else {
        return;
    };
    let kiln = root.join("kiln");
    let dot_git = kiln.join(".git");
    if dot_git.is_file() {
        println!("cargo:rerun-if-changed={}", dot_git.display());
    }

    if let Some(pin) = pin_via_git(&kiln).or_else(|| pin_via_git_files(&kiln)) {
        println!("cargo:rustc-env=SLIP_KILN_PIN={pin}");
    }
}

fn short_pin(full_or_short: &str) -> Option<String> {
    let trimmed = full_or_short.trim();
    if trimmed.len() >= PIN_LEN && trimmed.bytes().all(|b| b.is_ascii_hexdigit()) {
        Some(trimmed[..PIN_LEN].to_ascii_lowercase())
    } else {
        None
    }
}

fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git").arg("-C").arg(dir).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8(out.stdout).ok()?;
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// Resolves the `kiln` submodule's HEAD commit through `git` and emits
/// `cargo:rerun-if-changed` directives for the submodule's git metadata files
/// (handling git worktrees where `kiln/.git` is a `gitdir:` file).
fn pin_via_git(kiln: &Path) -> Option<String> {
    let top = git(kiln, &["rev-parse", "--show-toplevel"])?;
    // Ensure `kiln` is its own git work tree rather than inheriting the parent
    // repository's HEAD when built from an unpacked archive without `kiln/.git`.
    if std::fs::canonicalize(&top).ok()? != std::fs::canonicalize(kiln).ok()? {
        return None;
    }
    let pin = short_pin(&git(kiln, &["rev-parse", "HEAD"])?)?;
    let mut watch = vec!["HEAD".to_owned(), "packed-refs".to_owned()];
    if let Some(sym) = git(kiln, &["symbolic-ref", "-q", "HEAD"]) {
        watch.push(sym);
    }
    for w in &watch {
        if let Some(p) = git(kiln, &["rev-parse", "--path-format=absolute", "--git-path", w])
            && Path::new(&p).is_file()
        {
            println!("cargo:rerun-if-changed={p}");
        }
    }
    Some(pin)
}

/// Fallback when `git` is not available on `PATH`: resolves `kiln/.git`
/// (directory or `gitdir:` file), `HEAD`, loose refs, and `packed-refs`.
fn pin_via_git_files(kiln: &Path) -> Option<String> {
    let dot_git = kiln.join(".git");
    let git_dir = if dot_git.is_dir() {
        dot_git
    } else if dot_git.is_file() {
        let text = std::fs::read_to_string(&dot_git).ok()?;
        let rel = text.lines().next()?.strip_prefix("gitdir:")?.trim();
        let p = Path::new(rel);
        if p.is_absolute() { p.to_path_buf() } else { kiln.join(p) }
    } else {
        return None;
    };
    let head_path = git_dir.join("HEAD");
    if head_path.is_file() {
        println!("cargo:rerun-if-changed={}", head_path.display());
    }
    let head = std::fs::read_to_string(&head_path).ok()?;
    let head = head.trim();
    let Some(ref_name) = head.strip_prefix("ref:") else {
        return short_pin(head);
    };
    let ref_name = ref_name.trim();
    let loose = git_dir.join(ref_name);
    if loose.is_file() {
        println!("cargo:rerun-if-changed={}", loose.display());
        if let Ok(oid) = std::fs::read_to_string(&loose)
            && let Some(pin) = short_pin(&oid)
        {
            return Some(pin);
        }
    }
    let packed = git_dir.join("packed-refs");
    if packed.is_file() {
        println!("cargo:rerun-if-changed={}", packed.display());
        let text = std::fs::read_to_string(&packed).ok()?;
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with('^') {
                continue;
            }
            if let Some((oid, name)) = line.split_once(' ')
                && name.trim() == ref_name
            {
                return short_pin(oid);
            }
        }
    }
    None
}
