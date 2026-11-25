//! `swallowed-result` (`spec/harness.md` §3): no `.rs` file under `crates/` or
//! `xtask/` discards the `Result` of a callback-catching parse helper
//! (`snippet::{parse, parse_with, parse_with_tables, preprocess}`,
//! `parse_observer_with`, `common::{with_side, digest_side}`).

use std::path::Path;

use super::Violation;
use super::rust_tokens::{Token, tokens};
use crate::repo::Repo;

const LINT: &str = "swallowed-result";

pub fn check(repo: &Repo) -> Vec<Violation> {
    let mut out = Vec::new();
    for path in repo.files() {
        if !(path.starts_with("crates/") || path.starts_with("xtask/"))
            || !Path::new(path).extension().is_some_and(|ext| ext.eq_ignore_ascii_case("rs"))
        {
            continue;
        }
        let Ok(src) = std::fs::read_to_string(repo.root().join(path)) else {
            continue;
        };
        scan(path, &src, &mut out);
    }
    out
}

fn scan(path: &str, src: &str, out: &mut Vec<Violation>) {
    let toks = tokens(src);
    let mut i = 0;
    while i < toks.len() {
        // Form 1: `let _ = <call>(...);` without `.expect`/`.unwrap`/`?` at depth 0.
        if at(&toks, i, &["let", "_", "="]) {
            if let Some((call_line, name, close)) = find_target_call(&toks, i + 3, ";")
                && !propagates_after(&toks, close + 1, ";")
            {
                out.push(Violation::new(
                    path,
                    Some(call_line),
                    LINT,
                    format!(
                        "`let _ =` discards the Result of `{name}` (a callback panic would be swallowed)"
                    ),
                ));
            }
            i += 3;
            continue;
        }

        // Form 2: `if let Ok(...) = <call>(...) {` or `let Ok(...) = <call>(...) else`
        if at(&toks, i, &["let", "Ok", "("]) {
            let is_if_let = i > 0 && toks[i - 1].text == "if";
            let stop = if is_if_let { "{" } else { "else" };
            if let Some(after_pat) = skip_balanced(&toks, i + 2, "(", ")")
                && toks.get(after_pat).is_some_and(|t| t.text == "=")
                && let Some((call_line, name, close)) = find_target_call(&toks, after_pat + 1, stop)
                && !propagates_after(&toks, close + 1, stop)
            {
                let form = if is_if_let { "if let Ok" } else { "let Ok ... else" };
                out.push(Violation::new(
                    path,
                    Some(call_line),
                    LINT,
                    format!(
                        "`{form}` discards the Err of `{name}` (a callback panic would be swallowed)"
                    ),
                ));
            }
            i += 3;
            continue;
        }

        // Form 3: `<call>(...).ok()`
        if let Some((name, open)) = target_call_at(&toks, i)
            && let Some(after) = skip_balanced(&toks, open, "(", ")")
            && at(&toks, after, &[".", "ok", "("])
        {
            out.push(Violation::new(
                path,
                Some(toks[open].line),
                LINT,
                format!(
                    "`.ok()` discards the Err of `{name}` (a callback panic would be swallowed)"
                ),
            ));
        }

        i += 1;
    }
}

fn at(toks: &[Token<'_>], i: usize, pat: &[&str]) -> bool {
    toks.get(i..i + pat.len()).is_some_and(|slice| slice.iter().zip(pat).all(|(t, &p)| t.text == p))
}

/// If a target call starts at `i`, returns `(display_name, index_of_open_paren)`.
fn target_call_at(toks: &[Token<'_>], i: usize) -> Option<(&'static str, usize)> {
    if at(toks, i, &["snippet", "::", "parse", "("]) {
        return Some(("snippet::parse", i + 3));
    }
    if at(toks, i, &["snippet", "::", "preprocess", "("]) {
        return Some(("snippet::preprocess", i + 3));
    }
    if at(toks, i, &["common", "::", "with_side", "("]) {
        return Some(("common::with_side", i + 3));
    }
    if at(toks, i, &["common", "::", "digest_side", "("]) {
        return Some(("common::digest_side", i + 3));
    }
    let prev = i.checked_sub(1).and_then(|k| toks.get(k)).map(|t| t.text);
    if matches!(prev, Some("." | "fn")) {
        return None;
    }
    let name = toks.get(i)?.text;
    if toks.get(i + 1).is_some_and(|t| t.text == "(") {
        match name {
            "parse_with" => return Some(("parse_with", i + 1)),
            "parse_with_tables" => return Some(("parse_with_tables", i + 1)),
            "parse_observer_with" => return Some(("parse_observer_with", i + 1)),
            _ => {}
        }
    }
    None
}

/// Finds a target call at depth 0 of the expression starting at `start` before
/// `stop` (`;`, `{`, or `else`), returning `(line, name, close_paren_index)`.
fn find_target_call(
    toks: &[Token<'_>],
    start: usize,
    stop: &str,
) -> Option<(usize, &'static str, usize)> {
    let mut i = start;
    let mut depth = 0usize;
    while i < toks.len() {
        let t = toks[i].text;
        if depth == 0 && (t == stop || t == ";") {
            return None;
        }
        if depth == 0
            && let Some((name, open)) = target_call_at(toks, i)
            && let Some(after) = skip_balanced(toks, open, "(", ")")
        {
            return Some((toks[open].line, name, after - 1));
        }
        match t {
            "(" | "[" | "{" => depth += 1,
            ")" | "]" | "}" => depth = depth.saturating_sub(1),
            _ => {}
        }
        i += 1;
    }
    None
}

/// Whether the suffix after a call's closing `)` up to `stop` propagates
/// errors (`?`, `.unwrap`, `.unwrap_or_else`, `.expect`, etc.).
fn propagates_after(toks: &[Token<'_>], start: usize, stop: &str) -> bool {
    let mut i = start;
    let mut depth = 0usize;
    while i < toks.len() {
        let t = toks[i].text;
        if depth == 0 && (t == stop || t == ";") {
            return false;
        }
        if depth == 0 {
            if t == "?" {
                return true;
            }
            if t == "."
                && toks.get(i + 1).is_some_and(|n| {
                    matches!(
                        n.text,
                        "unwrap" | "unwrap_err" | "unwrap_or_else" | "expect" | "expect_err"
                    )
                })
            {
                return true;
            }
        }
        match t {
            "(" | "[" | "{" => depth += 1,
            ")" | "]" | "}" => depth = depth.saturating_sub(1),
            _ => {}
        }
        i += 1;
    }
    false
}

/// Starting at `open` (which must be `open_tok`), returns the index just after
/// the matching `close_tok`.
fn skip_balanced(
    toks: &[Token<'_>],
    open: usize,
    open_tok: &str,
    close_tok: &str,
) -> Option<usize> {
    if toks.get(open)?.text != open_tok {
        return None;
    }
    let mut depth = 0usize;
    let mut i = open;
    while i < toks.len() {
        if toks[i].text == open_tok {
            depth += 1;
        } else if toks[i].text == close_tok {
            depth -= 1;
            if depth == 0 {
                return Some(i + 1);
            }
        }
        i += 1;
    }
    None
}
