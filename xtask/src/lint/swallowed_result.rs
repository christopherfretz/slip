//! `swallowed-result` (`spec/harness.md` §3): no `.rs` file under `crates/` or
//! `xtask/` discards the `Result` of a callback-catching parse helper
//! (`snippet::{parse, parse_with, parse_with_tables, preprocess}`,
//! `parse_observer_with`,
//! `common::{with_side, digest_side, digest_side_with, skip_unparsed}`).

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

/// A local variable bound to the unpropagated `Result` of a target helper in
/// the current block scope.
struct Binding<'a> {
    var: &'a str,
    call_name: &'static str,
    depth: usize,
}

fn scan(path: &str, src: &str, out: &mut Vec<Violation>) {
    let toks = tokens(src);
    let mut bindings: Vec<Binding<'_>> = Vec::new();
    let mut depth = 0usize;
    let mut i = 0;

    while i < toks.len() {
        let tok = toks[i].text;
        if tok == "{" {
            depth += 1;
            i += 1;
            continue;
        }
        if tok == "}" {
            bindings.retain(|b| b.depth < depth);
            depth = depth.saturating_sub(1);
            i += 1;
            continue;
        }

        // Form 1 & 2: `let _ =`, `let _name =`, `let r =`, `if let Ok`, `let Ok ... else`.
        if tok == "let" {
            let is_if_let = i > 0 && toks[i - 1].text == "if";
            if at(&toks, i, &["let", "Ok", "("]) {
                check_let_ok(path, &toks, i, is_if_let, &bindings, out);
            } else if !is_if_let {
                check_let_binding(path, &toks, i, depth, &mut bindings, out);
            }
            i += 1;
            continue;
        }

        // Form 4: `match <expr> { ... }` discarding `Err` in an unguarded arm.
        if tok == "match" {
            if let Some((_, name, brace_idx)) = find_target_or_binding(&toks, i + 1, "{", &bindings)
            {
                check_match_arms(path, &toks, brace_idx, name, out);
            }
            i += 1;
            continue;
        }

        // Form 3: discarding method suffix directly on `<call>(...)`
        // (`.ok()`, `.unwrap_or_default()`, `.unwrap_or(...)`, `.is_ok()`,
        // or discarding `.unwrap_or_else(...)`).
        if let Some((name, open)) = target_call_at(&toks, i)
            && let Some(after) = skip_balanced(&toks, open, "(", ")")
            && let Some(method) = discarding_suffix_at(&toks, after)
        {
            out.push(Violation::new(
                path,
                Some(toks[open].line),
                LINT,
                format!(
                    "`{method}` discards the Err of `{name}` (a callback panic would be swallowed)"
                ),
            ));
        }

        // Form 3b: discarding method suffix on a tracked bound variable (`r.ok()`, etc.).
        if is_ident(tok)
            && !matches!(
                i.checked_sub(1).and_then(|k| toks.get(k)).map(|t| t.text),
                Some("." | "::" | "fn" | "let")
            )
            && let Some(b) = bindings.iter().rev().find(|b| b.var == tok)
            && let Some(method) = discarding_suffix_at(&toks, i + 1)
        {
            out.push(Violation::new(
                path,
                Some(toks[i].line),
                LINT,
                format!(
                    "`{method}` discards the Err of `{}` (a callback panic would be swallowed)",
                    b.call_name
                ),
            ));
        }

        i += 1;
    }
}

/// Form 2: `if let Ok(...) = <expr> { ... }` (without a non-empty `else`) or
/// `let Ok(...) = <expr> else`.
fn check_let_ok(
    path: &str,
    toks: &[Token<'_>],
    i: usize,
    is_if_let: bool,
    bindings: &[Binding<'_>],
    out: &mut Vec<Violation>,
) {
    let stop = if is_if_let { "{" } else { "else" };
    let Some(after_pat) = skip_balanced(toks, i + 2, "(", ")") else {
        return;
    };
    if toks.get(after_pat).is_none_or(|t| t.text != "=") {
        return;
    }
    let Some((line, name, end_idx)) = find_target_or_binding(toks, after_pat + 1, stop, bindings)
    else {
        return;
    };
    if is_if_let {
        let Some(after_body) = skip_balanced(toks, end_idx, "{", "}") else {
            return;
        };
        let discards = if toks.get(after_body).is_some_and(|t| t.text == "else") {
            // `else {}` still discards `Err`; a non-empty `else` handles it.
            at(toks, after_body + 1, &["{", "}"])
        } else {
            true
        };
        if discards {
            out.push(Violation::new(
                path,
                Some(line),
                LINT,
                format!(
                    "`if let Ok` discards the Err of `{name}` (a callback panic would be swallowed)"
                ),
            ));
        }
    } else {
        out.push(Violation::new(
            path,
            Some(line),
            LINT,
            format!(
                "`let Ok ... else` discards the Err of `{name}` (a callback panic would be swallowed)"
            ),
        ));
    }
}

/// Inspects `let [mut] <name> [: <ty>] = <rhs>;` starting at `let_idx`.
/// Flags `let _ =` and `let _name =` when `<rhs>` does not propagate, and
/// records regular bindings to unpropagated target calls in `bindings`.
fn check_let_binding<'a>(
    path: &str,
    toks: &[Token<'a>],
    let_idx: usize,
    depth: usize,
    bindings: &mut Vec<Binding<'a>>,
    out: &mut Vec<Violation>,
) {
    let mut k = let_idx + 1;
    if toks.get(k).is_some_and(|t| t.text == "mut") {
        k += 1;
    }
    let Some(name_tok) = toks.get(k) else {
        return;
    };
    let var = name_tok.text;
    if !is_ident(var) {
        return;
    }
    k += 1;
    // Skip optional `: <type>` up to `=` at depth 0.
    let mut ty_depth = 0usize;
    while k < toks.len() {
        let t = toks[k].text;
        if ty_depth == 0 {
            if t == "=" {
                break;
            }
            if t == ";" {
                return;
            }
        }
        match t {
            "(" | "[" | "{" | "<" => ty_depth += 1,
            ")" | "]" | "}" | ">" => ty_depth = ty_depth.saturating_sub(1),
            _ => {}
        }
        k += 1;
    }
    if toks.get(k).map(|t| t.text) != Some("=") {
        return;
    }
    let rhs = k + 1;

    if var == "_" || var.starts_with('_') {
        if let Some((call_line, name, close)) = find_target_call(toks, rhs, ";")
            && !propagates_after(toks, close + 1, ";")
            && discarding_suffix_at(toks, close + 1).is_none()
        {
            out.push(Violation::new(
                path,
                Some(call_line),
                LINT,
                format!(
                    "`let {var} =` discards the Result of `{name}` (a callback panic would be swallowed)"
                ),
            ));
        }
        return;
    }

    // Shadow any existing binding of `var` at the same depth.
    bindings.retain(|b| !(b.var == var && b.depth == depth));
    if let Some((_, call_name, close)) = find_target_call(toks, rhs, ";")
        && !propagates_after(toks, close + 1, ";")
        && discarding_suffix_at(toks, close + 1).is_none()
    {
        bindings.push(Binding { var, call_name, depth });
    }
}

/// Inspects the arms of `match <target_or_binding> { ... }` where `brace_idx`
/// points at `{`. Flags an unguarded `Err(_)` / `Err(_name)` / `Err(..)` / `_`
/// arm whose body discards the error when no earlier guarded `Err(...) if ...`
/// arm handled panics.
fn check_match_arms(
    path: &str,
    toks: &[Token<'_>],
    brace_idx: usize,
    call_name: &'static str,
    out: &mut Vec<Violation>,
) {
    let Some(end) = skip_balanced(toks, brace_idx, "{", "}") else {
        return;
    };
    let mut i = brace_idx + 1;
    let body_end = end - 1;
    let mut saw_guarded_err = false;

    while i < body_end {
        // Parse one arm: `<pat> [if <guard>] => <expr> [,]`
        let pat_start = i;
        let mut arm_depth = 0usize;
        let mut arrow = None;
        let mut guard = None;
        while i < body_end {
            let t = toks[i].text;
            if arm_depth == 0 {
                if at(toks, i, &["=", ">"]) {
                    arrow = Some(i);
                    break;
                }
                if t == "if" && guard.is_none() {
                    guard = Some(i);
                }
            }
            match t {
                "(" | "[" | "{" => arm_depth += 1,
                ")" | "]" | "}" => arm_depth = arm_depth.saturating_sub(1),
                _ => {}
            }
            i += 1;
        }
        let Some(arrow_idx) = arrow else {
            break;
        };
        let pat_end = guard.unwrap_or(arrow_idx);
        let pat = &toks[pat_start..pat_end];

        // Find the end of the arm body (`=>` is two tokens: `=` and `>`).
        let expr_start = arrow_idx + 2;
        let expr_end = if toks.get(expr_start).is_some_and(|t| t.text == "{") {
            let after_block = skip_balanced(toks, expr_start, "{", "}").unwrap_or(body_end);
            i = after_block;
            if i < body_end && toks[i].text == "," {
                i += 1;
            }
            after_block
        } else {
            let mut k = expr_start;
            let mut d = 0usize;
            while k < body_end {
                let t = toks[k].text;
                if d == 0 && t == "," {
                    break;
                }
                match t {
                    "(" | "[" | "{" => d += 1,
                    ")" | "]" | "}" => d = d.saturating_sub(1),
                    _ => {}
                }
                k += 1;
            }
            i = if k < body_end && toks[k].text == "," { k + 1 } else { k };
            k
        };

        let is_err_pat = pat_matches_err(pat);
        if is_err_pat && guard.is_some() {
            saw_guarded_err = true;
            continue;
        }
        if !saw_guarded_err
            && pat_discards_err(pat)
            && is_discarding_arm_body(&toks[expr_start..expr_end])
        {
            out.push(Violation::new(
                path,
                Some(toks[pat_start].line),
                LINT,
                format!(
                    "`match` discards the Err of `{call_name}` (a callback panic would be swallowed)"
                ),
            ));
        }
    }
}

/// Whether a match-arm pattern mentions `Err`.
fn pat_matches_err(pat: &[Token<'_>]) -> bool {
    pat.iter().any(|t| t.text == "Err")
}

/// Whether a match-arm pattern ignores the `Err` payload (`Err(_)`, `Err(_x)`,
/// `Err(..)`, or bare `_`).
fn pat_discards_err(pat: &[Token<'_>]) -> bool {
    if pat.len() == 1 && pat[0].text == "_" {
        return true;
    }
    let mut k = 0;
    while k < pat.len() {
        if pat[k].text == "Err"
            && pat.get(k + 1).is_some_and(|t| t.text == "(")
            && let Some(after) = skip_balanced(pat, k + 1, "(", ")")
        {
            let inner = &pat[k + 2..after - 1];
            if inner.is_empty()
                || (inner.len() == 1 && (inner[0].text == "_" || inner[0].text.starts_with('_')))
                || inner.iter().all(|t| t.text == ".")
            {
                return true;
            }
        }
        k += 1;
    }
    false
}

/// Whether a match-arm body discards the error rather than handling/recording
/// it (`{}`, `()`, `continue`, `break`, `None`, `return`, or a block containing
/// only those / `let _ = ...;`).
fn is_discarding_arm_body(body: &[Token<'_>]) -> bool {
    let mut slice = body;
    if slice.len() >= 2 && slice[0].text == "{" && slice[slice.len() - 1].text == "}" {
        slice = &slice[1..slice.len() - 1];
    }
    if slice.is_empty() {
        return true;
    }
    if slice.len() == 2 && slice[0].text == "(" && slice[1].text == ")" {
        return true;
    }
    // Strip trailing `;` inside a single-statement block.
    if slice.last().is_some_and(|t| t.text == ";") {
        slice = &slice[..slice.len() - 1];
    }
    matches!(slice.first().map(|t| t.text), Some("continue" | "break" | "None"))
        || (slice.len() == 1 && slice[0].text == "return")
        || (slice.len() >= 2 && slice[0].text == "return" && matches!(slice[1].text, "None" | "("))
}

fn at(toks: &[Token<'_>], i: usize, pat: &[&str]) -> bool {
    toks.get(i..i + pat.len()).is_some_and(|slice| slice.iter().zip(pat).all(|(t, &p)| t.text == p))
}

fn is_ident(s: &str) -> bool {
    s.bytes().next().is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
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
    if at(toks, i, &["common", "::", "digest_side_with", "("]) {
        return Some(("common::digest_side_with", i + 3));
    }
    if at(toks, i, &["common", "::", "skip_unparsed"])
        && let Some(open) = open_paren_after(toks, i + 3)
    {
        return Some(("common::skip_unparsed", open));
    }
    let prev = i.checked_sub(1).and_then(|k| toks.get(k)).map(|t| t.text);
    if matches!(prev, Some("." | "fn")) {
        return None;
    }
    let name = toks.get(i)?.text;
    if let Some(open) = open_paren_after(toks, i + 1) {
        match name {
            "parse_with" => return Some(("parse_with", open)),
            "parse_with_tables" => return Some(("parse_with_tables", open)),
            "parse_observer_with" => return Some(("parse_observer_with", open)),
            _ => {}
        }
    }
    None
}

/// Returns the index of `(` starting at `i`, skipping an optional turbofish
/// `::<...>`.
fn open_paren_after(toks: &[Token<'_>], i: usize) -> Option<usize> {
    if toks.get(i).is_some_and(|t| t.text == "(") {
        return Some(i);
    }
    if at(toks, i, &["::", "<"]) {
        let after = skip_balanced(toks, i + 1, "<", ">")?;
        if toks.get(after).is_some_and(|t| t.text == "(") {
            return Some(after);
        }
    }
    None
}

/// Finds either an unpropagated target call at depth 0 before `stop`, or a
/// tracked binding `<var>` (optionally `&<var>` / `&mut <var>`) right before
/// `stop`. Returns `(line, call_name, stop_token_index)`.
fn find_target_or_binding(
    toks: &[Token<'_>],
    start: usize,
    stop: &str,
    bindings: &[Binding<'_>],
) -> Option<(usize, &'static str, usize)> {
    // Check direct tracked binding: `[& [mut]] <var> <stop>`
    let mut k = start;
    if toks.get(k).is_some_and(|t| t.text == "&") {
        k += 1;
        if toks.get(k).is_some_and(|t| t.text == "mut") {
            k += 1;
        }
    }
    if let Some(var_tok) = toks.get(k)
        && toks.get(k + 1).is_some_and(|t| t.text == stop)
        && let Some(b) = bindings.iter().rev().find(|b| b.var == var_tok.text)
    {
        return Some((var_tok.line, b.call_name, k + 1));
    }

    // Otherwise look for a direct target call at depth 0 before `stop`.
    let (call_line, name, close) = find_target_call(toks, start, stop)?;
    if propagates_after(toks, close + 1, stop) {
        return None;
    }
    let stop_idx = find_stop(toks, close + 1, stop)?;
    Some((call_line, name, stop_idx))
}

fn find_stop(toks: &[Token<'_>], start: usize, stop: &str) -> Option<usize> {
    let mut i = start;
    let mut depth = 0usize;
    while i < toks.len() {
        let t = toks[i].text;
        if depth == 0 {
            if t == stop {
                return Some(i);
            }
            if t == ";" {
                return None;
            }
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

/// If a discarding method call starts at `i` (`.ok()`, `.unwrap_or_default()`,
/// `.unwrap_or(...)`, `.is_ok()`, or a discarding `.unwrap_or_else(...)`),
/// returns its display label.
fn discarding_suffix_at(toks: &[Token<'_>], i: usize) -> Option<&'static str> {
    if at(toks, i, &[".", "ok", "("]) {
        return Some(".ok()");
    }
    if at(toks, i, &[".", "unwrap_or_default", "("]) {
        return Some(".unwrap_or_default()");
    }
    if at(toks, i, &[".", "unwrap_or", "("]) {
        return Some(".unwrap_or()");
    }
    if at(toks, i, &[".", "is_ok", "("]) {
        return Some(".is_ok()");
    }
    if at(toks, i, &[".", "unwrap_or_else", "("]) && is_discarding_unwrap_or_else(toks, i + 2) {
        return Some(".unwrap_or_else()");
    }
    None
}

/// Whether `.unwrap_or_else(...)` (with `open` at `(`) discards the `Err`:
/// either the closure body is `{}` / `()`, or the closure ignores its error
/// parameter (`|_|` / `|_e|`) without panicking/asserting/returning `Err`.
fn is_discarding_unwrap_or_else(toks: &[Token<'_>], open: usize) -> bool {
    let Some(after) = skip_balanced(toks, open, "(", ")") else {
        return false;
    };
    let inner = &toks[open + 1..after - 1];
    let mut k = 0;
    if inner.get(k).is_some_and(|t| t.text == "move") {
        k += 1;
    }
    if inner.get(k).map(|t| t.text) != Some("|") {
        return false;
    }
    // Find closing `|` of closure parameter list.
    let param_start = k + 1;
    let Some(pipe_rel) = inner[param_start..].iter().position(|t| t.text == "|") else {
        return false;
    };
    let params = &inner[param_start..param_start + pipe_rel];
    let body = &inner[param_start + pipe_rel + 1..];

    // Empty block `{}` or unit `()` always discards `Err`.
    if (body.len() == 2 && body[0].text == "{" && body[1].text == "}")
        || (body.len() == 2 && body[0].text == "(" && body[1].text == ")")
    {
        return true;
    }
    // Underscore-ignored error parameter (`|_|` or `|_e|`) unless body panics /
    // asserts / constructs `Err`.
    let ignores_param = params.first().is_some_and(|t| t.text == "_" || t.text.starts_with('_'));
    if ignores_param {
        let handles = body.iter().any(|t| {
            matches!(
                t.text,
                "panic"
                    | "unreachable"
                    | "assert"
                    | "assert_eq"
                    | "assert_ne"
                    | "Err"
                    | "todo"
                    | "unimplemented"
            )
        });
        return !handles;
    }
    false
}

/// Whether the suffix after a call's closing `)` up to `stop` propagates
/// errors (`?`, `.unwrap`, `.unwrap_err`, `.expect`, `.expect_err`, or a
/// non-discarding `.unwrap_or_else`).
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
                && let Some(method) = toks.get(i + 1).map(|n| n.text)
            {
                if matches!(method, "unwrap" | "unwrap_err" | "expect" | "expect_err") {
                    return true;
                }
                if method == "unwrap_or_else"
                    && toks.get(i + 2).is_some_and(|p| p.text == "(")
                    && !is_discarding_unwrap_or_else(toks, i + 2)
                {
                    return true;
                }
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
