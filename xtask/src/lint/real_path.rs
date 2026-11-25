//! The `real-path` lint (`spec/harness.md` §3, `spec/edits.md` §2.1):
//! outside `crates/slip_spelling/`, non-test code under `crates/*/src/`
//! calls `slip_spelling::real_path`, never `canonicalize`, unless the line
//! directly above carries `// canonicalize: ` with a non-empty reason.

use std::path::Path;

use super::Violation;
use super::rust_tokens::{Token, tokens};
use crate::repo::Repo;

const LINT: &str = "real-path";

/// Whether `path` is a `.rs` file under `crates/<crate>/src/` outside
/// `crates/slip_spelling/`.
fn checked_file(path: &str) -> bool {
    let Some(rest) = path.strip_prefix("crates/") else { return false };
    let Some((crate_name, inside)) = rest.split_once('/') else { return false };
    crate_name != "slip_spelling"
        && inside.starts_with("src/")
        && Path::new(path).extension().is_some_and(|e| e == "rs")
}

/// Whether `toks[i..]` starts with the outer attribute `#[cfg(test)]`.
fn is_cfg_test(toks: &[Token<'_>], i: usize) -> bool {
    const ATTR: [&str; 7] = ["#", "[", "cfg", "(", "test", ")", "]"];
    toks.get(i..i + ATTR.len()).is_some_and(|s| s.iter().zip(ATTR).all(|(t, w)| t.text == w))
}

/// Starting at `open` (`toks[i].text == open`), the index after its
/// matching `close` (or `toks.len()` if unbalanced).
fn skip_balanced(toks: &[Token<'_>], mut i: usize, open: &str, close: &str) -> usize {
    let mut depth = 0usize;
    while i < toks.len() {
        if toks[i].text == open {
            depth += 1;
        } else if toks[i].text == close {
            depth -= 1;
            if depth == 0 {
                return i + 1;
            }
        }
        i += 1;
    }
    i
}

/// Starting at `#[cfg(test)]`, the index after the annotated item (including
/// any stacked outer attributes and its `{ ... }` body, terminating `;` or
/// `,`, or stopping before an enclosing `}` that was not opened here).
fn skip_cfg_test_item(toks: &[Token<'_>], mut i: usize) -> usize {
    i += 7;
    while toks.get(i).is_some_and(|t| t.text == "#")
        && toks.get(i + 1).is_some_and(|t| t.text == "[")
    {
        i = skip_balanced(toks, i + 1, "[", "]");
    }
    let mut paren = 0usize;
    let mut bracket = 0usize;
    while i < toks.len() {
        match toks[i].text {
            "(" => {
                paren += 1;
                i += 1;
            }
            ")" => {
                paren = paren.saturating_sub(1);
                i += 1;
            }
            "[" => {
                bracket += 1;
                i += 1;
            }
            "]" => {
                bracket = bracket.saturating_sub(1);
                i += 1;
            }
            "{" if paren == 0 && bracket == 0 => return skip_balanced(toks, i, "{", "}"),
            ";" | "," if paren == 0 && bracket == 0 => return i + 1,
            "}" if paren == 0 && bracket == 0 => return i,
            _ => i += 1,
        }
    }
    i
}

/// Whether the line directly above `line` (1-based) carries
/// `// canonicalize: ` followed by a non-empty reason.
fn justified(lines: &[&str], line: usize) -> bool {
    line >= 2
        && lines
            .get(line - 2)
            .and_then(|l| l.trim_start().strip_prefix("// canonicalize: "))
            .is_some_and(|reason| !reason.trim().is_empty())
}

/// Every un-justified `canonicalize` token in `text` outside `#[cfg(test)]`
/// items, as 1-based line numbers.
fn bad_lines(text: &str) -> Vec<usize> {
    let lines: Vec<&str> = text.lines().collect();
    let toks = tokens(text);
    let mut out = Vec::new();
    let mut i = 0;
    while i < toks.len() {
        if is_cfg_test(&toks, i) {
            i = skip_cfg_test_item(&toks, i);
            continue;
        }
        if toks[i].text == "canonicalize" {
            let line = toks[i].line;
            if !justified(&lines, line) {
                out.push(line);
            }
        }
        i += 1;
    }
    out
}

/// `real-path`.
pub fn check(repo: &Repo) -> Vec<Violation> {
    let mut out = Vec::new();
    for path in repo.files().filter(|f| checked_file(f)) {
        let Some(text) = repo.read(path) else { continue };
        for line in bad_lines(&text) {
            out.push(Violation::new(
                path,
                Some(line),
                LINT,
                "names `canonicalize`; use `slip_spelling::real_path`, or justify on the line \
                 above with `// canonicalize: <reason>` (spec/edits.md §2.1)",
            ));
        }
    }
    out
}
