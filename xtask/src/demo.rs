//! `cargo xtask demo 1` (`spec/harness.md` §10): Demo 1, the first
//! demonstration of the trusted base on real code.
//!
//! ```text
//! cargo xtask demo 1 [--slip <path>] [-j <n>] [--case <name>]… [--keep]
//! ```
//!
//! The inputs are `bench/demo1/` (§10.1): [`read_config`] reads
//! `demo.toml`, [`read_edit`] a case's `edit.txt`, [`read_expect`] its
//! `expect.txt`. The run (§10.3) makes a tree per case of hard links to
//! the pristine corpus ([`link_tree`]), applies the edit by writing each
//! edited file as a new file ([`apply_edit`]), and runs `slip` through
//! [`slip_run`]. [`check`] is "the result of a case": pure, so that the
//! tests need neither `slip` nor a corpus. Reports are read as
//! [`json::parse`] reads them and runs as [`perf::load`] reads them; xtask
//! uses neither `slip_cli` nor `slip_verify` for this.
//!
//! Decisions where §10 is silent are in `spec/landed/harness/WP-059.md`.

use crate::corpus::{self, Corpus, Layout, rows};
use crate::invariants::{self, Env, Failure};
use crate::lint::manifest;
use crate::perf::{self, Loaded, render::text, seconds};
use crate::slip_run::{self, Ran};
use slip_model::json::{self, Value};
use slip_model::{Digest, Status};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fmt::Write as _;
use std::io::Write;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// The log of `slip`'s standard error for the version, the plan and the
/// map run, in `demo1/`; a case's is `demo1/<case>.log`.
const MAP_LOG: &str = "map.log";

fn case_log(case: &Case) -> String {
    format!("{}.log", case.name)
}

const USAGE: &str = "usage: cargo xtask demo 1 [--slip <path>] [-j <n>] [--case <name>]… [--keep]";

/// The inputs of Demo 1, relative to the repository root.
pub const INPUTS: &str = "bench/demo1";

/// `--findings` and `--locate` of every verify run (§10.3).
pub const FINDINGS: &str = "100000";
pub const LOCATE: &str = "1024";

/// At most this many groups are listed under a `DIFFERENT` case, and this
/// many `digest.cause.*` counters under `indeterminate` (§10.3).
pub const LISTED: usize = 10;

/// The report format (`spec/verify.md` §7.1).
pub const REPORT: &str = "slip.verify.v1";

// ---------------------------------------------------------------------------
// §10.1 Inputs

/// `demo.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// A row of `bench/corpora.toml`.
    pub corpus: String,
    /// The subject roots, relative to the corpus's `src/`.
    pub roots: Vec<String>,
    /// The selection: the `--only` of every `slip` run.
    pub only: Vec<String>,
}

/// Reads `demo.toml` with the line reader of the lints: exactly the keys
/// `corpus` (a string), `roots` and `only` (arrays of strings), outside
/// any table. A missing, repeated or unknown key, a table, a value of
/// another type, an empty `roots` or `only`, and a root that is not a
/// plain relative path are errors naming the line.
pub fn read_config(text: &str) -> Result<Config, String> {
    let (headers, entries) = manifest::parse(text);
    if let Some(h) = headers.first() {
        return Err(format!("line {}: demo.toml has no tables (`[{}]`)", h.line, h.table));
    }
    let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
    let (mut corpus, mut roots, mut only) = (None, None, None);
    for e in &entries {
        if let Some(first) = seen.insert(e.key.as_str(), e.line) {
            return Err(format!(
                "line {}: `{}` is given twice (first at line {first})",
                e.line, e.key
            ));
        }
        match e.key.as_str() {
            "corpus" => {
                corpus = Some(
                    one_string(&e.value)
                        .ok_or_else(|| format!("line {}: `corpus` must be one string", e.line))?,
                );
            }
            "roots" | "only" => {
                let items = string_array(&e.value).ok_or_else(|| {
                    format!("line {}: `{}` must be an array of strings", e.line, e.key)
                })?;
                if items.is_empty() {
                    return Err(format!("line {}: `{}` is empty", e.line, e.key));
                }
                if e.key == "roots" {
                    if let Some(bad) = items.iter().find(|r| !plain_relative(r)) {
                        return Err(format!(
                            "line {}: root `{bad}` is not a relative path without `.` or `..`",
                            e.line
                        ));
                    }
                    roots = Some(items);
                } else {
                    only = Some(items);
                }
            }
            other => return Err(format!("line {}: unknown key `{other}`", e.line)),
        }
    }
    let missing = |k: &str| format!("demo.toml has no `{k}`");
    Ok(Config {
        corpus: corpus.ok_or_else(|| missing("corpus"))?,
        roots: roots.ok_or_else(|| missing("roots"))?,
        only: only.ok_or_else(|| missing("only"))?,
    })
}

/// A value that is exactly one quoted string.
fn one_string(value: &str) -> Option<String> {
    let v = value.trim();
    let quoted = v.len() >= 2
        && ((v.starts_with('"') && v.ends_with('"')) || (v.starts_with('\'') && v.ends_with('\'')));
    let s = manifest::strings(v);
    (quoted && s.len() == 1).then(|| s.into_iter().next().unwrap_or_default())
}

/// A value that is an array of strings and nothing else.
fn string_array(value: &str) -> Option<Vec<String>> {
    let inner = value.trim().strip_prefix('[')?.strip_suffix(']')?;
    let items = manifest::strings(inner);
    let mut rest = inner.to_owned();
    for s in &items {
        let found = ['"', '\''].iter().find_map(|q| {
            let lit = format!("{q}{s}{q}");
            rest.find(&lit).map(|i| (i, lit.len()))
        });
        let (i, n) = found?;
        rest.replace_range(i..i + n, "");
    }
    rest.chars().all(|c| c == ',' || c.is_whitespace()).then_some(items)
}

/// A non-empty relative path whose components are all names (no `.`,
/// `..`, root, or empty component).
fn plain_relative(p: &str) -> bool {
    !p.is_empty()
        && !p.starts_with('/')
        && p.split('/').all(|c| !c.is_empty() && c != "." && c != "..")
}

/// One entry of `edit.txt`. `line` is the 1-based line of the `replace`
/// or `create` that starts it, for the errors of [`apply_edit`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    Replace { file: String, all: bool, old: Vec<u8>, new: Vec<u8>, line: usize },
    Create { file: String, content: Vec<u8>, line: usize },
}

impl Entry {
    /// The path the entry writes, relative to `src/`.
    pub fn file(&self) -> &str {
        match self {
            Entry::Replace { file, .. } | Entry::Create { file, .. } => file,
        }
    }

    pub fn line(&self) -> usize {
        match self {
            Entry::Replace { line, .. } | Entry::Create { line, .. } => *line,
        }
    }
}

/// Reads `edit.txt` (§10.1). A text is the lines between its keyword
/// line and `with` or `end`, each with its line feed: bytes, compared as
/// bytes. Errors name the line: a `replace` with no `file` above it, a
/// keyword line with extra words, a path that is absolute or has a `.` or
/// `..` component, an empty old text, a `with` or `end` missing before
/// the end of the file, a line that is no keyword, and a file with no
/// entry.
pub fn read_edit(text: &[u8]) -> Result<Vec<Entry>, String> {
    let lines: Vec<&[u8]> = split_lines(text);
    let mut out = Vec::new();
    let mut file: Option<String> = None;
    let mut i = 0;
    while i < lines.len() {
        let n = i + 1;
        let line = std::str::from_utf8(bare(lines[i]))
            .map_err(|_| format!("line {n}: a keyword line is not UTF-8"))?;
        if line.trim().is_empty() {
            i += 1;
            continue;
        }
        if let Some(path) = line.strip_prefix("file ") {
            file = Some(path_word(path, n)?);
            i += 1;
        } else if line == "replace" || line == "replace all" {
            let Some(f) = file.clone() else {
                return Err(format!("line {n}: `{line}` with no `file` line above it"));
            };
            let (old, next) = text_until(&lines, i + 1, &["with"], n)?;
            if old.is_empty() {
                return Err(format!("line {n}: the old text of `{line}` is empty"));
            }
            let (new, next) = text_until(&lines, next + 1, &["end"], n)?;
            out.push(Entry::Replace { file: f, all: line == "replace all", old, new, line: n });
            i = next + 1;
        } else if let Some(path) = line.strip_prefix("create ") {
            let f = path_word(path, n)?;
            let (content, next) = text_until(&lines, i + 1, &["end"], n)?;
            out.push(Entry::Create { file: f, content, line: n });
            i = next + 1;
        } else {
            return Err(format!("line {n}: `{line}` is not `file`, `replace [all]` or `create`"));
        }
    }
    if out.is_empty() {
        return Err("the edit has no entry".to_owned());
    }
    Ok(out)
}

/// The lines of `text`, each with its line feed (the last may have none).
fn split_lines(text: &[u8]) -> Vec<&[u8]> {
    let mut out = Vec::new();
    let mut start = 0;
    for (i, b) in text.iter().enumerate() {
        if *b == b'\n' {
            out.push(&text[start..=i]);
            start = i + 1;
        }
    }
    if start < text.len() {
        out.push(&text[start..]);
    }
    out
}

/// The line without its line feed.
fn bare(line: &[u8]) -> &[u8] {
    line.strip_suffix(b"\n").unwrap_or(line)
}

/// The text from line index `from` up to the first line that is exactly
/// one of `stops`; returns it and the index of that line. `opened` is the
/// 1-based line of the keyword, for the error.
fn text_until(
    lines: &[&[u8]],
    from: usize,
    stops: &[&str],
    opened: usize,
) -> Result<(Vec<u8>, usize), String> {
    let mut text = Vec::new();
    for (j, l) in lines.iter().enumerate().skip(from) {
        if stops.iter().any(|s| bare(l) == s.as_bytes()) {
            return Ok((text, j));
        }
        text.extend_from_slice(l);
    }
    Err(format!("line {opened}: no `{}` line after it", stops.join("` or `")))
}

/// The path of a `file` or `create` line.
fn path_word(path: &str, n: usize) -> Result<String, String> {
    if plain_relative(path) {
        Ok(path.to_owned())
    } else {
        Err(format!("line {n}: `{path}` is not a relative path without `.` or `..`"))
    }
}

/// Every entry's path lies under one of `roots`; otherwise an error naming
/// the entry's line.
pub fn check_roots(entries: &[Entry], roots: &[String]) -> Result<(), String> {
    for e in entries {
        let f = Path::new(e.file());
        if !roots.iter().any(|r| f.starts_with(r) && f != Path::new(r)) {
            return Err(format!(
                "line {}: `{}` is not under a root ({})",
                e.line(),
                e.file(),
                roots.join(", ")
            ));
        }
    }
    Ok(())
}

/// Applies `entries` in order to the tree `root` (the paths are relative
/// to it), and returns the files written, in the order of their first
/// entry. Each edited file is written **as a new file**: the old one (a
/// hard link to the pristine file) is removed first, never opened for
/// writing; its permission bits are kept.
///
/// Errors name the entry's line: a path through a symbolic link or a
/// non-directory, a `replace` of a file that does not exist (or is not a
/// regular file), a `create` of one that does, an old text not found, or
/// found more than once without `all`. Nothing is written when any entry
/// fails.
pub fn apply_edit(root: &Path, entries: &[Entry]) -> Result<Vec<PathBuf>, String> {
    // The new contents, in the order of their first entry.
    let mut order: Vec<PathBuf> = Vec::new();
    let mut files: BTreeMap<PathBuf, (Vec<u8>, Option<u32>)> = BTreeMap::new();
    for e in entries {
        let n = e.line();
        let path = root.join(e.file());
        no_link_on_the_way(root, e.file()).map_err(|why| format!("line {n}: {why}"))?;
        let shown = e.file();
        match e {
            Entry::Replace { all, old, new, .. } => {
                if !files.contains_key(&path) {
                    let m = std::fs::symlink_metadata(&path)
                        .map_err(|_| format!("line {n}: `{shown}` does not exist"))?;
                    if !m.is_file() {
                        return Err(format!("line {n}: `{shown}` is not a regular file"));
                    }
                    let bytes = std::fs::read(&path)
                        .map_err(|err| format!("line {n}: `{shown}`: cannot read: {err}"))?;
                    files.insert(path.clone(), (bytes, Some(m.permissions().mode())));
                    order.push(path.clone());
                }
                let (content, _) = files
                    .get_mut(&path)
                    .unwrap_or_else(|| unreachable!("the file was inserted above"));
                let at = occurrences(content, old);
                match (at.len(), all) {
                    (0, _) => {
                        return Err(format!("line {n}: the old text is not found in `{shown}`"));
                    }
                    (1, _) | (_, true) => *content = replaced(content, &at, old.len(), new),
                    (k, false) => {
                        return Err(format!(
                            "line {n}: the old text is found {k} times in `{shown}` (`replace all` \
                             replaces every one)"
                        ));
                    }
                }
            }
            Entry::Create { content, .. } => {
                if files.contains_key(&path) || std::fs::symlink_metadata(&path).is_ok() {
                    return Err(format!("line {n}: `{shown}` exists; `create` makes a new file"));
                }
                files.insert(path.clone(), (content.clone(), None));
                order.push(path.clone());
            }
        }
    }
    for path in &order {
        let (content, mode) = &files[path];
        if mode.is_some() {
            std::fs::remove_file(path)
                .map_err(|e| format!("{}: cannot remove: {e}", path.display()))?;
        } else if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| format!("{}: cannot create: {e}", dir.display()))?;
        }
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|e| format!("{}: cannot create: {e}", path.display()))?;
        f.write_all(content).map_err(|e| format!("{}: cannot write: {e}", path.display()))?;
        if let Some(mode) = mode {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(*mode))
                .map_err(|e| format!("{}: cannot set its mode: {e}", path.display()))?;
        }
    }
    Ok(order)
}

/// Every directory on the way from `root` to `rel` that exists is a real
/// directory, not a symbolic link (which could lead out of the tree).
fn no_link_on_the_way(root: &Path, rel: &str) -> Result<(), String> {
    let mut at = root.to_path_buf();
    let parts: Vec<&str> = rel.split('/').collect();
    for part in &parts[..parts.len() - 1] {
        at.push(part);
        match std::fs::symlink_metadata(&at) {
            Ok(m) if m.file_type().is_symlink() => {
                return Err(format!("`{rel}` goes through the symbolic link `{part}`"));
            }
            Ok(m) if !m.is_dir() => {
                return Err(format!("`{rel}` goes through `{part}`, which is not a directory"));
            }
            Ok(_) => {}
            Err(_) => return Ok(()),
        }
    }
    Ok(())
}

/// The starts of the non-overlapping occurrences of `needle`, left to right.
fn occurrences(hay: &[u8], needle: &[u8]) -> Vec<usize> {
    let mut out = Vec::new();
    let mut i = 0;
    while i + needle.len() <= hay.len() {
        if &hay[i..i + needle.len()] == needle {
            out.push(i);
            i += needle.len();
        } else {
            i += 1;
        }
    }
    out
}

fn replaced(hay: &[u8], at: &[usize], len: usize, new: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(hay.len());
    let mut last = 0;
    for &i in at {
        out.extend_from_slice(&hay[last..i]);
        out.extend_from_slice(new);
        last = i + len;
    }
    out.extend_from_slice(&hay[last..]);
    out
}

/// Makes `to` (which must not exist) a copy of the directory `from` in
/// which every regular file is a hard link to `from`'s (a copy where
/// linking fails), and every symbolic link a symbolic link with the same
/// target. Returns the number of files linked or copied. Any other kind
/// of file is an error.
pub fn link_tree(from: &Path, to: &Path) -> Result<u64, String> {
    let m = std::fs::symlink_metadata(from)
        .map_err(|e| format!("{}: cannot read: {e}", from.display()))?;
    if !m.is_dir() {
        return Err(format!("{} is not a directory", from.display()));
    }
    if std::fs::symlink_metadata(to).is_ok() {
        return Err(format!("{} exists", to.display()));
    }
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("{}: cannot create: {e}", parent.display()))?;
    }
    let mut n = 0;
    link_dir(from, to, &mut n)?;
    Ok(n)
}

fn link_dir(from: &Path, to: &Path, n: &mut u64) -> Result<(), String> {
    std::fs::create_dir(to).map_err(|e| format!("{}: cannot create: {e}", to.display()))?;
    let entries =
        std::fs::read_dir(from).map_err(|e| format!("{}: cannot read: {e}", from.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("{}: cannot read: {e}", from.display()))?;
        let (src, dst) = (entry.path(), to.join(entry.file_name()));
        let kind = entry.file_type().map_err(|e| format!("{}: {e}", src.display()))?;
        if kind.is_dir() {
            link_dir(&src, &dst, n)?;
        } else if kind.is_symlink() {
            let target = std::fs::read_link(&src).map_err(|e| format!("{}: {e}", src.display()))?;
            std::os::unix::fs::symlink(&target, &dst)
                .map_err(|e| format!("{}: cannot create: {e}", dst.display()))?;
            *n += 1;
        } else if kind.is_file() {
            if std::fs::hard_link(&src, &dst).is_err() {
                std::fs::copy(&src, &dst)
                    .map_err(|e| format!("{}: cannot copy: {e}", src.display()))?;
            }
            *n += 1;
        } else {
            return Err(format!("{}: not a file, directory or symbolic link", src.display()));
        }
    }
    Ok(())
}

/// `expect.txt`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expect {
    Verified,
    Mismatch(Vec<Wanted>),
}

/// A finding line of `expect.txt`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wanted {
    /// `changed`, `removed`, `added` or `derived_changed`.
    pub kind: String,
    pub name: String,
    /// A file relative to `src/`, and a line.
    pub at: Option<(String, u64)>,
}

impl Wanted {
    /// The line as `expect.txt` writes it, without its indentation.
    pub fn text(&self) -> String {
        match &self.at {
            Some((f, l)) => format!("{} {} @ {f}:{l}", self.kind, self.name),
            None => format!("{} {}", self.kind, self.name),
        }
    }

    /// Whether the line's image is the post-image (§10.1: `added`).
    pub fn in_post(&self) -> bool {
        self.kind == "added"
    }
}

/// The kinds a finding line may have.
pub const KINDS: [&str; 4] = ["changed", "removed", "added", "derived_changed"];

/// Reads `expect.txt` (§10.1): `verified` alone, or `mismatch` and at
/// least one finding line. Blank lines are ignored; the others are
/// trimmed. A finding line is `<kind> <name>` with an optional
/// ` @ <file>:<line>` (split at the last ` @ `); the file is a relative
/// path without `.` or `..`, the line a positive decimal number. Errors
/// name the line.
pub fn read_expect(text: &str) -> Result<Expect, String> {
    let mut lines = text.lines().enumerate().filter(|(_, l)| !l.trim().is_empty());
    let Some((_, first)) = lines.next() else {
        return Err("expect.txt is empty".to_owned());
    };
    match first.trim() {
        "verified" => match lines.next() {
            None => Ok(Expect::Verified),
            Some((i, _)) => Err(format!("line {}: `verified` takes no finding line", i + 1)),
        },
        "mismatch" => {
            let mut wanted = Vec::new();
            for (i, l) in lines {
                wanted.push(read_wanted(l.trim()).map_err(|e| format!("line {}: {e}", i + 1))?);
            }
            if wanted.is_empty() {
                return Err("`mismatch` needs at least one finding line".to_owned());
            }
            Ok(Expect::Mismatch(wanted))
        }
        other => Err(format!("the first line is `{other}`, not `verified` or `mismatch`")),
    }
}

fn read_wanted(line: &str) -> Result<Wanted, String> {
    let (kind, rest) = line.split_once(' ').ok_or_else(|| format!("`{line}` has no name"))?;
    if !KINDS.contains(&kind) {
        return Err(format!("`{kind}` is not a kind ({})", KINDS.join(", ")));
    }
    let (name, at) = match rest.rsplit_once(" @ ") {
        Some((name, place)) => {
            let (file, l) = place
                .rsplit_once(':')
                .ok_or_else(|| format!("the place `{place}` is not `<file>:<line>`"))?;
            let line_no = (!l.is_empty() && l.bytes().all(|b| b.is_ascii_digit()))
                .then(|| l.parse::<u64>().ok())
                .flatten()
                .filter(|n| *n > 0)
                .ok_or_else(|| format!("the line `{l}` of `{place}` is not a positive number"))?;
            if !plain_relative(file) {
                return Err(format!("`{file}` is not a relative path without `.` or `..`"));
            }
            (name.trim(), Some((file.to_owned(), line_no)))
        }
        None => (rest.trim(), None),
    };
    if name.is_empty() {
        return Err(format!("`{line}` has no name"));
    }
    Ok(Wanted { kind: kind.to_owned(), name: name.to_owned(), at })
}

/// The names `slip units` printed with the indeterminate flag
/// (`spec/exec.md` §8.9): lines `<key> <kind> <class> <flags> <name>`
/// whose flags hold `i`, the name with the `\xNN` escapes of
/// `spec/verify.md` §7.1 undone (kept escaped if the bytes are not UTF-8).
/// Lines of another shape are ignored.
pub fn indeterminate_units(stdout: &str) -> BTreeSet<String> {
    stdout
        .lines()
        .filter_map(|l| {
            let mut parts = l.splitn(5, ' ');
            let (_key, _kind, _class) = (parts.next()?, parts.next()?, parts.next()?);
            let flags = parts.next()?;
            let name = parts.next()?;
            flags.contains('i').then(|| unescape(name))
        })
        .collect()
}

/// Undoes `\xNN`; a string whose bytes are then not UTF-8 is kept as it is.
fn unescape(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\'
            && i + 3 < b.len()
            && b[i + 1] == b'x'
            && let Some(v) = std::str::from_utf8(&b[i + 2..i + 4])
                .ok()
                .and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(v);
            i += 4;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_owned())
}

// ---------------------------------------------------------------------------
// §10.3 The result of a case

/// What became of one finding line (§10.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Line {
    Found,
    NoPlace,
    WrongPlace { file: String, line: u64 },
    Blocked,
    Missing,
}

/// The result of a case (§10.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaseResult {
    Verified,
    Clean,
    Different,
    Found,
    Partly,
    Missed,
    Error(String),
}

impl CaseResult {
    /// The word of the table.
    pub fn word(&self) -> &'static str {
        match self {
            CaseResult::Verified => "verified",
            CaseResult::Clean => "clean",
            CaseResult::Different => "DIFFERENT",
            CaseResult::Found => "found",
            CaseResult::Partly => "partly",
            CaseResult::Missed => "MISSED",
            CaseResult::Error(_) => "ERROR",
        }
    }

    /// `verified` or `found`: the result of a case in a run that is `ok`.
    pub fn ok(&self) -> bool {
        matches!(self, CaseResult::Verified | CaseResult::Found)
    }

    /// `DIFFERENT`, `MISSED` or `ERROR`: the result of a case that fails
    /// the run.
    pub fn fails(&self) -> bool {
        matches!(self, CaseResult::Different | CaseResult::Missed | CaseResult::Error(_))
    }
}

/// The observer states that leave a case with no result (§10.3).
pub const NO_RESULT: [&str; 4] = ["not_parsed", "unverifiable", "never_parsed", "failed"];

/// The finding kinds that say Slip could not digest a unit.
pub const INDETERMINATE: [&str; 2] = ["indeterminate", "derived_indeterminate"];

/// A report's numbers and groups, as [`check`] reads them.
struct Read<'a> {
    verdict: &'a str,
    counts: BTreeMap<&'static str, u64>,
    groups: &'a [Value],
}

const COUNTS: [&str; 9] = [
    "selected",
    "verified",
    "unchanged",
    "mismatch",
    "not_parsed",
    "unverifiable",
    "never_parsed",
    "failed",
    "indeterminate_only",
];

fn read_report(report: &Value) -> Result<Read<'_>, String> {
    let kind = report.get("report").and_then(Value::as_str);
    if kind != Some(REPORT) {
        return Err(format!("the report is not `{REPORT}`"));
    }
    let verdict = report
        .get("verdict")
        .and_then(Value::as_str)
        .filter(|v| matches!(*v, "verified" | "not_verified" | "incomplete"))
        .ok_or_else(|| "the report has no verdict".to_owned())?;
    let obs = report.get("observers").ok_or_else(|| "the report has no `observers`".to_owned())?;
    let mut counts = BTreeMap::new();
    for c in COUNTS {
        let n = obs
            .get(c)
            .and_then(Value::as_u64)
            .ok_or_else(|| format!("the report's `observers.{c}` is not a number"))?;
        counts.insert(c, n);
    }
    let groups = report
        .get("findings")
        .and_then(|f| f.get("groups"))
        .and_then(Value::as_array)
        .ok_or_else(|| "the report's `findings.groups` is not an array".to_owned())?;
    for (i, g) in groups.iter().enumerate() {
        if g.get("kind").and_then(Value::as_str).is_none()
            || g.get("name").and_then(Value::as_str).is_none()
        {
            return Err(format!("the report's `findings.groups[{i}]` has no kind or name"));
        }
    }
    Ok(Read { verdict, counts, groups })
}

/// A group's `kind` and `name`.
fn kind_name(g: &Value) -> (&str, &str) {
    (
        g.get("kind").and_then(Value::as_str).unwrap_or(""),
        g.get("name").and_then(Value::as_str).unwrap_or(""),
    )
}

/// The groups of `report` whose kind is not indeterminate.
pub fn determinate_groups(report: &Value) -> Vec<&Value> {
    report
        .get("findings")
        .and_then(|f| f.get("groups"))
        .and_then(Value::as_array)
        .unwrap_or_default()
        .iter()
        .filter(|g| !INDETERMINATE.contains(&kind_name(g).0))
        .collect()
}

/// A site of a group's `where`: its file relative to `src` (as it is if it
/// is not under `src`), its line, column, node and what; `None` for no
/// `where`, or for the empty site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Site {
    pub file: String,
    pub line: u64,
    pub column: u64,
    pub node: String,
    pub what: String,
}

/// The site `side` (`pre` or `post`) of the group `g`.
pub fn site(g: &Value, side: &str, src: &Path) -> Option<Site> {
    let s = g.get("where")?.get(side)?;
    let file = s.get("file")?;
    let file = match file {
        Value::Str(f) => relative(f, src),
        other => json::write_compact(other).unwrap_or_default(),
    };
    let line = s.get("line")?.as_u64()?;
    let column = s.get("column").and_then(Value::as_u64).unwrap_or(0);
    if file.is_empty() && line == 0 {
        return None;
    }
    Some(Site {
        file,
        line,
        column,
        node: s.get("node").and_then(Value::as_str).unwrap_or("").to_owned(),
        what: s.get("what").and_then(Value::as_str).unwrap_or("").to_owned(),
    })
}

/// `path` relative to `src` when it is under it, else as it is.
fn relative(path: &str, src: &Path) -> String {
    Path::new(path)
        .strip_prefix(src)
        .ok()
        .and_then(|p| p.to_str())
        .filter(|p| !p.is_empty())
        .map_or_else(|| path.to_owned(), str::to_owned)
}

/// What became of the finding line `w` in the groups of a report (§10.1).
/// `pre` and `post` are the names `slip units` printed with the
/// indeterminate flag in each image.
fn line_of(
    w: &Wanted,
    groups: &[Value],
    src: &Path,
    pre: &BTreeSet<String>,
    post: &BTreeSet<String>,
) -> Line {
    let matching: Vec<&Value> =
        groups.iter().filter(|g| kind_name(g) == (w.kind.as_str(), w.name.as_str())).collect();
    if matching.is_empty() {
        let image = if w.in_post() { post } else { pre };
        return if image.contains(&w.name) { Line::Blocked } else { Line::Missing };
    }
    let Some((file, line)) = &w.at else { return Line::Found };
    let side = if w.kind == "removed" { "pre" } else { "post" };
    let sites: Vec<Option<Site>> = matching.iter().map(|g| site(g, side, src)).collect();
    if sites.iter().flatten().any(|s| s.file == *file && s.line == *line) {
        Line::Found
    } else if sites.iter().any(Option::is_none) {
        Line::NoPlace
    } else {
        let s = sites.into_iter().flatten().next().unwrap_or_else(|| {
            unreachable!("there is a matching group and none is without a site")
        });
        Line::WrongPlace { file: s.file, line: s.line }
    }
}

/// §10.3 "The result of a case", from the case's `expect` and the
/// `report` of its verify run (`spec/verify.md` §7.1). `src` is the
/// corpus's `src/`, against which the places of the report are read;
/// `pre` and `post` are [`indeterminate_units`] of the two images (empty
/// when `slip units` was not run). The lines are one per finding line of a
/// `mismatch`, in order; none for `verified` or an error.
pub fn check(
    expect: &Expect,
    report: &Value,
    src: &Path,
    pre: &BTreeSet<String>,
    post: &BTreeSet<String>,
) -> (CaseResult, Vec<Line>) {
    let r = match read_report(report) {
        Ok(r) => r,
        Err(e) => return (CaseResult::Error(e), Vec::new()),
    };
    let broken: Vec<String> = NO_RESULT
        .iter()
        .filter(|s| r.counts[*s] > 0)
        .map(|s| format!("{} observer(s) {s}", r.counts[s]))
        .collect();
    if !broken.is_empty() {
        return (CaseResult::Error(broken.join(", ")), Vec::new());
    }
    if r.counts["selected"] == 0 {
        return (CaseResult::Error("no observer selected".to_owned()), Vec::new());
    }
    let other_findings = r.groups.iter().any(|g| !INDETERMINATE.contains(&kind_name(g).0));
    match expect {
        Expect::Verified => {
            let result = if r.verdict == "verified" {
                CaseResult::Verified
            } else if r.counts["mismatch"] == r.counts["indeterminate_only"] && !other_findings {
                CaseResult::Clean
            } else {
                CaseResult::Different
            };
            (result, Vec::new())
        }
        Expect::Mismatch(wanted) => {
            let lines: Vec<Line> =
                wanted.iter().map(|w| line_of(w, r.groups, src, pre, post)).collect();
            let missed = lines.iter().any(|l| matches!(l, Line::Missing | Line::WrongPlace { .. }));
            let result = if missed || r.verdict == "verified" {
                CaseResult::Missed
            } else if lines.iter().all(|l| *l == Line::Found) {
                if r.verdict == "not_verified" {
                    CaseResult::Found
                } else {
                    CaseResult::Error(format!(
                        "every line is found, and the verdict is {}",
                        r.verdict
                    ))
                }
            } else {
                CaseResult::Partly
            };
            (result, lines)
        }
    }
}

/// The cause of a blocked line: the `what` of the report's indeterminate
/// group of that name, on the line's image (or the other, if that one is
/// empty).
pub fn blocked_cause(w: &Wanted, report: &Value, src: &Path) -> Option<String> {
    let groups = report.get("findings")?.get("groups")?.as_array()?;
    let (first, second) = if w.in_post() { ("post", "pre") } else { ("pre", "post") };
    groups
        .iter()
        .filter(|g| {
            let (k, n) = kind_name(g);
            INDETERMINATE.contains(&k) && n == w.name
        })
        .find_map(|g| {
            [first, second]
                .iter()
                .find_map(|s| site(g, s, src).map(|s| s.what).filter(|w| !w.is_empty()))
        })
}

// ---------------------------------------------------------------------------
// The command

/// A case of `bench/demo1/cases/`, read.
#[derive(Debug, Clone)]
pub struct Case {
    pub name: String,
    /// Its position among every case, in the order of their names: the
    /// round of its verify run.
    pub round: u64,
    pub edit: Vec<Entry>,
    pub base: Option<String>,
    /// `declared.txt`, if it has a line that is not blank or a comment.
    pub declared: Option<PathBuf>,
    /// The `--only` substrings of `observers` (empty: the default).
    pub observers: Vec<String>,
    pub expect: Expect,
}

/// Reads every case under `dir` (`bench/demo1/cases`), in the order of
/// their names. Errors name the case and the file.
pub fn read_cases(dir: &Path, roots: &[String]) -> Result<Vec<Case>, String> {
    let shown = dir.display();
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map_err(|e| format!("{shown}: cannot read: {e}"))?
        .map(|e| {
            let e = e.map_err(|e| format!("{shown}: cannot read: {e}"))?;
            let is_dir = e.file_type().map_err(|e| format!("{shown}: {e}"))?.is_dir();
            Ok(is_dir.then(|| e.file_name().to_string_lossy().into_owned()))
        })
        .collect::<Result<Vec<Option<String>>, String>>()?
        .into_iter()
        .flatten()
        .collect();
    names.sort();
    let mut cases: Vec<Case> = Vec::new();
    for (round, name) in names.iter().enumerate() {
        let d = dir.join(name);
        let at = |f: &str| format!("{name}/{f}");
        let read = |f: &str| -> Result<Option<Vec<u8>>, String> {
            match std::fs::read(d.join(f)) {
                Ok(b) => Ok(Some(b)),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(format!("{}: cannot read: {e}", at(f))),
            }
        };
        let utf8 =
            |f: &str, b: Vec<u8>| String::from_utf8(b).map_err(|_| format!("{}: not UTF-8", at(f)));
        let edit = match read("edit.txt")? {
            Some(b) => {
                let e = read_edit(&b).map_err(|e| format!("{}: {e}", at("edit.txt")))?;
                check_roots(&e, roots).map_err(|e| format!("{}: {e}", at("edit.txt")))?;
                e
            }
            None => Vec::new(),
        };
        let base = match read("base")? {
            Some(b) => {
                let b = utf8("base", b)?.trim().to_owned();
                if !cases.iter().any(|c| c.name == b) {
                    return Err(format!("{}: `{b}` is not a case before this one", at("base")));
                }
                Some(b)
            }
            None => None,
        };
        let declared = match read("declared.txt")? {
            Some(b) => {
                let t = utf8("declared.txt", b)?;
                t.lines()
                    .any(|l| {
                        let l = l.trim_start_matches('\u{feff}').trim();
                        !l.is_empty() && !l.starts_with('#')
                    })
                    .then(|| d.join("declared.txt"))
            }
            None => None,
        };
        let observers = match read("observers")? {
            Some(b) => utf8("observers", b)?
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_owned)
                .collect(),
            None => Vec::new(),
        };
        let expect = match read("expect.txt")? {
            Some(b) => read_expect(&utf8("expect.txt", b)?)
                .map_err(|e| format!("{}: {e}", at("expect.txt")))?,
            None => return Err(format!("{}: missing", at("expect.txt"))),
        };
        cases.push(Case {
            name: name.clone(),
            round: round as u64,
            edit,
            base,
            declared,
            observers,
            expect,
        });
    }
    Ok(cases)
}

/// The options of §10 after `1`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Options {
    pub slip: Option<PathBuf>,
    pub jobs: Option<String>,
    pub cases: Vec<String>,
    pub keep: bool,
}

/// The options; `Err` is a usage error.
pub fn parse_args(args: &[String]) -> Result<Options, String> {
    let rest = match args {
        [n, rest @ ..] if n == "1" => rest,
        [n, ..] => return Err(format!("there is no demo `{n}` (only 1)")),
        [] => return Err("missing the demo's number".to_owned()),
    };
    let mut o = Options::default();
    let mut it = rest.iter();
    while let Some(arg) = it.next() {
        let mut value = || it.next().cloned().ok_or_else(|| format!("`{arg}` needs a value"));
        match arg.as_str() {
            "--slip" if o.slip.is_none() => o.slip = Some(PathBuf::from(value()?)),
            "-j" if o.jobs.is_none() => o.jobs = Some(value()?),
            "--case" => o.cases.push(value()?),
            "--keep" if !o.keep => o.keep = true,
            "--slip" | "-j" | "--keep" => return Err(format!("`{arg}` is given twice")),
            other => return Err(format!("unknown argument `{other}`")),
        }
    }
    Ok(o)
}

/// How the demo ended (§10.3): exit status 0, 4 or 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Ok,
    Partial,
    Failed,
}

/// `cargo xtask demo …` over the repository `repo`.
pub fn main(repo: &Path, args: &[String]) -> ExitCode {
    let env = Env {
        corpus_dir: std::env::var_os("SLIP_CORPUS_DIR"),
        target_dir: std::env::var_os("CARGO_TARGET_DIR"),
    };
    let mut stdout = std::io::stdout();
    match run(repo, args, &env, &mut stdout) {
        Ok(Outcome::Ok) => ExitCode::SUCCESS,
        Ok(Outcome::Partial) => ExitCode::from(4),
        Ok(Outcome::Failed) => ExitCode::from(1),
        Err(Failure::Usage(e)) => {
            eprintln!("xtask demo: {e}\n{USAGE}");
            ExitCode::from(2)
        }
        Err(Failure::Error(e)) => {
            eprintln!("xtask demo: {e}");
            ExitCode::from(1)
        }
    }
}

/// Runs `args` (after `demo`), writing the table to `out` as it goes.
pub fn run(
    repo: &Path,
    args: &[String],
    env: &Env,
    out: &mut dyn Write,
) -> Result<Outcome, Failure> {
    let opts = parse_args(args).map_err(Failure::Usage)?;
    let e = Failure::Error;
    let inputs = repo.join(INPUTS);
    let toml = std::fs::read_to_string(inputs.join("demo.toml"))
        .map_err(|err| e(format!("{INPUTS}/demo.toml: cannot read: {err}")))?;
    let config = read_config(&toml).map_err(|err| e(format!("{INPUTS}/demo.toml: {err}")))?;
    let cases = read_cases(&inputs.join("cases"), &config.roots)
        .map_err(|err| e(format!("{INPUTS}/cases/{err}")))?;
    if let Some(unknown) = opts.cases.iter().find(|n| !cases.iter().any(|c| &c.name == *n)) {
        return Err(Failure::Usage(format!("there is no case `{unknown}`")));
    }
    let corpora = rows::load(repo).map_err(e)?;
    let c = rows::find(&corpora, &config.corpus).map_err(e)?;
    let root = corpus::corpus_root(repo, env.corpus_dir.as_deref()).map_err(e)?;
    let layout = Layout::new(&root, &c.name);
    configured(c, &layout).map_err(e)?;
    let slip =
        slip_run::find_slip(repo, opts.slip.as_deref(), env.target_dir.as_deref()).map_err(e)?;
    let ctx = Ctx { slip: &slip, layout: &layout, config: &config, opts: &opts, corpus: c };
    ctx.demo(&cases, out).map_err(e)
}

/// The corpus is configured, as `invariants` requires it (§9).
fn configured(c: &Corpus, layout: &Layout) -> Result<(), String> {
    let s = corpus::status(c, layout);
    if s.configured && s.generated && matches!(s.database, Some(Ok(_))) {
        Ok(())
    } else {
        Err(format!(
            "corpus `{}` is not configured: run `cargo xtask corpus configure {}`",
            c.name, c.name
        ))
    }
}

struct Ctx<'a> {
    slip: &'a Path,
    layout: &'a Layout,
    config: &'a Config,
    opts: &'a Options,
    corpus: &'a Corpus,
}

/// A verify run's numbers (§10.3).
#[derive(Debug, Clone, Default)]
struct Numbers {
    units: u64,
    indeterminate: u64,
    reparsed: u64,
    wall_ns: u64,
    cpu_ns: u64,
    peak_rss_max: u64,
}

/// What one case gave.
struct Done {
    result: CaseResult,
    lines: Vec<Line>,
    report: Option<Value>,
    numbers: Option<Numbers>,
}

fn emit(out: &mut dyn Write, text: &str) -> Result<(), String> {
    out.write_all(text.as_bytes())
        .and_then(|()| out.flush())
        .map_err(|e| format!("cannot write the output: {e}"))
}

impl Ctx<'_> {
    fn demo1(&self) -> PathBuf {
        self.layout.dir.join("demo1")
    }

    fn store(&self) -> PathBuf {
        self.demo1().join("store")
    }

    fn tree(&self, case: &str) -> PathBuf {
        self.demo1().join("trees").join(case)
    }

    /// Runs `slip` with `args`; a status not in `accept` is an error.
    /// Runs `slip`, its standard error appended to `demo1/<log>` (kept out
    /// of the table: §10.3's output is the table alone).
    fn slip(&self, args: &[OsString], accept: &[i32], log: &str) -> Result<Ran, String> {
        let what = args.first().map(|a| a.to_string_lossy().into_owned()).unwrap_or_default();
        let path = self.demo1().join(log);
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| format!("{}: cannot open: {e}", path.display()))?;
        let ran = slip_run::run_to(self.slip, args, file.into())?;
        if accept.contains(&ran.status) {
            Ok(ran)
        } else {
            Err(format!(
                "`slip {what}` exited with status {} (its standard error: demo1/{log})",
                ran.status
            ))
        }
    }

    fn only_args(&self) -> Vec<OsString> {
        self.config.only.iter().flat_map(|o| ["--only".into(), o.into()]).collect()
    }

    fn jobs_args(&self) -> Vec<OsString> {
        self.opts.jobs.iter().flat_map(|j| ["-j".into(), j.into()]).collect()
    }

    /// The case's observers: its `observers` file, or the first selected.
    fn observer_args(&self, case: &Case) -> Vec<OsString> {
        if case.observers.is_empty() {
            let mut v = self.only_args();
            v.extend(["--limit".into(), "1".into()]);
            v
        } else {
            case.observers.iter().flat_map(|o| ["--only".into(), o.into()]).collect()
        }
    }

    /// `--root <src>/<root> --post <tree>/<root>` for every root.
    fn image_args(&self, case: &str) -> Vec<OsString> {
        let mut v = Vec::new();
        for r in &self.config.roots {
            v.extend([
                "--root".into(),
                self.layout.src.join(r).into(),
                "--post".into(),
                self.tree(case).join(r).into(),
            ]);
        }
        v
    }

    fn demo(&self, all: &[Case], out: &mut dyn Write) -> Result<Outcome, String> {
        let selected: Vec<&Case> = all
            .iter()
            .filter(|c| self.opts.cases.is_empty() || self.opts.cases.contains(&c.name))
            .collect();
        // The cases whose trees are made: the selected ones and their bases.
        let mut needed: BTreeSet<&str> = BTreeSet::new();
        for c in &selected {
            let mut at = Some(*c);
            while let Some(c) = at {
                needed.insert(&c.name);
                at = c.base.as_ref().and_then(|b| all.iter().find(|x| &x.name == b));
            }
        }
        let made: Vec<&Case> = all.iter().filter(|c| needed.contains(c.name.as_str())).collect();
        let pristine = self.pristine_hashes(&made)?;
        let demo1 = self.demo1();
        slip_run::remove_tree(&self.layout.dir, &demo1)?;
        std::fs::create_dir_all(&demo1)
            .map_err(|e| format!("{}: cannot create: {e}", demo1.display()))?;
        let (version_slip, version_kiln) = self.versions()?;

        let map = self.map_run()?;
        let width = selected.iter().map(|c| c.name.len()).max().unwrap_or(0).max(10) + 2;
        emit(out, &map.head(self.corpus, width))?;

        // Step 3: the cases.
        let mut done: Vec<(&Case, Done)> = Vec::new();
        let mut trees_made: BTreeSet<String> = BTreeSet::new();
        for case in &made {
            let tree = self.make_tree(case, &trees_made);
            if let Err(err) = &tree {
                if !selected.iter().any(|c| c.name == case.name) {
                    return Err(format!("{}: the tree: {err}", case.name));
                }
            } else {
                trees_made.insert(case.name.clone());
            }
            if !selected.iter().any(|c| c.name == case.name) {
                continue;
            }
            let result = match tree {
                Err(err) => Done {
                    result: CaseResult::Error(format!("the tree: {err}")),
                    lines: Vec::new(),
                    report: None,
                    numbers: None,
                },
                Ok(()) => self.run_case(case, &map.plan),
            };
            emit(out, &self.render_case(case, &result, width))?;
            done.push((case, result));
        }

        // Step 4: no edit went through a link.
        let through = self.check_pristine(&pristine)?;
        if !self.opts.keep {
            slip_run::remove_tree(&self.layout.dir, &demo1.join("trees"))?;
        }

        let mut tail = map.causes();
        let (outcome, last) = verdict(&done, &through);
        let _ = writeln!(tail, "demo 1: {last}");
        let json = self.json(&map, (&version_slip, &version_kiln), &done, &last)?;
        let path = demo1.join("demo1.json");
        std::fs::write(&path, json)
            .map_err(|e| format!("{}: cannot write: {e}", path.display()))?;
        emit(out, &tail)?;
        Ok(outcome)
    }

    /// Steps 1 and 2 of §10.3: the plan, the map run over the selection,
    /// and its numbers.
    fn map_run(&self) -> Result<MapRun, String> {
        let store = || -> [OsString; 2] { ["--store".into(), self.store().into()] };
        let mut args: Vec<OsString> =
            vec!["plan".into(), "--db".into(), self.layout.database().into()];
        args.extend(store());
        let ran = self.slip(&args, &[0, 1], MAP_LOG)?;
        let plan = slip_run::line(&ran.stdout, "plan")
            .and_then(Digest::from_hex)
            .ok_or_else(|| "`slip plan` printed no `plan <id>` line".to_owned())?;
        args = vec!["parse".into(), "--plan".into(), plan.to_string().into()];
        args.extend(store());
        args.extend(self.only_args());
        args.extend(self.jobs_args());
        let ran = self.slip(&args, &[0, 1], MAP_LOG)?;
        let map = load_printed(&ran, "parse", &plan)?;
        let counters = perf::shard_counters(&map)?;
        let determinate = map
            .shards
            .iter()
            .filter(|(id, shard)| {
                invariants::side(&map, id).status() == Some(Status::Ok)
                    && shard.as_ref().is_some_and(|sh| {
                        sh.counters.iter().any(|c| c.name == "digest.indeterminate" && c.value == 0)
                    })
            })
            .count() as u64;
        Ok(MapRun {
            plan,
            observers: map.shards.len() as u64,
            determinate,
            units: counters.get("digest.units").copied().unwrap_or(0),
            indeterminate: counters.get("digest.indeterminate").copied().unwrap_or(0),
            measured: perf::measured(&map.record)?,
            counters,
        })
    }

    /// `slip --version`: `slip <version> (kiln <version>)`.
    fn versions(&self) -> Result<(String, String), String> {
        let ran = self.slip(&["--version".into()], &[0], MAP_LOG)?;
        let line = ran.stdout.lines().next().unwrap_or("");
        line.strip_prefix("slip ")
            .and_then(|r| r.strip_suffix(')'))
            .and_then(|r| r.split_once(" (kiln "))
            .map(|(s, k)| (s.to_owned(), k.to_owned()))
            .ok_or_else(|| format!("`slip --version` printed `{line}`, not `slip <v> (kiln <v>)`"))
    }

    /// The hash of every pristine file an edit of `cases` touches (`None`:
    /// the file does not exist, which a `create` requires).
    fn pristine_hashes(&self, cases: &[&Case]) -> Result<BTreeMap<String, Option<Digest>>, String> {
        let mut out = BTreeMap::new();
        for e in cases.iter().flat_map(|c| &c.edit) {
            if !out.contains_key(e.file()) {
                out.insert(e.file().to_owned(), self.hash_pristine(e.file())?);
            }
        }
        Ok(out)
    }

    fn hash_pristine(&self, rel: &str) -> Result<Option<Digest>, String> {
        let p = self.layout.src.join(rel);
        match std::fs::read(&p) {
            Ok(b) => Ok(Some(slip_model::hash::content_digest(&b))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("{}: cannot read: {e}", p.display())),
        }
    }

    /// The files of `before` whose hash changed: an edit went through a
    /// link (§10.3 step 4).
    fn check_pristine(
        &self,
        before: &BTreeMap<String, Option<Digest>>,
    ) -> Result<Vec<String>, String> {
        let mut changed = Vec::new();
        for (rel, h) in before {
            if self.hash_pristine(rel)? != *h {
                changed.push(rel.clone());
            }
        }
        Ok(changed)
    }

    /// `demo1/trees/<case>/<root>` for every root, from the pristine root
    /// or the base's tree, then the edit.
    fn make_tree(&self, case: &Case, made: &BTreeSet<String>) -> Result<(), String> {
        let tree = self.tree(&case.name);
        for r in &self.config.roots {
            let from = match &case.base {
                Some(b) if made.contains(b) => self.tree(b).join(r),
                Some(b) => return Err(format!("the tree of its base `{b}` was not made")),
                None => self.layout.src.join(r),
            };
            link_tree(&from, &tree.join(r))?;
        }
        apply_edit(&tree, &case.edit)?;
        Ok(())
    }

    /// `<cmd> --plan <id> --store <store>`.
    fn plan_args(&self, cmd: &str, plan: &Digest) -> Vec<OsString> {
        vec![
            cmd.into(),
            "--plan".into(),
            plan.to_string().into(),
            "--store".into(),
            self.store().into(),
        ]
    }

    /// The `slip` runs of one case (§10.3 step 3), after its tree.
    fn run_case(&self, case: &Case, plan: &Digest) -> Done {
        let mut report = None;
        let mut numbers = None;
        let result = (|| -> Result<(CaseResult, Vec<Line>), String> {
            let decl = self.demo1().join(format!("{}.decl", case.name));
            if let Some(from) = &case.declared {
                let mut a = self.plan_args("declare", plan);
                a.extend(["--from".into(), from.into(), "--out".into(), decl.clone().into()]);
                for r in &self.config.roots {
                    a.extend(["--owned".into(), self.layout.src.join(r).into()]);
                }
                a.extend(self.image_args(&case.name));
                a.extend(self.observer_args(case));
                self.slip(&a, &[0], &case_log(case))?;
            }
            let (value, n) = self.verify(case, plan, case.declared.is_some().then_some(&decl))?;
            numbers = Some(n);
            let src = &self.layout.src;
            let none = BTreeSet::new();
            let (mut result, mut lines) = check(&case.expect, &value, src, &none, &none);
            if let Expect::Mismatch(wanted) = &case.expect {
                let unfound: BTreeSet<&str> = wanted
                    .iter()
                    .zip(&lines)
                    .filter(|(_, l)| matches!(l, Line::Blocked | Line::Missing))
                    .map(|(w, _)| w.name.as_str())
                    .collect();
                if !unfound.is_empty() {
                    let [pre, post] = self.unit_flags(case, plan, &unfound)?;
                    (result, lines) = check(&case.expect, &value, src, &pre, &post);
                }
            }
            report = Some(value);
            Ok((result, lines))
        })();
        match result {
            Ok((result, lines)) => Done { result, lines, report, numbers },
            Err(e) => Done { result: CaseResult::Error(e), lines: Vec::new(), report, numbers },
        }
    }

    /// `slip verify` of the case: its report, which must be of `plan` and
    /// the case's round, and the numbers of its run record.
    fn verify(
        &self,
        case: &Case,
        plan: &Digest,
        declared: Option<&PathBuf>,
    ) -> Result<(Value, Numbers), String> {
        let path = self.demo1().join(format!("{}.json", case.name));
        let mut a = self.plan_args("verify", plan);
        a.extend(["--round".into(), case.round.to_string().into()]);
        a.extend(self.image_args(&case.name));
        if let Some(d) = declared {
            a.extend(["--declared".into(), d.into()]);
        }
        a.extend([
            "--findings".into(),
            FINDINGS.into(),
            "--locate".into(),
            LOCATE.into(),
            "--report".into(),
            path.clone().into(),
        ]);
        a.extend(self.only_args());
        a.extend(self.jobs_args());
        let ran = self.slip(&a, &[0, 1, 4], &case_log(case))?;
        let shown = path.display();
        let bytes =
            std::fs::read(&path).map_err(|e| format!("{shown}: cannot read the report: {e}"))?;
        let value =
            json::parse(&bytes).map_err(|e| format!("{shown}: the report is not JSON: {e}"))?;
        let of_plan = value.get("plan").and_then(Value::as_str) == Some(&plan.to_string());
        if !of_plan || value.get("round").and_then(Value::as_u64) != Some(case.round) {
            return Err(format!("{shown}: the report is not of plan {plan}, round {}", case.round));
        }
        let run = load_printed(&ran, "verify", plan)?;
        let sums = perf::shard_counters(&run)?;
        let m = perf::measured(&run.record)?;
        let get = |k: &str| sums.get(k).copied().unwrap_or(0);
        let numbers = Numbers {
            units: get("digest.units"),
            indeterminate: get("digest.indeterminate"),
            reparsed: get("verify.reparsed"),
            wall_ns: m.wall_ns,
            cpu_ns: m.cpu_ns,
            peak_rss_max: m.peak_rss_max,
        };
        Ok((value, numbers))
    }

    /// `slip units` with the case's image and observers, without and with
    /// `--post-image`, kept to the `names` (`--match`): the names flagged
    /// indeterminate in the pre- and the post-image.
    fn unit_flags(
        &self,
        case: &Case,
        plan: &Digest,
        names: &BTreeSet<&str>,
    ) -> Result<[BTreeSet<String>; 2], String> {
        let mut flags = [BTreeSet::new(), BTreeSet::new()];
        for (i, post) in [false, true].into_iter().enumerate() {
            let mut a = self.plan_args("units", plan);
            if post {
                a.push("--post-image".into());
            }
            a.extend(self.image_args(&case.name));
            a.extend(self.observer_args(case));
            for n in names {
                a.extend(["--match".into(), (*n).into()]);
            }
            flags[i] = indeterminate_units(&self.slip(&a, &[0], &case_log(case))?.stdout);
        }
        Ok(flags)
    }

    /// The lines of the table for one case (§10.3).
    fn render_case(&self, case: &Case, done: &Done, width: usize) -> String {
        let src = &self.layout.src;
        let mut o = String::new();
        let word = done.result.word();
        match (&done.report, &done.numbers) {
            (Some(report), Some(numbers)) => {
                let count = |k: &str| {
                    report
                        .get("observers")
                        .and_then(|v| v.get(k))
                        .and_then(Value::as_u64)
                        .unwrap_or(0)
                };
                let (verified, only) = (count("verified"), count("indeterminate_only"));
                let other = count("selected").saturating_sub(verified + only);
                let _ = writeln!(
                    o,
                    "{:<width$}{word:<10}observers verified {verified} indeterminate_only {only} \
                     other {other}  units {} indeterminate {}",
                    case.name, numbers.units, numbers.indeterminate
                );
                let _ = writeln!(
                    o,
                    "{:<width$}findings {}  reparsed {}  wall_s {} cpu_s {} peak_rss_max {}",
                    "",
                    determinate_groups(report).len(),
                    numbers.reparsed,
                    seconds(numbers.wall_ns),
                    seconds(numbers.cpu_ns),
                    numbers.peak_rss_max
                );
            }
            _ => {
                let _ = writeln!(o, "{:<width$}{word}", case.name);
            }
        }
        if let CaseResult::Error(why) = &done.result {
            let _ = writeln!(o, "  error {}", text(why.as_bytes()));
        }
        if let (Expect::Mismatch(wanted), Some(report)) = (&case.expect, &done.report) {
            for (w, l) in wanted.iter().zip(&done.lines) {
                o.push_str(&render_line(w, l, report, src));
            }
        }
        if let (CaseResult::Different, Some(report)) = (&done.result, &done.report) {
            for g in determinate_groups(report).into_iter().take(LISTED) {
                let (kind, name) = kind_name(g);
                let n = g.get("observers").and_then(Value::as_u64).unwrap_or(0);
                let _ = writeln!(o, "  {kind} {} observers {n}", text(name.as_bytes()));
            }
        }
        o
    }

    /// `demo1/demo1.json`: the numbers of the table, the plan id, the
    /// tool versions, and each case's groups that are not indeterminate,
    /// with paths under `src/` written relative to it.
    fn json(
        &self,
        map: &MapRun,
        tools: (&str, &str),
        done: &[(&Case, Done)],
        last: &str,
    ) -> Result<String, String> {
        let s = |t: &str| Value::Str(t.to_owned());
        let counters = Value::Object(
            map.counters
                .iter()
                .filter(|(k, _)| k.starts_with("digest."))
                .map(|(k, v)| (k.clone(), int(*v)))
                .collect(),
        );
        let m = &map.measured;
        let doc = Value::Object(vec![
            ("demo".into(), int(1)),
            ("corpus".into(), s(&self.corpus.name)),
            ("pin".into(), s(&self.corpus.tag)),
            ("plan".into(), s(&map.plan.to_string())),
            ("tool_slip".into(), s(tools.0)),
            ("tool_kiln".into(), s(tools.1)),
            ("roots".into(), Value::Array(self.config.roots.iter().map(|r| s(r)).collect())),
            ("only".into(), Value::Array(self.config.only.iter().map(|r| s(r)).collect())),
            ("observers".into(), int(map.observers)),
            ("determinate".into(), int(map.determinate)),
            ("units".into(), int(map.units)),
            ("indeterminate".into(), int(map.indeterminate)),
            (
                "map".into(),
                Value::Object(vec![
                    ("wall_ns".into(), int(m.wall_ns)),
                    ("cpu_ns".into(), int(m.cpu_ns)),
                    ("peak_rss_max".into(), int(m.peak_rss_max)),
                    ("counters".into(), counters),
                ]),
            ),
            (
                "cases".into(),
                Value::Array(done.iter().map(|(c, d)| case_json(c, d, &self.layout.src)).collect()),
            ),
            ("result".into(), s(last)),
        ]);
        json::write_pretty(&doc).map_err(|e| format!("demo1.json: {e}"))
    }
}

fn int(n: u64) -> Value {
    Value::Int(i128::from(n))
}

/// One case of `demo1.json`.
fn case_json(case: &Case, d: &Done, src: &Path) -> Value {
    let s = |t: &str| Value::Str(t.to_owned());
    let mut lines = Vec::new();
    if let Expect::Mismatch(wanted) = &case.expect {
        for (w, l) in wanted.iter().zip(&d.lines) {
            let (word, at) = match l {
                Line::Found => ("found", Value::Null),
                Line::NoPlace => ("no-place", Value::Null),
                Line::Blocked => ("blocked", Value::Null),
                Line::Missing => ("missing", Value::Null),
                Line::WrongPlace { file, line } => ("wrong", s(&format!("{file}:{line}"))),
            };
            let why = match (l, &d.report) {
                (Line::Blocked, Some(r)) => {
                    blocked_cause(w, r, src).map_or(Value::Null, Value::Str)
                }
                _ => Value::Null,
            };
            lines.push(Value::Object(vec![
                ("line".into(), s(&w.text())),
                ("result".into(), s(word)),
                ("at".into(), at),
                ("cause".into(), why),
            ]));
        }
    }
    let report = d.report.as_ref();
    let count = |k: &str| {
        report
            .and_then(|r| r.get("observers"))
            .and_then(|o| o.get(k))
            .and_then(Value::as_u64)
            .map_or(Value::Null, int)
    };
    let numbers = d.numbers.as_ref().map_or(Value::Null, |n| {
        Value::Object(vec![
            ("units".into(), int(n.units)),
            ("indeterminate".into(), int(n.indeterminate)),
            ("reparsed".into(), int(n.reparsed)),
            ("wall_ns".into(), int(n.wall_ns)),
            ("cpu_ns".into(), int(n.cpu_ns)),
            ("peak_rss_max".into(), int(n.peak_rss_max)),
        ])
    });
    let groups = report.map_or(Value::Null, |r| {
        Value::Array(determinate_groups(r).into_iter().map(|g| relative_group(g, src)).collect())
    });
    let error = match &d.result {
        CaseResult::Error(e) => s(e),
        _ => Value::Null,
    };
    Value::Object(vec![
        ("name".into(), s(&case.name)),
        ("round".into(), int(case.round)),
        ("result".into(), s(d.result.word())),
        ("error".into(), error),
        ("verdict".into(), report.and_then(|r| r.get("verdict")).cloned().unwrap_or(Value::Null)),
        ("selected".into(), count("selected")),
        ("verified".into(), count("verified")),
        ("indeterminate_only".into(), count("indeterminate_only")),
        ("mismatch".into(), count("mismatch")),
        ("run".into(), numbers),
        ("lines".into(), Value::Array(lines)),
        ("groups".into(), groups),
    ])
}

/// The map run of §10.3 step 2.
struct MapRun {
    plan: Digest,
    /// `s`: the selected observers.
    observers: u64,
    /// `d`: those whose map shard is `ok` with `digest.indeterminate` 0.
    determinate: u64,
    /// `digest.units` and `digest.indeterminate` summed.
    units: u64,
    indeterminate: u64,
    measured: perf::Measured,
    counters: BTreeMap<String, u64>,
}

impl MapRun {
    /// The first two lines of the table.
    fn head(&self, c: &Corpus, width: usize) -> String {
        let mut o = String::new();
        let _ = writeln!(
            o,
            "demo 1: {} {}, {} observers, {} determinate, units {} indeterminate {}",
            c.name, c.tag, self.observers, self.determinate, self.units, self.indeterminate
        );
        let m = &self.measured;
        let _ = writeln!(
            o,
            "{:<width$}wall_s {} cpu_s {} peak_rss_max {}",
            "map",
            seconds(m.wall_ns),
            seconds(m.cpu_ns),
            m.peak_rss_max
        );
        o
    }

    /// The `indeterminate` lines: when a unit is indeterminate, every
    /// `digest.indeterminate.*` counter, then the ten largest
    /// `digest.cause.*` (by value, then name).
    fn causes(&self) -> String {
        let mut o = String::new();
        if self.indeterminate == 0 {
            return o;
        }
        let mut lines: Vec<(&String, &u64)> =
            self.counters.iter().filter(|(k, _)| k.starts_with("digest.indeterminate.")).collect();
        let mut largest: Vec<(&String, &u64)> =
            self.counters.iter().filter(|(k, _)| k.starts_with("digest.cause.")).collect();
        largest.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        largest.truncate(LISTED);
        lines.extend(largest);
        for (k, n) in lines {
            let _ = writeln!(o, "indeterminate {k} {n}");
        }
        o
    }
}

/// The run's outcome and the text after `demo 1: ` (§10.3), from the
/// cases' results and the pristine files an edit changed.
fn verdict(done: &[(&Case, Done)], through: &[String]) -> (Outcome, String) {
    let failed: Vec<&str> =
        done.iter().filter(|(_, d)| d.result.fails()).map(|(c, _)| c.name.as_str()).collect();
    if !through.is_empty() {
        (Outcome::Failed, format!("FAILED (an edit went through a link: {})", through.join(", ")))
    } else if !failed.is_empty() {
        (Outcome::Failed, format!("FAILED ({})", failed.join(", ")))
    } else if done.iter().all(|(_, d)| d.result.ok()) {
        (Outcome::Ok, "ok".to_owned())
    } else {
        let n = |r: CaseResult| done.iter().filter(|(_, d)| d.result == r).count();
        (
            Outcome::Partial,
            format!("PARTIAL ({} clean, {} partly)", n(CaseResult::Clean), n(CaseResult::Partly)),
        )
    }
}

/// The table's lines for one finding line `w` that became `l` (§10.3).
fn render_line(w: &Wanted, l: &Line, report: &Value, src: &Path) -> String {
    let t = text(w.text().as_bytes());
    match l {
        Line::Found => {
            let mut o = format!("  found    {t}\n");
            for g in groups_of(report, w) {
                for side in ["pre", "post"] {
                    let _ = writeln!(o, "  {}", site_line(side, site(g, side, src).as_ref()));
                }
            }
            o
        }
        Line::NoPlace => format!("  no-place {t}\n"),
        Line::Blocked => {
            let why = blocked_cause(w, report, src)
                .unwrap_or_else(|| "no indeterminate group of that name in the report".to_owned());
            format!("  blocked  {t}: {}\n", text(why.as_bytes()))
        }
        Line::Missing => format!("  MISSING  {t}\n"),
        Line::WrongPlace { file, line } => {
            format!("  WRONG    {t}: at {}:{line}\n", text(file.as_bytes()))
        }
    }
}

/// The groups of the report's findings of `w`'s kind and name.
fn groups_of<'a>(report: &'a Value, w: &Wanted) -> Vec<&'a Value> {
    report
        .get("findings")
        .and_then(|f| f.get("groups"))
        .and_then(Value::as_array)
        .unwrap_or_default()
        .iter()
        .filter(|g| kind_name(g) == (w.kind.as_str(), w.name.as_str()))
        .collect()
}

/// `pre  <file>:<line>:<column> <node> <what>`; `-` for an empty file.
fn site_line(side: &str, s: Option<&Site>) -> String {
    let pad = if side == "pre" { "pre " } else { "post" };
    match s {
        None => format!("{pad} -"),
        Some(s) => {
            let file = if s.file.is_empty() { "-".to_owned() } else { text(s.file.as_bytes()) };
            format!(
                "{pad} {file}:{}:{} {} {}",
                s.line,
                s.column,
                text(s.node.as_bytes()),
                text(s.what.as_bytes())
            )
            .trim_end()
            .to_owned()
        }
    }
}

/// A group with its paths (`first`, `file`, the sites' `file`) relative to
/// `src` where they are under it.
fn relative_group(g: &Value, src: &Path) -> Value {
    let rel = |v: &Value| match v {
        Value::Str(p) => Value::Str(relative(p, src)),
        other => other.clone(),
    };
    let Value::Object(members) = g else { return g.clone() };
    Value::Object(
        members
            .iter()
            .map(|(k, v)| {
                let v = match (k.as_str(), v) {
                    ("first" | "file", _) => rel(v),
                    ("where", Value::Object(sides)) => Value::Object(
                        sides
                            .iter()
                            .map(|(side, site)| {
                                let site = match site {
                                    Value::Object(f) => Value::Object(
                                        f.iter()
                                            .map(|(fk, fv)| {
                                                (
                                                    fk.clone(),
                                                    if fk == "file" { rel(fv) } else { fv.clone() },
                                                )
                                            })
                                            .collect(),
                                    ),
                                    other => other.clone(),
                                };
                                (side.clone(), site)
                            })
                            .collect(),
                    ),
                    _ => v.clone(),
                };
                (k.clone(), v)
            })
            .collect(),
    )
}

/// The run record `slip <what>` printed on its `run` line, loaded; it must
/// be of `plan`.
fn load_printed(ran: &Ran, what: &str, plan: &Digest) -> Result<Loaded, String> {
    let printed = slip_run::line(&ran.stdout, "run")
        .ok_or_else(|| format!("`slip {what}` printed no `run <path>` line"))?;
    let loaded = perf::load(Path::new(printed))?;
    if loaded.path.plan != *plan {
        return Err(format!(
            "`slip {what}` printed the run {printed}, which is not of plan {plan}"
        ));
    }
    Ok(loaded)
}
