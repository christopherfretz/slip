//! The `local-paths` lint (`spec/harness.md` §3): no listed text file holds
//! an absolute path into a home directory or this host's workspace.
//! Ported from Kiln's `xtask/src/local_paths.rs`, with Slip's prefixes.
//!
//! A *local path* starts with one of [`PREFIXES`] and goes on with at least
//! one path character, and its `/` starts a path: at the start of a line,
//! after a character that cannot be part of a path (a space, a quote, a
//! backtick, `=`, `(`, …), or after a one-letter flag (`-I/x`). A bare
//! prefix, followed by a quote or a backtick as in the rule's own text, is
//! not a path; nor is a prefix inside a longer path (`a/root/b`) or after a
//! placeholder or an expansion (`<sysroot>/root`, `$(pwd)/root`).
//!
//! No file is exempt. The rule text names only bare prefixes, and the tests
//! assemble their local paths at run time (`concat!` of a bare prefix and a
//! name), so neither spells a local path in the committed tree. Files that
//! are not UTF-8 or that contain a NUL byte are binary and skipped.

use super::Violation;
use crate::repo::Repo;

/// The prefixes that make a path local.
pub const PREFIXES: &[&str] = &["/home/", "/Users/", "/root/", "/Volumes/"];

/// `local-paths`.
pub fn check(repo: &Repo) -> Vec<Violation> {
    let mut out = Vec::new();
    for rel in repo.files() {
        let Ok(bytes) = std::fs::read(repo.root().join(rel)) else { continue };
        if bytes.contains(&0) {
            continue;
        }
        let Ok(text) = std::str::from_utf8(&bytes) else { continue };
        for (i, line) in text.lines().enumerate() {
            for p in local_paths(line) {
                out.push(Violation::new(
                    rel,
                    Some(i + 1),
                    "local-paths",
                    format!("absolute local path `{p}`"),
                ));
            }
        }
    }
    out
}

/// The local paths on `line`, each up to the next character that ends a
/// path.
pub fn local_paths(line: &str) -> Vec<&str> {
    let b = line.as_bytes();
    let mut found = Vec::new();
    let mut at = 0;
    while let Some(off) = line[at..].find('/') {
        let i = at + off;
        at = i + 1;
        if !starts_path(b, i) {
            continue;
        }
        let rest = &line[i..];
        let Some(prefix) = PREFIXES.iter().find(|p| rest.starts_with(**p)) else { continue };
        if rest[prefix.len()..].bytes().next().is_none_or(|c| !is_path_byte(c)) {
            continue;
        }
        let end = rest.bytes().position(ends_path).unwrap_or(rest.len());
        found.push(&rest[..end]);
        at = i + end.max(1);
    }
    found
}

/// Whether the `/` at `i` starts an absolute path.
fn starts_path(b: &[u8], i: usize) -> bool {
    let Some(&prev) = i.checked_sub(1).and_then(|j| b.get(j)) else { return true };
    if matches!(prev, b'>' | b'}' | b')' | b'$' | b'*') {
        return false;
    }
    if !is_path_byte(prev) {
        return true;
    }
    // A one-letter flag: `-I/x`, ` -L/x`.
    prev.is_ascii_alphabetic() && i >= 2 && b[i - 2] == b'-' && (i == 2 || !is_path_byte(b[i - 3]))
}

fn is_path_byte(c: u8) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, b'/' | b'.' | b'_' | b'-' | b'~' | b'+' | b'@')
}

fn ends_path(c: u8) -> bool {
    c.is_ascii_whitespace() || matches!(c, b'"' | b'\'' | b'`' | b')' | b',' | b';' | b'>' | b']')
}
