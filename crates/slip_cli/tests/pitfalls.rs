//! The pitfalls runner (`spec/pitfalls.md` §1, "Tests"; `docs/wp/WP-056.md`).
//!
//! Drives the `slip` executable over every row of
//! `crates/slip_verify/tests/pitfalls/`, as a user would: for each subject
//! of a row a one-entry compilation database in a directory of its own,
//! `slip parse`, and for each tree of that subject `slip declare` (when the
//! row declares something) and `slip verify --root <subject> --post
//! <tree>`, then the report (`spec/verify.md` §7.1). Each tree gets one
//! outcome of the table of §1; the test fails on any `FAIL`, `XPASS` or
//! `ERROR` and prints the summary of §1 whether or not it passes.
//!
//! The rows' directories are read, never written: the databases, stores,
//! declared files and reports are under the test's temporary directory,
//! removed when the run passes (kept, and named, when it does not, or with
//! `SLIP_PITFALLS_KEEP=1`). `SLIP_PITFALLS_ONLY=<substring>` (any of a
//! comma-separated list) runs the rows whose directory name contains it,
//! for development; the summary then says the run was partial.
//!
//! The rows run on `THREADS` threads, each `slip` with `-j 1`, so the suite
//! holds one token of the host's jobserver per thread at most
//! (`spec/harness.md` §5).
//!
//! The runner's own cases are at the end: the parts that decide an outcome
//! from a report are pure functions and are tested on reports written
//! here; the parts that run `slip` are tested on rows written to a
//! temporary directory.

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::io::{Read, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use slip_model::json::{self, Value};

const SLIP: &str = env!("CARGO_BIN_EXE_slip");

/// The one switch for places (`spec/pitfalls.md` §1 "Places"). Localisation
/// (`spec/verify.md` §6.1, WP-054) is on main: `Mandatory`. Read in one
/// place, [`pitfalls`]; the runner's own cases pass the state they test.
const PLACES: Places = Places::Mandatory;

/// The threads the rows run on.
const THREADS: usize = 4;

/// The bound on one command (`slip` or the compiler).
const BOUND: Duration = Duration::from_mins(3);

/// The kinds of finding that say the walk gave up (`spec/verify.md` §7).
const INDETERMINATE: [&str; 2] = ["indeterminate", "derived_indeterminate"];

/// The kinds a `mismatch <kind> <entity>` line may name.
const KINDS: [&str; 8] = [
    "changed",
    "removed",
    "added",
    "derived_changed",
    "indeterminate",
    "derived_indeterminate",
    "derived_only_pre",
    "derived_only_post",
];

/// How a line with ` @ <file>:<line>` is met (`spec/pitfalls.md` §1 "Places").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Places {
    /// The finding is there, and has no `where` or has it at that place.
    Advisory,
    /// The finding is there with a `where` at that place.
    Mandatory,
}

impl Places {
    fn name(self) -> &'static str {
        match self {
            Places::Advisory => "advisory",
            Places::Mandatory => "mandatory",
        }
    }
}

// ----- expect.txt -------------------------------------------------------------------

/// A finding a `mismatch <kind> <entity> [@ <file>:<line>]` line wants.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Wanted {
    kind: String,
    name: String,
    at: Option<(String, u64)>,
}

/// What a tree must give (`spec/pitfalls.md` §1, the grammar of `expect.txt`).
#[derive(Debug, Clone, PartialEq, Eq)]
enum Line {
    Verified,
    NotParsed,
    /// `mismatch` alone when empty; else every line of the tree.
    Mismatch(Vec<Wanted>),
    /// `none <reason>`: the tree has no directory.
    None(String),
}

impl std::fmt::Display for Line {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Line::Verified => f.write_str("verified"),
            Line::NotParsed => f.write_str("not_parsed"),
            Line::None(r) => write!(f, "none {r}"),
            Line::Mismatch(ws) if ws.is_empty() => f.write_str("mismatch"),
            Line::Mismatch(ws) => {
                for (i, w) in ws.iter().enumerate() {
                    if i > 0 {
                        f.write_str("; ")?;
                    }
                    write!(f, "mismatch {} {}", w.kind, w.name)?;
                    if let Some((file, line)) = &w.at {
                        write!(f, " @ {file}:{line}")?;
                    }
                }
                Ok(())
            }
        }
    }
}

/// A parsed `expect.txt`.
#[derive(Debug, Default)]
struct Expect {
    std: Option<String>,
    lines: BTreeMap<String, Line>,
    xfail: BTreeMap<String, String>,
}

/// A tree's name: `good`, `bad`, `good-<k>`, `bad-<k>` with `k` ≥ 2.
/// `Some((bad, k))`.
fn tree_name(s: &str) -> Option<(bool, Option<u32>)> {
    let (base, k) = match s.split_once('-') {
        Some((b, k)) => (b, Some(number(k)?)),
        None => (s, None),
    };
    let bad = match base {
        "good" => false,
        "bad" => true,
        _ => return None,
    };
    Some((bad, k))
}

/// The `k` of `pre-<k>`, `declared-<k>.txt` or a tree: digits, no leading
/// zero, at least 2.
fn number(s: &str) -> Option<u32> {
    if s.is_empty() || s.starts_with('0') || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok().filter(|k| *k >= 2)
}

fn parse_line(what: &str) -> Result<Line, String> {
    match what {
        "verified" => return Ok(Line::Verified),
        "not_parsed" => return Ok(Line::NotParsed),
        "mismatch" => return Ok(Line::Mismatch(Vec::new())),
        "none" => return Err("`none` needs a reason".to_owned()),
        _ => {}
    }
    if let Some(reason) = what.strip_prefix("none ") {
        return Ok(Line::None(reason.trim().to_owned()));
    }
    let Some(rest) = what.strip_prefix("mismatch ") else {
        return Err(format!("`{what}` is not a line of spec/pitfalls.md §1"));
    };
    let rest = rest.trim();
    let (kind, entity) = rest.split_once(' ').ok_or_else(|| format!("`{what}` names no entity"))?;
    if !KINDS.contains(&kind) {
        return Err(format!("`{kind}` is not a kind of finding"));
    }
    let (name, at) = match entity.rsplit_once(" @ ") {
        Some((name, place)) => {
            let (file, line) = place
                .trim()
                .rsplit_once(':')
                .ok_or_else(|| format!("`{place}` is not <file>:<line>"))?;
            let line: u64 = line
                .parse()
                .ok()
                .filter(|n| *n > 0)
                .ok_or_else(|| format!("`{place}` has no line number"))?;
            if file.is_empty() {
                return Err(format!("`{place}` has no file"));
            }
            (name.trim(), Some((file.to_owned(), line)))
        }
        None => (entity.trim(), None),
    };
    if name.is_empty() {
        return Err(format!("`{what}` names no entity"));
    }
    Ok(Line::Mismatch(vec![Wanted { kind: kind.to_owned(), name: name.to_owned(), at }]))
}

fn parse_expect(text: &str) -> Result<Expect, String> {
    let mut e = Expect::default();
    for (i, raw) in text.lines().enumerate() {
        let n = i + 1;
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let at = |m: String| format!("expect.txt:{n}: {m}");
        if let Some(rest) = line.strip_prefix("std:") {
            let std = rest.trim();
            if std.is_empty() || std.contains(char::is_whitespace) {
                return Err(at(format!("`{line}` is not `std: <standard>`")));
            }
            if e.std.replace(std.to_owned()).is_some() {
                return Err(at("a second `std:` line".to_owned()));
            }
        } else if let Some(rest) = line.strip_prefix("xfail ") {
            let (tree, reason) =
                rest.split_once(':').ok_or_else(|| at("`xfail` without `<tree>:`".to_owned()))?;
            let (tree, reason) = (tree.trim(), reason.trim());
            if tree_name(tree).is_none() {
                return Err(at(format!("`{tree}` is not a tree")));
            }
            if reason.is_empty() {
                return Err(at(format!("`xfail {tree}` gives no reason")));
            }
            if e.xfail.insert(tree.to_owned(), reason.to_owned()).is_some() {
                return Err(at(format!("a second `xfail {tree}`")));
            }
        } else {
            let (tree, what) = line
                .split_once(':')
                .ok_or_else(|| at(format!("`{line}` is not `<tree>: <what>`")))?;
            let tree = tree.trim();
            if tree_name(tree).is_none() {
                return Err(at(format!("`{tree}` is not a tree")));
            }
            let new = parse_line(what.trim()).map_err(at)?;
            match (e.lines.get_mut(tree), new) {
                (None, new) => {
                    e.lines.insert(tree.to_owned(), new);
                }
                (Some(Line::Mismatch(old)), Line::Mismatch(more))
                    if !old.is_empty() && !more.is_empty() =>
                {
                    old.extend(more);
                }
                (Some(_), _) => return Err(at(format!("a second line for `{tree}`"))),
            }
        }
    }
    Ok(e)
}

// ----- a row -----------------------------------------------------------------------

#[derive(Debug)]
struct Tree {
    name: String,
    bad: bool,
    dir: PathBuf,
}

#[derive(Debug)]
struct Subject {
    /// `pre` or `pre-<k>`.
    name: String,
    dir: PathBuf,
    /// The declared file of every tree of this subject.
    declared: Option<PathBuf>,
    trees: Vec<Tree>,
}

#[derive(Debug)]
struct Row {
    name: String,
    expect: Expect,
    subjects: Vec<Subject>,
}

impl Row {
    /// The trees that have a line but no directory (`none`), for the count.
    fn bad_lines(&self) -> impl Iterator<Item = &Line> {
        self.expect
            .lines
            .iter()
            .filter(|(t, _)| tree_name(t).is_some_and(|(bad, _)| bad))
            .map(|(_, l)| l)
    }

    /// Caught only by a failed parse (`spec/pitfalls.md` §5).
    fn parse_only(&self) -> bool {
        let mut bad = self.bad_lines().peekable();
        bad.peek().is_some() && bad.all(|l| *l == Line::NotParsed)
    }
}

/// Reads and checks the layout of a row (`spec/pitfalls.md` §1, "A row is
/// malformed").
#[expect(clippy::too_many_lines, reason = "one rule after another of §1")]
fn read_row(dir: &Path) -> Result<Row, String> {
    let name = dir.file_name().and_then(|n| n.to_str()).ok_or("the row's name is not UTF-8")?;
    let mut pres: BTreeMap<u32, PathBuf> = BTreeMap::new();
    let mut declared: BTreeMap<u32, PathBuf> = BTreeMap::new();
    let mut trees: BTreeMap<String, PathBuf> = BTreeMap::new();
    let mut expect_text = None;
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let file = entry.file_name();
        let file = file.to_str().ok_or("an entry's name is not UTF-8")?;
        let is_dir = entry.file_type().map_err(|e| e.to_string())?.is_dir();
        let bad_entry = || format!("unexpected entry `{file}`");
        if is_dir {
            if file == "pre" {
                pres.insert(1, path);
            } else if let Some(k) = file.strip_prefix("pre-") {
                pres.insert(number(k).ok_or_else(bad_entry)?, path);
            } else if tree_name(file).is_some() {
                trees.insert(file.to_owned(), path);
            } else {
                return Err(bad_entry());
            }
        } else if file == "expect.txt" {
            expect_text =
                Some(std::fs::read_to_string(&path).map_err(|e| format!("expect.txt: {e}"))?);
        } else if file == "declared.txt" {
            declared.insert(1, path);
        } else if let Some(k) = file.strip_prefix("declared-").and_then(|r| r.strip_suffix(".txt"))
        {
            declared.insert(number(k).ok_or_else(bad_entry)?, path);
        } else {
            return Err(bad_entry());
        }
    }
    let expect = parse_expect(&expect_text.ok_or("no expect.txt")?)?;
    if !pres.contains_key(&1) {
        return Err("no pre/".to_owned());
    }
    for k in declared.keys() {
        if *k > 1 && !pres.contains_key(k) {
            return Err(format!("declared-{k}.txt has no pre-{k}/"));
        }
    }
    for tree in trees.keys() {
        match expect.lines.get(tree) {
            None => return Err(format!("{tree}/ has no line in expect.txt")),
            Some(Line::None(_)) => {
                return Err(format!("`{tree}: none` beside a {tree}/ directory"));
            }
            Some(Line::Verified) if tree.starts_with("bad") => {
                return Err(format!("`{tree}: verified`: a bad tree that verifies is a FAIL"));
            }
            Some(_) => {}
        }
    }
    for (tree, line) in &expect.lines {
        let (bad, k) = tree_name(tree).unwrap_or_default();
        match line {
            Line::None(reason) => {
                if bad {
                    return Err(format!("`{tree}: none`: only a good tree may have none"));
                }
                if reason.is_empty() {
                    return Err(format!("`{tree}: none` gives no reason"));
                }
                if let Some(k) = k
                    && !pres.contains_key(&k)
                {
                    return Err(format!("`{tree}: none` and no pre-{k}/"));
                }
            }
            _ if !trees.contains_key(tree) => {
                return Err(format!("`{tree}` has a line and no directory"));
            }
            _ => {}
        }
    }
    for k in pres.keys().filter(|k| **k > 1) {
        let good = format!("good-{k}");
        if !trees.contains_key(&good) && !expect.lines.contains_key(&good) {
            return Err(format!("pre-{k}/ has neither good-{k}/ nor a `good-{k}: none` line"));
        }
    }
    for tree in expect.xfail.keys() {
        match expect.lines.get(tree) {
            None | Some(Line::None(_)) => {
                return Err(format!("`xfail {tree}` and {tree} has no directory"));
            }
            Some(_) => {}
        }
    }
    let mut subjects: Vec<Subject> = pres
        .iter()
        .map(|(k, dir)| Subject {
            name: if *k == 1 { "pre".to_owned() } else { format!("pre-{k}") },
            dir: dir.clone(),
            declared: declared.get(k).or_else(|| declared.get(&1)).cloned(),
            trees: Vec::new(),
        })
        .collect();
    let mut ordered: Vec<(Option<u32>, bool, String, PathBuf)> = trees
        .into_iter()
        .map(|(t, d)| {
            let (bad, k) = tree_name(&t).unwrap_or_default();
            (k, bad, t, d)
        })
        .collect();
    ordered.sort();
    for (k, bad, name, dir) in ordered {
        let subject = k.filter(|k| pres.contains_key(k)).unwrap_or(1);
        let at = pres.keys().position(|p| *p == subject).unwrap_or(0);
        subjects[at].trees.push(Tree { name, bad, dir });
    }
    Ok(Row { name: name.to_owned(), expect, subjects })
}

// ----- the catalogue against the directories ----------------------------------------

/// The ids of the rows of §2 to §4 of `spec/pitfalls.md`, in file order.
fn catalogue_ids(spec: &str) -> Vec<String> {
    let mut ids = Vec::new();
    let mut inside = false;
    for line in spec.lines() {
        if line.starts_with("## ") {
            inside = ["## 2.", "## 3.", "## 4."].iter().any(|s| line.starts_with(s));
            continue;
        }
        if !inside {
            continue;
        }
        let Some(cell) = line.strip_prefix("| ").and_then(|l| l.split(" |").next()) else {
            continue;
        };
        let b = cell.as_bytes();
        if b.len() >= 3 && b[0].is_ascii_uppercase() && b[1..].iter().all(u8::is_ascii_digit) {
            ids.push(cell.to_owned());
        }
    }
    ids
}

/// Every row has exactly one directory and every directory a row
/// (`spec/pitfalls.md` §1 "The catalogue against the directories"). The
/// errors, each `(<directory or id>, <message>)`.
fn check_catalogue(ids: &[String], dirs: &[String]) -> Vec<(String, String)> {
    let mut errors = Vec::new();
    let mut seen = BTreeSet::new();
    for id in ids {
        if !seen.insert(id.as_str()) {
            errors.push((id.clone(), "the id is twice in spec/pitfalls.md".to_owned()));
        }
    }
    let mut found: BTreeMap<String, Vec<&str>> = BTreeMap::new();
    for d in dirs {
        let valid =
            !d.is_empty() && d.bytes().all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'-'));
        let parts = d.split_once('-').filter(|(_, slug)| !slug.is_empty());
        match parts {
            Some((id, _)) if valid => {
                let id = id.to_ascii_uppercase();
                if !seen.contains(id.as_str()) {
                    errors.push((
                        d.clone(),
                        "a directory without a row in spec/pitfalls.md".to_owned(),
                    ));
                }
                found.entry(id).or_default().push(d);
            }
            _ => errors.push((d.clone(), "not `<id>-<slug>` of [a-z0-9-]".to_owned())),
        }
    }
    for id in &seen {
        match found.get(*id).map(Vec::as_slice) {
            None | Some([]) => {
                errors.push(((*id).to_owned(), "a row without a directory".to_owned()));
            }
            Some([_]) => {}
            Some(many) => {
                errors
                    .push(((*id).to_owned(), format!("several directories: {}", many.join(", "))));
            }
        }
    }
    errors
}

// ----- the report ------------------------------------------------------------------

/// A site of a group's `where`, its file made relative to the tree or the
/// subject.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Site {
    file: String,
    line: u64,
    what: String,
}

#[derive(Debug, Clone)]
struct Group {
    kind: String,
    name: String,
    key: String,
    pre_id: String,
    post_id: String,
    pre: Option<Site>,
    post: Option<Site>,
}

/// What a tree gave.
#[derive(Debug, Clone)]
struct Found {
    /// `verified`, `not_verified` or `incomplete`.
    verdict: String,
    /// The observer's state (`spec/verify.md` §7).
    state: String,
    message: String,
    groups: Vec<Group>,
}

impl Found {
    fn describe(&self) -> String {
        let mut s = format!("verdict {}, {}", self.verdict, self.state);
        if !self.message.is_empty() {
            let _ = write!(s, " ({})", first_line(&self.message));
        }
        if self.groups.is_empty() {
            return s;
        }
        let mut groups: Vec<&Group> = self.groups.iter().collect();
        groups.sort_by_key(|g| INDETERMINATE.contains(&g.kind.as_str()));
        let _ = write!(s, ", {} group(s):", groups.len());
        for g in groups.iter().take(4) {
            let _ = write!(s, " [{} {}", g.kind, g.name);
            let site = if matches!(g.kind.as_str(), "removed" | "derived_only_pre") {
                g.pre.as_ref()
            } else {
                g.post.as_ref().or(g.pre.as_ref())
            };
            if let Some(site) = site {
                let _ = write!(s, " @ {}:{}", site.file, site.line);
                if !site.what.is_empty() && INDETERMINATE.contains(&g.kind.as_str()) {
                    let _ = write!(s, " ({})", site.what);
                }
            }
            s.push(']');
        }
        if groups.len() > 4 {
            s.push_str(" …");
        }
        s
    }
}

fn first_line(s: &str) -> &str {
    s.lines().next().unwrap_or("")
}

/// `path` relative to the first of `roots` it is under, else as it is.
fn relative(path: &str, roots: &[&Path]) -> String {
    for r in roots {
        if let Ok(p) = Path::new(path).strip_prefix(r)
            && let Some(p) = p.to_str()
            && !p.is_empty()
        {
            return p.to_owned();
        }
    }
    path.to_owned()
}

fn read_site(g: &Value, side: &str, roots: &[&Path]) -> Option<Site> {
    let s = g.get("where")?.get(side)?;
    let file = s.get("file")?.as_str()?;
    let line = s.get("line")?.as_u64()?;
    if file.is_empty() && line == 0 {
        return None;
    }
    let what = s.get("what").and_then(Value::as_str).unwrap_or("").to_owned();
    Some(Site { file: relative(file, roots), line, what })
}

/// The one observer's state and the groups of a report (`spec/verify.md`
/// §7.1); places relative to the tree or the subject (`roots`).
fn read_found(report: &Value, roots: &[&Path]) -> Result<Found, String> {
    let str_at = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).map(str::to_owned);
    let verdict = str_at(report, "verdict").ok_or("the report has no verdict")?;
    let obs = report.get("observers").ok_or("the report has no `observers`")?;
    if obs.get("selected").and_then(Value::as_u64) != Some(1) {
        return Err("the run did not select one observer".to_owned());
    }
    let nv = report.get("not_verified").and_then(Value::as_array).ok_or("no `not_verified`")?;
    let (state, message) = match nv.first() {
        Some(o) => (
            str_at(o, "state").ok_or("a `not_verified` entry has no state")?,
            str_at(o, "message").unwrap_or_default(),
        ),
        None if obs.get("verified").and_then(Value::as_u64) == Some(1) => {
            ("verified".to_owned(), String::new())
        }
        None => ("unchanged".to_owned(), String::new()),
    };
    let groups = report
        .get("findings")
        .and_then(|f| f.get("groups"))
        .and_then(Value::as_array)
        .ok_or("the report's `findings.groups` is not an array")?;
    let groups = groups
        .iter()
        .map(|g| Group {
            kind: str_at(g, "kind").unwrap_or_default(),
            name: str_at(g, "name").unwrap_or_default(),
            key: str_at(g, "key").unwrap_or_default(),
            pre_id: str_at(g, "pre").unwrap_or_else(|| "null".to_owned()),
            post_id: str_at(g, "post").unwrap_or_else(|| "null".to_owned()),
            pre: read_site(g, "pre", roots),
            post: read_site(g, "post", roots),
        })
        .collect();
    Ok(Found { verdict, state, message, groups })
}

// ----- lines against what was found ---------------------------------------------------

/// Whether `site` is the place `at`.
fn at_place(site: Option<&Site>, at: &(String, u64)) -> bool {
    site.is_some_and(|s| s.file == at.0 && s.line == at.1)
}

/// Whether the line `w` is met by `found`'s groups; `Some(skipped)` if it
/// is, `skipped` 1 when it is met without its place (advisory only).
/// `step4(name)` says whether the entity is declared in the subject and
/// derived in the tree, whose `changed` is placed at `where.pre`
/// (`spec/pitfalls.md` §1); it is asked only when the two sides disagree.
fn wanted_met(
    w: &Wanted,
    found: &Found,
    places: Places,
    step4: &mut dyn FnMut(&str) -> bool,
) -> Option<usize> {
    let groups: Vec<&Group> =
        found.groups.iter().filter(|g| g.kind == w.kind && g.name == w.name).collect();
    if groups.is_empty() {
        return None;
    }
    let Some(at) = &w.at else { return Some(0) };
    let mut absent = false;
    let mut asked: Option<bool> = None;
    for g in groups {
        let side = if matches!(w.kind.as_str(), "removed" | "derived_only_pre") {
            g.pre.as_ref()
        } else if w.kind == "changed"
            && at_place(g.pre.as_ref(), at) != at_place(g.post.as_ref(), at)
        {
            let s4 = *asked.get_or_insert_with(|| step4(&w.name));
            if s4 { g.pre.as_ref() } else { g.post.as_ref() }
        } else {
            g.post.as_ref()
        };
        if at_place(side, at) {
            return Some(0);
        }
        absent |= side.is_none();
    }
    (absent && places == Places::Advisory).then_some(1)
}

/// Whether `line` is met by `found` (`spec/pitfalls.md` §1); `Some(skipped)`.
fn meets(
    line: &Line,
    found: &Found,
    places: Places,
    step4: &mut dyn FnMut(&str) -> bool,
) -> Option<usize> {
    match line {
        Line::Verified => (found.verdict == "verified" && found.state == "verified").then_some(0),
        Line::NotParsed => (found.state == "not_parsed").then_some(0),
        Line::None(_) => None,
        Line::Mismatch(ws) => {
            if found.state != "mismatch" {
                return None;
            }
            if ws.is_empty() {
                return found
                    .groups
                    .iter()
                    .any(|g| !INDETERMINATE.contains(&g.kind.as_str()))
                    .then_some(0);
            }
            let mut skipped = 0;
            for w in ws {
                skipped += wanted_met(w, found, places, step4)?;
            }
            Some(skipped)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Outcome {
    Pass,
    Xfail,
    Fail,
    Xpass,
    Error,
}

impl Outcome {
    fn name(self) -> &'static str {
        match self {
            Outcome::Pass => "PASS",
            Outcome::Xfail => "XFAIL",
            Outcome::Fail => "FAIL",
            Outcome::Xpass => "XPASS",
            Outcome::Error => "ERROR",
        }
    }
}

/// The table of outcomes (`spec/pitfalls.md` §1), for a tree that ran.
#[expect(clippy::fn_params_excessive_bools, reason = "the four columns of the table")]
fn outcome(bad: bool, verified: bool, met: bool, xfail: bool) -> Outcome {
    if bad && verified {
        return Outcome::Fail;
    }
    match (met, xfail) {
        (true, false) => Outcome::Pass,
        (false, false) => Outcome::Fail,
        (false, true) => Outcome::Xfail,
        (true, true) => Outcome::Xpass,
    }
}

// ----- running slip ----------------------------------------------------------------

#[derive(Debug)]
struct Out {
    status: ExitStatus,
    stdout: String,
    stderr: String,
}

impl Out {
    fn code(&self) -> Option<i32> {
        self.status.code()
    }

    fn value(&self, key: &str) -> Option<&str> {
        self.stdout.lines().find_map(|l| l.strip_prefix(key)?.strip_prefix(' '))
    }

    fn brief(&self) -> String {
        format!("status {:?}: {}", self.code(), first_line(self.stderr.trim()))
    }
}

fn run_bounded(mut cmd: Command) -> Result<Out, String> {
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| format!("cannot start {cmd:?}: {e}"))?;
    let stdout = reader(child.stdout.take().expect("piped"));
    let stderr = reader(child.stderr.take().expect("piped"));
    let status = wait_bounded(&mut child)?;
    Ok(Out {
        status,
        stdout: stdout.join().unwrap_or_default(),
        stderr: stderr.join().unwrap_or_default(),
    })
}

fn reader(mut r: impl Read + Send + 'static) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = r.read_to_end(&mut bytes);
        String::from_utf8_lossy(&bytes).into_owned()
    })
}

fn wait_bounded(child: &mut Child) -> Result<ExitStatus, String> {
    let deadline = Instant::now() + BOUND;
    let mut pause = Duration::from_millis(1);
    loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("did not finish within {BOUND:?}"));
        }
        std::thread::sleep(pause);
        pause = (pause * 2).min(Duration::from_millis(25));
    }
}

fn slip(dir: &Path, args: &[&str]) -> Result<Out, String> {
    let mut cmd = Command::new(SLIP);
    cmd.args(args).current_dir(dir);
    run_bounded(cmd).map_err(|e| format!("slip {}: {e}", args.first().unwrap_or(&"")))
}

/// Where the suite runs: the compiler of tier T2, if any.
#[derive(Debug)]
struct Env {
    cc: Option<PathBuf>,
    require_all: bool,
}

impl Env {
    fn from_process() -> Env {
        Env {
            cc: compiler(),
            require_all: std::env::var_os("SLIP_REQUIRE_ALL").is_some_and(|v| v == "1"),
        }
    }
}

/// `clang++-19`, else `clang++`, on `PATH` (`spec/pitfalls.md` §1, tier T2).
fn compiler() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for name in ["clang++-19", "clang++"] {
        for d in std::env::split_paths(&path) {
            let p = d.join(name);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

/// Substrings of clang's standard error that mark a compiler crash even
/// when the driver exits with status 1 (`docs/wp/WP-171.md`).
const CRASH_MARKERS: [&str; 2] =
    ["PLEASE submit a bug report", "frontend command failed due to signal"];

fn has_crash_text(stderr: &str) -> bool {
    CRASH_MARKERS.iter().any(|m| stderr.contains(m))
}

/// What the production compiler said of a tree: `Ok(None)` for skipped,
/// `Ok(Some((accepted, diagnostics)))` for a clean accept (exit 0) or clean
/// rejection (exit 1 without crash text), or `Err` when the compiler crashed
/// (any other exit status, termination by signal, or crash text on stderr),
/// naming the tree.
///
/// Isolates `TMPDIR` inside `tmp` (under the run's work directory, removed
/// on pass) and passes `-fno-crash-diagnostics` (`WP-167`).
fn compile(
    env: &Env,
    tree: &Path,
    std: &str,
    tmp: &Path,
) -> Result<Option<(bool, String)>, String> {
    let Some(cc) = &env.cc else {
        if env.require_all {
            return Err("tier T2 was skipped: no clang++ (SLIP_REQUIRE_ALL=1)".to_owned());
        }
        return Ok(None);
    };
    std::fs::create_dir_all(tmp).map_err(|e| format!("{}: {e}", tmp.display()))?;
    let mut cmd = Command::new(cc);
    cmd.args([
        &format!("-std={std}"),
        "-fsyntax-only",
        "-fno-crash-diagnostics",
        "-I.",
        "-Idep",
        "main.cpp",
    ])
    .env("TMPDIR", tmp)
    .current_dir(tree);
    let out = run_bounded(cmd)?;
    if !has_crash_text(&out.stderr) {
        match out.code() {
            Some(0) => return Ok(Some((true, out.stderr))),
            Some(1) => return Ok(Some((false, out.stderr))),
            _ => {}
        }
    }
    let how = match out.code() {
        Some(c) => format!("exit {c}"),
        None => "killed by signal".to_owned(),
    };
    let detail = out
        .stderr
        .lines()
        .find(|l| CRASH_MARKERS.iter().any(|m| l.contains(m)))
        .or_else(|| out.stderr.lines().find(|l| !l.trim().is_empty()))
        .unwrap_or("")
        .trim();
    Err(format!(
        "clang crashed on {} ({how}){}",
        tree.display(),
        if detail.is_empty() { String::new() } else { format!(": {detail}") }
    ))
}

/// Every file under `a` is under `b` with the same bytes, and the reverse.
fn same_tree(a: &Path, b: &Path) -> Result<(), String> {
    fn files(root: &Path, dir: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) -> std::io::Result<()> {
        for e in std::fs::read_dir(dir)? {
            let e = e?;
            let p = e.path();
            if e.file_type()?.is_dir() {
                files(root, &p, out)?;
            } else {
                out.insert(p.strip_prefix(root).unwrap_or(&p).to_owned(), std::fs::read(&p)?);
            }
        }
        Ok(())
    }
    let read = |d: &Path| {
        let mut m = BTreeMap::new();
        if d.exists() {
            files(d, d, &mut m).map_err(|e| format!("{}: {e}", d.display()))?;
        }
        Ok::<_, String>(m)
    };
    let (fa, fb) = (read(a)?, read(b)?);
    for (p, bytes) in &fa {
        match fb.get(p) {
            None => return Err(format!("dep/{} is missing", p.display())),
            Some(other) if other != bytes => return Err(format!("dep/{} differs", p.display())),
            Some(_) => {}
        }
    }
    if let Some(p) = fb.keys().find(|p| !fa.contains_key(*p)) {
        return Err(format!("dep/{} is not in the subject", p.display()));
    }
    Ok(())
}

/// What became of one tree.
#[derive(Debug, Clone)]
struct TreeResult {
    row: String,
    tree: String,
    outcome: Outcome,
    expected: String,
    found: String,
    /// Lines met without their place (advisory).
    skipped: usize,
    t2_skipped: bool,
    notes: Vec<String>,
}

/// A subject parsed: its directory, real; its work directory; its plan.
struct Parsed<'a> {
    subject: &'a Subject,
    real: PathBuf,
    work: PathBuf,
    plan: String,
    std: String,
}

/// Runs one row; one result per tree, or one `ERROR` for the row.
fn run_row(dir: &Path, work: &Path, env: &Env, places: Places) -> Vec<TreeResult> {
    let name = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let error = |tree: &str, expected: &str, found: String| TreeResult {
        row: name.clone(),
        tree: tree.to_owned(),
        outcome: Outcome::Error,
        expected: expected.to_owned(),
        found,
        skipped: 0,
        t2_skipped: false,
        notes: Vec::new(),
    };
    let row = match read_row(dir) {
        Ok(r) => r,
        Err(e) => return vec![error("-", "a well-formed row", e)],
    };
    let std = row.expect.std.clone().unwrap_or_else(|| "c++20".to_owned());
    let mut results = Vec::new();
    for subject in &row.subjects {
        let work = work.join(&row.name).join(&subject.name);
        let parsed = parse_subject(subject, &work, &std, env);
        match parsed {
            Ok(p) => {
                for (i, tree) in subject.trees.iter().enumerate() {
                    results.push(run_tree(&row, &p, tree, (i + 1) as u64, env, places));
                }
            }
            Err(e) => {
                for tree in &subject.trees {
                    let line = row
                        .expect
                        .lines
                        .get(&tree.name)
                        .map(ToString::to_string)
                        .unwrap_or_default();
                    results.push(error(&tree.name, &line, format!("{}: {e}", subject.name)));
                }
            }
        }
    }
    results
}

/// The subject's database, `slip parse`, and tier T2 on the subject.
fn parse_subject<'a>(
    subject: &'a Subject,
    work: &Path,
    std: &str,
    env: &Env,
) -> Result<Parsed<'a>, String> {
    std::fs::create_dir_all(work).map_err(|e| format!("{}: {e}", work.display()))?;
    let real = std::fs::canonicalize(&subject.dir).map_err(|e| e.to_string())?;
    let t2 = compile(env, &real, std, &work.join("clang-tmp"))?;
    if let Some((false, diag)) = &t2 {
        return Err(format!("clang rejects the subject: {}", first_line(diag)));
    }
    let dir = json_string(real.to_str().ok_or("the subject's path is not UTF-8")?);
    let db = format!(
        "[\n{{\"directory\": {dir}, \"file\": \"main.cpp\", \"command\": \"clang++ -std={std} -I. -Idep -c main.cpp\"}}\n]\n"
    );
    std::fs::write(work.join("cdb.json"), db).map_err(|e| e.to_string())?;
    let out = slip(work, &["parse", "--db", "cdb.json", "--store", "store", "-j", "1"])?;
    if out.code() != Some(0) {
        return Err(format!("slip parse: {}", out.brief()));
    }
    let plan = out.value("plan").ok_or("slip parse printed no plan")?.to_owned();
    Ok(Parsed { subject, real, work: work.to_owned(), plan, std: std.to_owned() })
}

fn json_string(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            c if (c as u32) < 0x20 => {
                let _ = write!(o, "\\u{:04x}", c as u32);
            }
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

/// `--root <subject> --post <tree>`.
fn image<'a>(p: &'a Parsed<'_>, tree: &'a str) -> [&'a str; 4] {
    ["--root", p.real.to_str().unwrap_or(""), "--post", tree]
}

/// `slip declare` for a tree: the path of the `DECL` container, or `None`
/// when nothing is declared. In a `bad` tree a line that fails with
/// `NoUnit` is left out and retried without (`spec/pitfalls.md` §1).
fn declare(
    p: &Parsed<'_>,
    tree: &Tree,
    tree_real: &str,
    notes: &mut Vec<String>,
) -> Result<Option<PathBuf>, String> {
    let Some(from) = &p.subject.declared else { return Ok(None) };
    let text = std::fs::read_to_string(from).map_err(|e| format!("{}: {e}", from.display()))?;
    let mut lines: Vec<&str> = text.lines().collect();
    let effective =
        |ls: &[&str]| ls.iter().any(|l| !l.trim().is_empty() && !l.trim().starts_with('#'));
    let from_file = p.work.join(format!("{}.declared.txt", tree.name));
    let out_path = p.work.join(format!("{}.decl", tree.name));
    let (from_s, out_s) = (from_file.to_str().unwrap_or(""), out_path.to_str().unwrap_or(""));
    let dep = p.real.join("dep");
    let dep_s = dep.to_str().unwrap_or("").to_owned();
    loop {
        if !effective(&lines) {
            return Ok(None);
        }
        let mut body = lines.join("\n");
        body.push('\n');
        std::fs::write(&from_file, body).map_err(|e| e.to_string())?;
        let mut args = vec![
            "declare", "--plan", &p.plan, "--store", "store", "--from", from_s, "--out", out_s,
        ];
        args.extend(["--owned", p.real.to_str().unwrap_or("")]);
        if dep.is_dir() {
            args.extend(["--readonly", &dep_s]);
        }
        args.extend(image(p, tree_real));
        let out = slip(&p.work, &args)?;
        match out.code() {
            Some(0) => return Ok(Some(out_path)),
            Some(1) if tree.bad => {
                let n = out.stderr.lines().find_map(|l| {
                    let i = l.find(": no unit: ")?;
                    l[..i].rsplit(':').next()?.parse::<usize>().ok()
                });
                match n {
                    Some(n) if n >= 1 && n <= lines.len() && !lines[n - 1].trim().is_empty() => {
                        notes.push(format!(
                            "declared line {n} left out (no unit in this tree): {}",
                            lines[n - 1].trim()
                        ));
                        lines[n - 1] = "";
                    }
                    _ => return Err(refused(&out, from_s)),
                }
            }
            Some(1) => return Err(refused(&out, from_s)),
            _ => return Err(format!("slip declare: {}", out.brief())),
        }
    }
}

/// How an `ERROR` from `slip declare` refusing the declared file begins.
const REFUSED: &str = "slip declare refuses the declared file: ";

/// The message of a `slip declare` that refused the text `from` (status
/// 1): `REFUSED`, then `line <n>: <class>: <detail>` as `slip` printed it
/// (`spec/exec.md` §8.8).
fn refused(out: &Out, from: &str) -> String {
    let error = out.stderr.lines().find_map(|l| {
        let rest = l.strip_prefix("slip: ")?.strip_prefix(from)?.strip_prefix(':')?;
        let (n, rest) = rest.split_once(": ")?;
        Some(format!("line {n}: {rest}"))
    });
    match error {
        Some(e) => format!("{REFUSED}{e}"),
        None => format!("slip declare: {}", out.brief()),
    }
}

/// Whether an `xfail` with `reason` covers the `ERROR` `error`: only a
/// refusal of the declared file, and only when the reason quotes, in
/// backquotes, the error's class or a word the error quotes
/// (`spec/pitfalls.md` §1, `xfail`).
fn covers(reason: &str, error: &str) -> bool {
    let Some(rest) = error.strip_prefix(REFUSED) else { return false };
    let Some((_, rest)) = rest.split_once(": ") else { return false };
    let (class, detail) = rest.split_once(": ").unwrap_or((rest, ""));
    let mut quoted: Vec<&str> = detail.split('`').skip(1).step_by(2).collect();
    quoted.push(class);
    quoted.iter().any(|q| !q.is_empty() && reason.contains(&format!("`{q}`")))
}

/// Whether `name` is declared in the subject and derived in the tree
/// (`spec/verify.md` §4 step 4), from `slip units`.
fn step4(p: &Parsed<'_>, tree_real: &str, name: &str) -> bool {
    let units = |post: bool, want: &str| -> bool {
        let mut args = vec!["units", "--plan", &p.plan, "--store", "store", "--match", name];
        if post {
            args.push("--post-image");
            args.extend(image(p, tree_real));
        }
        let Ok(out) = slip(&p.work, &args) else { return false };
        out.code() == Some(0)
            && out.stdout.lines().any(|l| {
                let f: Vec<&str> = l.splitn(5, ' ').collect();
                f.len() == 5 && f[2] == want && f[4] == name
            })
    };
    units(false, "declared") && units(true, "derived")
}

fn run_tree(
    row: &Row,
    p: &Parsed<'_>,
    tree: &Tree,
    round: u64,
    env: &Env,
    places: Places,
) -> TreeResult {
    let line = row.expect.lines.get(&tree.name).cloned().unwrap_or(Line::None(String::new()));
    let xfail = row.expect.xfail.get(&tree.name);
    let mut r = TreeResult {
        row: row.name.clone(),
        tree: tree.name.clone(),
        outcome: Outcome::Error,
        expected: line.to_string(),
        found: String::new(),
        skipped: 0,
        t2_skipped: false,
        notes: Vec::new(),
    };
    if let Some(x) = xfail {
        r.notes.push(format!("xfail: {x}"));
    }
    match evaluate(p, tree, &line, xfail.is_some(), round, env, places, &mut r) {
        Ok(()) => {}
        Err(e) => {
            let covered = xfail.is_some_and(|x| covers(x, &e));
            r.outcome = if covered { Outcome::Xfail } else { Outcome::Error };
            r.found = e;
        }
    }
    r
}

#[expect(clippy::too_many_arguments, reason = "the parts of one tree's run")]
fn evaluate(
    p: &Parsed<'_>,
    tree: &Tree,
    line: &Line,
    xfail: bool,
    round: u64,
    env: &Env,
    places: Places,
    r: &mut TreeResult,
) -> Result<(), String> {
    same_tree(&p.real.join("dep"), &tree.dir.join("dep"))?;
    let real = std::fs::canonicalize(&tree.dir).map_err(|e| e.to_string())?;
    let real_s = real.to_str().ok_or("the tree's path is not UTF-8")?.to_owned();
    match compile(env, &real, &p.std, &p.work.join("clang-tmp"))? {
        None => r.t2_skipped = true,
        Some((accepted, diag)) => {
            let want = *line != Line::NotParsed;
            if accepted != want {
                let said = if accepted { "accepts" } else { "rejects" };
                let d = diag.lines().find(|l| l.contains("error")).unwrap_or("").to_owned();
                let msg = format!(
                    "clang {said} the tree{}",
                    if d.is_empty() { String::new() } else { format!(": {d}") }
                );
                if !xfail {
                    return Err(msg);
                }
                r.notes.push(msg);
            }
        }
    }
    let decl = declare(p, tree, &real_s, &mut r.notes)?;
    let report = p.work.join(format!("{}.json", tree.name));
    let round_s = round.to_string();
    let mut args =
        vec!["verify", "--plan", &p.plan, "--store", "store", "-j", "1", "--round", &round_s];
    let decl_s = decl.as_ref().map(|d| d.to_str().unwrap_or("").to_owned());
    if let Some(d) = &decl_s {
        args.extend(["--declared", d]);
    }
    args.extend(image(p, &real_s));
    let report_s = report.to_str().unwrap_or("").to_owned();
    args.extend(["--findings", "100000", "--locate", "100000", "--report", &report_s]);
    let out = slip(&p.work, &args)?;
    if !matches!(out.code(), Some(0 | 1 | 4)) {
        return Err(format!("slip verify: {}", out.brief()));
    }
    let bytes = std::fs::read(&report).map_err(|e| format!("the report: {e}"))?;
    let value = json::parse(&bytes).map_err(|e| format!("the report is not JSON: {e}"))?;
    let found = read_found(&value, &[real.as_path(), p.real.as_path()])?;
    let _ = std::fs::write(p.work.join(format!("{}.found.txt", tree.name)), detail(&found));
    r.found = found.describe();
    if found.state == "failed" {
        return Err(format!("the observer failed: {}", first_line(&found.message)));
    }
    let mut ask = |name: &str| step4(p, &real_s, name);
    let met = meets(line, &found, places, &mut ask);
    r.skipped = met.unwrap_or(0);
    r.outcome = outcome(tree.bad, found.verdict == "verified", met.is_some(), xfail);
    Ok(())
}

/// Every group of a tree, with its ids, for a follow-up.
fn detail(found: &Found) -> String {
    let mut s = format!("verdict {} state {} {}\n", found.verdict, found.state, found.message);
    for g in &found.groups {
        let _ =
            writeln!(s, "{} {} key {} pre {} post {}", g.kind, g.name, g.key, g.pre_id, g.post_id);
        for (side, site) in [("pre", &g.pre), ("post", &g.post)] {
            if let Some(site) = site {
                let _ = writeln!(s, "  {side} {}:{} {}", site.file, site.line, site.what);
            }
        }
    }
    s
}

/// Runs `dirs` on `THREADS` threads; results in the order of `dirs`.
fn run_rows(dirs: &[PathBuf], work: &Path, env: &Env, places: Places) -> Vec<TreeResult> {
    let next = AtomicUsize::new(0);
    let done: Mutex<Vec<(usize, Vec<TreeResult>)>> = Mutex::new(Vec::new());
    std::thread::scope(|s| {
        for _ in 0..THREADS.min(dirs.len().max(1)) {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(dir) = dirs.get(i) else { break };
                    let r = run_row(dir, work, env, places);
                    done.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push((i, r));
                }
            });
        }
    });
    let mut done = done.into_inner().unwrap_or_else(std::sync::PoisonError::into_inner);
    done.sort_by_key(|(i, _)| *i);
    done.into_iter().flat_map(|(_, r)| r).collect()
}

/// The summary of `spec/pitfalls.md` §1.
fn summary(rows: usize, parse_only: usize, results: &[TreeResult], places: Places) -> String {
    let count = |o: Outcome| results.iter().filter(|r| r.outcome == o).count();
    let mut s = format!(
        "pitfalls: {rows} rows, {} trees: PASS {} XFAIL {} FAIL {} XPASS {} ERROR {} SKIP(T2) {}\n",
        results.len(),
        count(Outcome::Pass),
        count(Outcome::Xfail),
        count(Outcome::Fail),
        count(Outcome::Xpass),
        count(Outcome::Error),
        results.iter().filter(|r| r.t2_skipped).count(),
    );
    let skipped: usize = results.iter().map(|r| r.skipped).sum();
    match places {
        Places::Mandatory => {
            let _ = writeln!(s, "pitfalls: places {}", places.name());
        }
        Places::Advisory => {
            let _ = writeln!(s, "pitfalls: places {}, {skipped} skipped", places.name());
        }
    }
    let _ = writeln!(s, "pitfalls: {parse_only} rows caught only by a failed parse");
    for r in results.iter().filter(|r| r.outcome != Outcome::Pass) {
        let _ = writeln!(
            s,
            "  {} {}/{}: {} / {}",
            r.outcome.name(),
            r.row,
            r.tree,
            r.expected,
            r.found
        );
    }
    for r in results {
        for n in r.notes.iter().filter(|n| !n.starts_with("xfail: ")) {
            let _ = writeln!(s, "pitfalls: note {}/{}: {n}", r.row, r.tree);
        }
    }
    s
}

/// Writes to the process's standard error directly, past the test
/// harness's capture: the summary is printed whether or not the test passes.
fn print_uncaptured(s: &str) {
    let mut e = std::io::stderr().lock();
    let _ = e.write_all(s.as_bytes());
    let _ = e.flush();
}

fn manifest() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// A directory of its own under the work root (`common::work_root()`).
fn work_dir(name: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let root = common::work_root().join(format!("pitfalls-{name}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("make the work directory");
    std::fs::canonicalize(&root).expect("the work directory's real path")
}

/// Every row of the catalogue (`spec/pitfalls.md` §1, "Tests").
#[test]
fn pitfalls() {
    let started = Instant::now();
    let rows_dir = manifest().join("../slip_verify/tests/pitfalls");
    let spec = std::fs::read_to_string(manifest().join("../../spec/pitfalls.md"))
        .expect("spec/pitfalls.md");
    let ids = catalogue_ids(&spec);
    let mut dirs: Vec<String> = std::fs::read_dir(&rows_dir)
        .expect("the rows' directory")
        .map(|e| e.expect("an entry"))
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    dirs.sort();
    let mut results: Vec<TreeResult> = check_catalogue(&ids, &dirs)
        .into_iter()
        .map(|(what, e)| TreeResult {
            row: what,
            tree: "-".to_owned(),
            outcome: Outcome::Error,
            expected: "the catalogue against the directories".to_owned(),
            found: e,
            skipped: 0,
            t2_skipped: false,
            notes: Vec::new(),
        })
        .collect();
    let only = std::env::var("SLIP_PITFALLS_ONLY").ok().filter(|s| !s.is_empty());
    let selected: Vec<PathBuf> = dirs
        .iter()
        .filter(|d| only.as_ref().is_none_or(|o| o.split(',').any(|o| d.contains(o))))
        .map(|d| rows_dir.join(d))
        .collect();
    let parse_only = selected.iter().filter(|d| read_row(d).is_ok_and(|r| r.parse_only())).count();
    let work = work_dir("catalogue");
    let env = Env::from_process();
    results.extend(run_rows(&selected, &work, &env, PLACES));
    let mut text = summary(selected.len(), parse_only, &results, PLACES);
    if let Some(o) = &only {
        let _ = writeln!(text, "pitfalls: partial run (SLIP_PITFALLS_ONLY={o})");
    }
    let bad = results
        .iter()
        .any(|r| matches!(r.outcome, Outcome::Fail | Outcome::Xpass | Outcome::Error));
    let keep = bad || std::env::var_os("SLIP_PITFALLS_KEEP").is_some_and(|v| v == "1");
    if keep {
        let _ = writeln!(text, "pitfalls: work directory {}", work.display());
    } else {
        let _ = std::fs::remove_dir_all(&work);
    }
    let _ = writeln!(text, "pitfalls: wall {:.3} s", started.elapsed().as_secs_f64());
    print_uncaptured(&text);
    assert!(!bad, "the pitfalls suite has a FAIL, XPASS or ERROR:\n{text}");
}

// ----- the runner's own cases (spec/pitfalls.md "Tests") --------------------------------

/// A row's name, its files and the message it must give.
type Layout<'a> = (&'a str, &'a [(&'a str, &'a str)], &'a str);

/// A row written to a temporary directory: `files` are `(path, text)`.
fn write_row(root: &Path, name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = root.join("rows").join(name);
    for (p, text) in files {
        let p = dir.join(p);
        std::fs::create_dir_all(p.parent().expect("a parent")).expect("mkdir");
        std::fs::write(&p, text).expect("write");
    }
    dir
}

#[expect(clippy::unnecessary_wraps, reason = "a site of a group is optional")]
fn site(file: &str, line: u64) -> Option<Site> {
    Some(Site { file: file.to_owned(), line, what: String::new() })
}

fn group(kind: &str, name: &str, pre: Option<Site>, post: Option<Site>) -> Group {
    Group {
        kind: kind.to_owned(),
        name: name.to_owned(),
        key: String::new(),
        pre_id: String::new(),
        post_id: String::new(),
        pre,
        post,
    }
}

fn found(verdict: &str, state: &str, groups: Vec<Group>) -> Found {
    Found { verdict: verdict.to_owned(), state: state.to_owned(), message: String::new(), groups }
}

fn line(text: &str) -> Line {
    parse_line(text).expect("a line")
}

fn never(_: &str) -> bool {
    panic!("step 4 asked when the two sides agree")
}

/// §1, the table of outcomes.
#[test]
fn outcome_table() {
    // (bad, verified, met, xfail)
    assert_eq!(outcome(false, true, true, false), Outcome::Pass);
    assert_eq!(outcome(true, false, true, false), Outcome::Pass);
    assert_eq!(outcome(true, false, false, false), Outcome::Fail);
    assert_eq!(outcome(false, false, false, false), Outcome::Fail);
    assert_eq!(outcome(true, false, false, true), Outcome::Xfail);
    assert_eq!(outcome(false, false, false, true), Outcome::Xfail);
    assert_eq!(outcome(false, true, true, true), Outcome::Xpass);
    assert_eq!(outcome(true, false, true, true), Outcome::Xpass);
}

/// §1: a `bad` tree whose verdict is `verified` is `FAIL`, with or without
/// `xfail`; end to end, a "wrong" edit that changes nothing.
#[test]
fn bad_tree_that_verifies_fails_even_with_xfail() {
    assert_eq!(outcome(true, true, false, true), Outcome::Fail);
    assert_eq!(outcome(true, true, false, false), Outcome::Fail);
    let root = work_dir("bad-verifies");
    let pre = "int f() { return 1; }\n";
    let dir = write_row(
        &root,
        "x01-bad-verifies",
        &[
            ("pre/main.cpp", pre),
            ("good/main.cpp", pre),
            ("bad/main.cpp", "int f() {\n  return 1;\n}\n"),
            ("expect.txt", "good: verified\nbad: mismatch\nxfail bad: hidden\n"),
        ],
    );
    let env = Env { cc: None, require_all: false };
    let r = run_row(&dir, &root.join("work"), &env, Places::Mandatory);
    let o: Vec<(&str, Outcome)> = r.iter().map(|r| (r.tree.as_str(), r.outcome)).collect();
    assert_eq!(o, [("good", Outcome::Pass), ("bad", Outcome::Fail)], "{r:#?}");
    let _ = std::fs::remove_dir_all(&root);
}

/// §1: every outcome of the table end to end, and `pre-2/` (the trees
/// numbered 2 are edits of it, with `declared-2.txt`... here none).
#[test]
fn outcomes_end_to_end_and_pre_2() {
    let root = work_dir("outcomes");
    let pre = "int f() { return 1; }\nint g() { return f(); }\n";
    let changed = "int f() { return 2; }\nint g() { return f(); }\n";
    // pre-2 differs from pre: good-2 is pre-2 unchanged, which against pre
    // would be a mismatch.
    let pre2 = "int f() { return 3; }\nint g() { return f(); }\n";
    let dir = write_row(
        &root,
        "x02-outcomes",
        &[
            ("pre/main.cpp", pre),
            ("good/main.cpp", pre),
            ("bad/main.cpp", changed),
            ("bad-3/main.cpp", changed),
            ("bad-4/main.cpp", changed),
            ("bad-5/main.cpp", "int f() { return 1 }\n"),
            ("pre-2/main.cpp", pre2),
            ("good-2/main.cpp", pre2),
            ("bad-2/main.cpp", pre),
            (
                "expect.txt",
                "good: verified\n\
                 bad: mismatch changed f() @ main.cpp:1\n\
                 good-2: verified\n\
                 bad-2: mismatch changed f() @ main.cpp:1\n\
                 bad-3: mismatch changed g() @ main.cpp:2\n\
                 bad-4: mismatch changed f() @ main.cpp:1\n\
                 bad-5: not_parsed\n\
                 xfail bad-3: g() does not change\n\
                 xfail bad-4: listed to show XPASS\n",
            ),
        ],
    );
    let env = Env { cc: None, require_all: false };
    let r = run_row(&dir, &root.join("work"), &env, Places::Mandatory);
    let o: BTreeMap<&str, Outcome> = r.iter().map(|r| (r.tree.as_str(), r.outcome)).collect();
    let want = BTreeMap::from([
        ("good", Outcome::Pass),
        ("bad", Outcome::Pass),
        ("bad-3", Outcome::Xfail),
        ("bad-4", Outcome::Xpass),
        ("bad-5", Outcome::Pass),
        ("good-2", Outcome::Pass),
        ("bad-2", Outcome::Pass),
    ]);
    assert_eq!(o, want, "{r:#?}");
    assert!(r.iter().all(|r| r.t2_skipped));
    // Under SLIP_REQUIRE_ALL=1 a missing compiler is an ERROR.
    let strict = Env { cc: None, require_all: true };
    let r = run_row(&dir, &root.join("work-strict"), &strict, Places::Mandatory);
    assert!(r.iter().all(|r| r.outcome == Outcome::Error), "{r:#?}");
    let _ = std::fs::remove_dir_all(&root);
}

/// §1 tier T2: the production compiler disagreeing with a line is `ERROR`.
#[test]
fn compiler_disagreement_is_an_error() {
    let Some(cc) = compiler() else {
        assert!(std::env::var_os("SLIP_REQUIRE_ALL").is_none_or(|v| v != "1"), "no clang++");
        return;
    };
    let root = work_dir("t2");
    let pre = "int f() { return 1; }\n";
    // Accepted by both: the line says not_parsed, so the compiler disagrees.
    let dir = write_row(
        &root,
        "x03-t2",
        &[
            ("pre/main.cpp", pre),
            ("good/main.cpp", pre),
            ("bad/main.cpp", "int f() { return 2; }\n"),
            ("expect.txt", "good: verified\nbad: not_parsed\n"),
        ],
    );
    let env = Env { cc: Some(cc), require_all: false };
    let r = run_row(&dir, &root.join("work"), &env, Places::Mandatory);
    let o: Vec<(&str, Outcome)> = r.iter().map(|r| (r.tree.as_str(), r.outcome)).collect();
    assert_eq!(o, [("good", Outcome::Pass), ("bad", Outcome::Error)], "{r:#?}");
    assert!(r[1].found.contains("clang accepts"), "{r:#?}");
    let _ = std::fs::remove_dir_all(&root);
}

/// §1 "`mismatch` alone": a tree whose findings are all indeterminate has
/// not shown the hazard caught.
#[test]
fn mismatch_alone_needs_a_finding_that_is_not_indeterminate() {
    let l = line("mismatch");
    let indet = found(
        "not_verified",
        "mismatch",
        vec![
            group("indeterminate", "f()", None, site("main.cpp", 1)),
            group("derived_indeterminate", "g<int>()", None, site("main.cpp", 2)),
        ],
    );
    assert_eq!(meets(&l, &indet, Places::Mandatory, &mut never), None);
    let mut with = indet.clone();
    with.groups.push(group("changed", "h()", site("main.cpp", 3), site("main.cpp", 3)));
    assert_eq!(meets(&l, &with, Places::Mandatory, &mut never), Some(0));
    // A named indeterminate line is met by its group (N20, R11).
    let named = line("mismatch indeterminate f()");
    assert_eq!(meets(&named, &indet, Places::Mandatory, &mut never), Some(0));
}

/// §1: an `unverifiable` verdict meets no line.
#[test]
fn unverifiable_meets_no_line() {
    let f = found(
        "incomplete",
        "unverifiable",
        vec![group("changed", "f()", None, site("main.cpp", 1))],
    );
    for l in [
        "verified",
        "not_parsed",
        "mismatch",
        "mismatch changed f()",
        "mismatch changed f() @ main.cpp:1",
    ] {
        for places in [Places::Advisory, Places::Mandatory] {
            assert_eq!(meets(&line(l), &f, places, &mut never), None, "{l}");
        }
    }
}

/// §1 "Places": a place at another line is not met in either state; the
/// pre side for `removed`.
#[test]
fn place_one_line_off_in_either_state() {
    let f = found(
        "not_verified",
        "mismatch",
        vec![
            group("changed", "f()", site("main.cpp", 4), site("main.cpp", 4)),
            group("removed", "g()", site("main.cpp", 7), None),
        ],
    );
    for places in [Places::Advisory, Places::Mandatory] {
        assert_eq!(
            meets(&line("mismatch changed f() @ main.cpp:4"), &f, places, &mut never),
            Some(0)
        );
        assert_eq!(meets(&line("mismatch changed f() @ main.cpp:5"), &f, places, &mut never), None);
        assert_eq!(meets(&line("mismatch changed f() @ main.cpp:3"), &f, places, &mut never), None);
        assert_eq!(meets(&line("mismatch changed f() @ other.h:4"), &f, places, &mut never), None);
        assert_eq!(
            meets(&line("mismatch removed g() @ main.cpp:7"), &f, places, &mut never),
            Some(0)
        );
        assert_eq!(meets(&line("mismatch removed g() @ main.cpp:8"), &f, places, &mut never), None);
    }
}

/// §1: a `changed` of §4 step 4 whose entity the subject declares and the
/// tree derives is placed at `where.pre`; any other `changed` at
/// `where.post`.
#[test]
fn place_of_a_step_4_changed_is_the_pre_side() {
    let f = found(
        "not_verified",
        "mismatch",
        vec![group("changed", "traits<Mine>", site("spec.h", 1), site("traits.h", 2))],
    );
    let l = line("mismatch changed traits<Mine> @ spec.h:1");
    assert_eq!(meets(&l, &f, Places::Mandatory, &mut |_| true), Some(0));
    assert_eq!(meets(&l, &f, Places::Mandatory, &mut |_| false), None);
    let post = line("mismatch changed traits<Mine> @ traits.h:2");
    assert_eq!(meets(&post, &f, Places::Mandatory, &mut |_| false), Some(0));
    assert_eq!(meets(&post, &f, Places::Mandatory, &mut |_| true), None);
}

/// §1 "Places": a finding without a `where` meets its line only in the
/// `advisory` state, and is counted there.
#[test]
fn place_absent_is_met_only_when_advisory() {
    let f = found("not_verified", "mismatch", vec![group("changed", "f()", None, None)]);
    let l = line("mismatch changed f() @ main.cpp:4");
    assert_eq!(meets(&l, &f, Places::Advisory, &mut never), Some(1));
    assert_eq!(meets(&l, &f, Places::Mandatory, &mut never), None);
    let r = TreeResult {
        row: "x".to_owned(),
        tree: "bad".to_owned(),
        outcome: Outcome::Pass,
        expected: String::new(),
        found: String::new(),
        skipped: 1,
        t2_skipped: false,
        notes: Vec::new(),
    };
    let s = summary(1, 0, std::slice::from_ref(&r), Places::Advisory);
    assert!(s.contains("pitfalls: places advisory, 1 skipped\n"), "{s}");
    let s = summary(1, 0, &[r], Places::Mandatory);
    assert!(s.contains("pitfalls: places mandatory\n"), "{s}");
}

/// §1: in a `bad` tree a `created` line that does not resolve (`NoUnit`) is
/// left out for that tree, and the runner says so; in a `good` tree it is
/// an `ERROR`.
#[test]
fn created_line_unresolved_in_a_bad_tree_is_left_out() {
    let root = work_dir("created");
    let pre = "int f() { return 1; }\n";
    let dir = write_row(
        &root,
        "x04-created",
        &[
            ("pre/main.cpp", pre),
            ("good/main.cpp", "int f() { return 1; }\nint spare() { return 2; }\n"),
            ("bad/main.cpp", "int f() { return 2; }\n"),
            ("good-2/main.cpp", pre),
            ("pre-2/main.cpp", pre),
            ("declared.txt", "created spare()\n"),
            (
                "expect.txt",
                "good: verified\nbad: mismatch changed f() @ main.cpp:1\ngood-2: verified\n",
            ),
        ],
    );
    let env = Env { cc: None, require_all: false };
    let r = run_row(&dir, &root.join("work"), &env, Places::Mandatory);
    let o: Vec<(&str, Outcome)> = r.iter().map(|r| (r.tree.as_str(), r.outcome)).collect();
    assert_eq!(
        o,
        [("good", Outcome::Pass), ("bad", Outcome::Pass), ("good-2", Outcome::Error)],
        "{r:#?}"
    );
    assert!(r[1].notes.iter().any(|n| n.contains("declared line 1 left out")), "{r:#?}");
    assert!(r[2].found.contains("slip declare"), "{r:#?}");
    let _ = std::fs::remove_dir_all(&root);
}

/// §1: `dep/` must be the same in every tree of a subject.
#[test]
fn dep_changed() {
    let root = work_dir("dep");
    let dep = "namespace legacy { inline int d() { return 1; } }\n";
    let main = "#include \"dep.h\"\nint f() { return legacy::d(); }\n";
    let dir = write_row(
        &root,
        "x05-dep",
        &[
            ("pre/main.cpp", main),
            ("pre/dep/dep.h", dep),
            ("good/main.cpp", main),
            ("good/dep/dep.h", dep),
            ("bad/main.cpp", main),
            ("bad/dep/dep.h", "namespace legacy { inline int d() { return 2; } }\n"),
            ("bad-2/main.cpp", main),
            ("expect.txt", "good: verified\nbad: mismatch\nbad-2: mismatch\n"),
        ],
    );
    let env = Env { cc: None, require_all: false };
    let r = run_row(&dir, &root.join("work"), &env, Places::Mandatory);
    let o: Vec<(&str, Outcome)> = r.iter().map(|r| (r.tree.as_str(), r.outcome)).collect();
    assert_eq!(
        o,
        [("good", Outcome::Pass), ("bad", Outcome::Error), ("bad-2", Outcome::Error)],
        "{r:#?}"
    );
    assert!(r[1].found.contains("dep/dep.h differs"), "{r:#?}");
    assert!(r[2].found.contains("dep/dep.h is missing"), "{r:#?}");
    let _ = std::fs::remove_dir_all(&root);
}

/// §1 "The catalogue against the directories": a row without a directory.
#[test]
fn row_without_directory() {
    let spec =
        "## 2. A\n\n| Id | x |\n|---|---|\n| N01 | a |\n| N02 | b |\n\n## 5. B\n| N99 | c |\n";
    assert_eq!(catalogue_ids(spec), ["N01", "N02"]);
    let e = check_catalogue(&catalogue_ids(spec), &["n01-a".to_owned()]);
    assert_eq!(e, [("N02".to_owned(), "a row without a directory".to_owned())]);
    let e = check_catalogue(&catalogue_ids(spec), &["n01-a".to_owned(), "n02-b".to_owned()]);
    assert!(e.is_empty(), "{e:?}");
}

/// §1 "The catalogue against the directories": a directory without a row,
/// two directories for one row, a name that is not `<id>-<slug>`.
#[test]
fn directory_without_row() {
    let ids = ["N01".to_owned()];
    let e = check_catalogue(&ids, &["n01-a".to_owned(), "n03-c".to_owned()]);
    assert_eq!(
        e,
        [("n03-c".to_owned(), "a directory without a row in spec/pitfalls.md".to_owned())]
    );
    let e = check_catalogue(&ids, &["n01-a".to_owned(), "n01-b".to_owned()]);
    assert_eq!(e, [("N01".to_owned(), "several directories: n01-a, n01-b".to_owned())]);
    let e = check_catalogue(&ids, &["n01-a".to_owned(), "N01_x".to_owned()]);
    assert_eq!(e, [("N01_x".to_owned(), "not `<id>-<slug>` of [a-z0-9-]".to_owned())]);
}

/// §1: a malformed `expect.txt`, and a malformed layout, are `ERROR`.
#[test]
fn malformed_expect() {
    for bad in [
        "good: verifed\n",
        "good verified\n",
        "bad: mismatch changed\n",
        "bad: mismatch moved f()\n",
        "bad: mismatch changed f() @ main.cpp\n",
        "bad: mismatch changed f() @ main.cpp:0\n",
        "good: none\n",
        "std: c++17\nstd: c++20\n",
        "xfail bad\n",
        "xfail bad: \n",
        "good: verified\ngood: verified\n",
        "bad: mismatch\nbad: mismatch changed f()\n",
        "pre: verified\n",
        "bad-1: mismatch\n",
        "bad-02: mismatch\n",
    ] {
        assert!(parse_expect(bad).is_err(), "{bad:?}");
    }
    let e = parse_expect(
        "# c\nstd: c++17\n\ngood: verified\nbad: mismatch changed f() @ main.cpp:3\n\
         bad: mismatch derived_changed a<b>::c(d &) @ dep/dep.h:4\nxfail bad: why\n",
    )
    .expect("well-formed");
    assert_eq!(e.std.as_deref(), Some("c++17"));
    assert_eq!(
        e.lines.get("bad"),
        Some(&Line::Mismatch(vec![
            Wanted {
                kind: "changed".to_owned(),
                name: "f()".to_owned(),
                at: Some(("main.cpp".to_owned(), 3))
            },
            Wanted {
                kind: "derived_changed".to_owned(),
                name: "a<b>::c(d &)".to_owned(),
                at: Some(("dep/dep.h".to_owned(), 4)),
            },
        ]))
    );
}

/// A row whose layout is malformed (`spec/pitfalls.md` §1) is one `ERROR`.
#[test]
fn malformed_layout() {
    let root = work_dir("malformed");
    let m = "int f() { return 1; }\n";
    let layouts: [Layout<'_>; 9] = [
        ("y01-no-pre", &[("good/main.cpp", m), ("expect.txt", "good: verified\n")], "no pre/"),
        (
            "y02-no-line",
            &[("pre/main.cpp", m), ("good/main.cpp", m), ("expect.txt", "")],
            "no line",
        ),
        ("y03-no-dir", &[("pre/main.cpp", m), ("expect.txt", "good: verified\n")], "no directory"),
        (
            "y04-none-beside",
            &[("pre/main.cpp", m), ("good/main.cpp", m), ("expect.txt", "good: none x\n")],
            "beside",
        ),
        (
            "y05-declared-k",
            &[
                ("pre/main.cpp", m),
                ("good/main.cpp", m),
                ("declared-2.txt", ""),
                ("expect.txt", "good: verified\n"),
            ],
            "declared-2.txt has no pre-2/",
        ),
        (
            "y06-pre-k",
            &[
                ("pre/main.cpp", m),
                ("pre-2/main.cpp", m),
                ("bad-2/main.cpp", m),
                ("expect.txt", "bad-2: mismatch\n"),
            ],
            "neither good-2/",
        ),
        (
            "y07-entry",
            &[("pre/main.cpp", m), ("notes.md", ""), ("expect.txt", "")],
            "unexpected entry",
        ),
        (
            "y08-xfail",
            &[("pre/main.cpp", m), ("expect.txt", "good: none x\nxfail good: y\n")],
            "`xfail good`",
        ),
        (
            "y09-bad-verified",
            &[
                ("pre/main.cpp", m),
                ("bad/main.cpp", m),
                ("expect.txt", "good: none x\nbad: verified\n"),
            ],
            "a bad tree that verifies",
        ),
    ];
    let env = Env { cc: None, require_all: false };
    for (name, files, msg) in layouts {
        let dir = write_row(&root, name, files);
        let r = run_row(&dir, &root.join("work"), &env, Places::Mandatory);
        assert_eq!(r.len(), 1, "{name}: {r:#?}");
        assert_eq!(r[0].outcome, Outcome::Error, "{name}");
        assert!(r[0].found.contains(msg), "{name}: {}", r[0].found);
    }
    let _ = std::fs::remove_dir_all(&root);
}

/// The summary's lines (`spec/pitfalls.md` §1).
#[test]
fn summary_lines() {
    let mk = |tree: &str, outcome| TreeResult {
        row: "n01-x".to_owned(),
        tree: tree.to_owned(),
        outcome,
        expected: "mismatch".to_owned(),
        found: "verdict incomplete, unverifiable".to_owned(),
        skipped: 0,
        t2_skipped: true,
        notes: Vec::new(),
    };
    let s =
        summary(1, 1, &[mk("good", Outcome::Pass), mk("bad", Outcome::Xfail)], Places::Mandatory);
    assert_eq!(
        s,
        "pitfalls: 1 rows, 2 trees: PASS 1 XFAIL 1 FAIL 0 XPASS 0 ERROR 0 SKIP(T2) 2\n\
         pitfalls: places mandatory\n\
         pitfalls: 1 rows caught only by a failed parse\n\
         \x20 XFAIL n01-x/bad: mismatch / verdict incomplete, unverifiable\n"
    );
}

/// §1 `xfail`: an `ERROR` from `slip declare` refusing the declared file is
/// covered only by a reason that quotes its class or a word it quotes.
#[test]
fn xfail_covers_a_declare_refusal_only_when_it_quotes_it() {
    let e = format!("{REFUSED}line 4: syntax: unknown keyword `kept`");
    assert!(covers("WP-111: the keyword `kept`", &e));
    assert!(covers("WP-111: a `syntax` error", &e));
    assert!(!covers("WP-111: kept", &e));
    assert!(!covers("WP-111: `no unit`", &e));
    assert!(!covers("`kept`", "slip verify: status Some(3): slip: kept"));
    assert!(!covers("`syntax`", "slip declare: status Some(3): syntax"));

    let root = work_dir("refusal");
    let m = "int f() { return 1; }\n";
    let env = Env { cc: None, require_all: false };
    for (name, reason, want) in [
        ("z01-quoted", "WP-111: `frobnicate`", Outcome::Xfail),
        ("z02-unrelated", "WP-111: something else", Outcome::Error),
    ] {
        let expect = format!("good: verified\nxfail good: {reason}\n");
        let dir = write_row(
            &root,
            name,
            &[
                ("pre/main.cpp", m),
                ("good/main.cpp", m),
                ("declared.txt", "frobnicate f()\n"),
                ("expect.txt", &expect),
            ],
        );
        let r = run_row(&dir, &root.join("work"), &env, Places::Mandatory);
        assert_eq!(r.len(), 1, "{r:#?}");
        assert_eq!(r[0].outcome, want, "{name}: {r:#?}");
        assert!(r[0].found.starts_with(REFUSED), "{name}: {}", r[0].found);
    }
    let _ = std::fs::remove_dir_all(&root);
}

/// WP-167 (`spec/pitfalls.md` §1, tier T2): `compile` isolates `TMPDIR`
/// inside the caller's scratch directory and passes `-fno-crash-diagnostics`.
#[test]
fn wp167_compile_isolates_tmp_and_disables_crash_diagnostics() {
    use std::os::unix::fs::PermissionsExt as _;
    let root = work_dir("wp167-tmp");
    let tree = root.join("tree");
    let tmp = root.join("clang-tmp");
    std::fs::create_dir_all(&tree).unwrap();
    std::fs::write(tree.join("main.cpp"), "int f() { return 0; }\n").unwrap();
    let fake_cc = root.join("fake-clang");
    std::fs::write(
        &fake_cc,
        "#!/bin/sh\n\
         no_crash=\n\
         for a in \"$@\"; do [ \"$a\" = -fno-crash-diagnostics ] && no_crash=1; done\n\
         [ -n \"$no_crash\" ] || { echo 'missing -fno-crash-diagnostics' >&2; exit 2; }\n\
         : > \"$TMPDIR/main-abcdef.cpp\"\n\
         exit 0\n",
    )
    .unwrap();
    std::fs::set_permissions(&fake_cc, std::fs::Permissions::from_mode(0o755)).unwrap();
    let env = Env { cc: Some(fake_cc), require_all: false };
    let res = compile(&env, &tree, "c++20", &tmp).unwrap();
    assert_eq!(res, Some((true, String::new())));
    assert!(tmp.join("main-abcdef.cpp").is_file(), "marker was not written into isolated TMPDIR");
    let _ = std::fs::remove_dir_all(&root);
}

/// WP-171 item 2: a compiler crash (exit status other than 0/1, termination by
/// signal, or clang crash text on stderr) fails the pitfalls run as `ERROR`,
/// naming the tree — even when the tree is expected `not_parsed` or `xfail`.
#[test]
fn wp171_compiler_crash_fails_the_run() {
    use std::os::unix::fs::PermissionsExt as _;
    let root = work_dir("wp171-crash");
    let pre = "int f() { return 1; }\n";
    let bad_syntax = "int f() { return 1 }\n";
    let dir = write_row(
        &root,
        "x06-crash",
        &[
            ("pre/main.cpp", pre),
            ("good/main.cpp", pre),
            ("bad/main.cpp", bad_syntax),
            ("bad-2/main.cpp", bad_syntax),
            (
                "expect.txt",
                "good: verified\nbad: not_parsed\nbad-2: not_parsed\nxfail bad-2: allowed\n",
            ),
        ],
    );
    let make_cc = |name: &str, bad_body: &str| {
        let p = root.join(name);
        let script = format!(
            "#!/bin/sh\n\
             case \"$PWD\" in\n\
               */bad|*/bad-2)\n\
                 {bad_body}\n\
                 ;;\n\
             esac\n\
             exit 0\n"
        );
        std::fs::write(&p, script).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p
    };

    // Control: clean exit 1 without crash text agrees with `not_parsed`.
    let clean_reject = make_cc("cc-reject", "echo 'main.cpp:1:20: error: expected ;' >&2; exit 1");
    let env = Env { cc: Some(clean_reject), require_all: false };
    let r = run_row(&dir, &root.join("work-reject"), &env, Places::Mandatory);
    let o: Vec<(&str, Outcome)> = r.iter().map(|r| (r.tree.as_str(), r.outcome)).collect();
    assert_eq!(
        o,
        [("good", Outcome::Pass), ("bad", Outcome::Pass), ("bad-2", Outcome::Xpass)],
        "{r:#?}"
    );

    // Every crash mode must produce Outcome::Error naming the tree, even for
    // `not_parsed` and `xfail`.
    for (tag, body, needle) in [
        ("exit139", "echo 'Segmentation fault' >&2; exit 139", "exit 139"),
        (
            "bug-report",
            "echo 'PLEASE submit a bug report to https://github.com/llvm/llvm-project/issues/' >&2; exit 1",
            "PLEASE submit a bug report",
        ),
        (
            "signal-text",
            "echo 'clang: error: clang frontend command failed due to signal' >&2; exit 1",
            "frontend command failed due to signal",
        ),
        ("sigsegv", "kill -SEGV $$", "killed by signal"),
    ] {
        let cc = make_cc(&format!("cc-{tag}"), body);
        let env = Env { cc: Some(cc), require_all: false };
        let r = run_row(&dir, &root.join(format!("work-{tag}")), &env, Places::Mandatory);
        let o: Vec<(&str, Outcome)> = r.iter().map(|r| (r.tree.as_str(), r.outcome)).collect();
        assert_eq!(
            o,
            [("good", Outcome::Pass), ("bad", Outcome::Error), ("bad-2", Outcome::Error)],
            "{tag}: {r:#?}"
        );
        for res in &r[1..] {
            assert!(
                res.found.contains("clang crashed on")
                    && res.found.contains(&res.tree)
                    && res.found.contains(needle),
                "{tag}: unexpected error for {}: {}",
                res.tree,
                res.found
            );
        }
    }
    let _ = std::fs::remove_dir_all(&root);
}
