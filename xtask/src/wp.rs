//! `cargo xtask wp [--ready]` (`spec/harness.md` §4), and the reader of
//! work-package files (`docs/wp/README.md` "File format") that the `wp`
//! lint shares.
//!
//! A work-package file starts with these lines, in this order:
//!
//! ```text
//! # WP-NNN — <title>
//!
//! - Status: todo | in-progress | review | done | dropped
//! - Milestone: M<digits>
//! - Track: A … H
//! - Needs: WP-NNN, WP-NNN | none
//! - Size: S | M | L
//! - Parallel with: …        (optional, not checked)
//! ```
//!
//! An id is `WP-` and three or more digits.

use crate::lint::Violation;
use crate::repo::Repo;
use std::path::Path;
use std::process::ExitCode;

/// The directory of work-package files.
pub const DIR: &str = "docs/wp";
/// The allowed statuses.
pub const STATUSES: &[&str] = &["todo", "in-progress", "review", "done", "dropped"];
/// The allowed sizes.
pub const SIZES: &[&str] = &["S", "M", "L"];
/// The allowed tracks (`docs/design/06-engineering-plan.md` §6).
pub const TRACKS: &[&str] = &["A", "B", "C", "D", "E", "F", "G", "H"];

/// The header of one work-package file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkPackage {
    pub id: String,
    pub title: String,
    pub status: String,
    pub milestone: String,
    pub track: String,
    /// The ids it needs; empty for `none`.
    pub needs: Vec<String>,
    pub size: String,
}

/// `s` if it is a work-package id (`WP-` and three or more digits).
pub fn parse_id(s: &str) -> Option<String> {
    let digits = s.strip_prefix("WP-")?;
    (digits.len() >= 3 && digits.bytes().all(|b| b.is_ascii_digit())).then(|| s.to_owned())
}

/// The listed work-package files: `docs/wp/WP-*.md`.
pub fn files(repo: &Repo) -> Vec<&str> {
    repo.files_in(DIR, ".md").filter(|f| f[DIR.len() + 1..].starts_with("WP-")).collect()
}

/// Reads the work-package file `path` with contents `text`; `exists`
/// says whether a work package id has a file. Every problem is a `wp`
/// violation.
pub fn parse(
    path: &str,
    text: &str,
    exists: &dyn Fn(&str) -> bool,
) -> Result<WorkPackage, Vec<Violation>> {
    const LINT: &str = "wp";
    let bad = |line: usize, msg: String| Violation::new(path, Some(line), LINT, msg);
    let mut problems = Vec::new();
    let name = path.rsplit('/').next().unwrap_or(path);
    let Some(id) = name.strip_suffix(".md").and_then(parse_id) else {
        return Err(vec![Violation::new(path, None, LINT, "file name is not `WP-NNN.md`")]);
    };
    let lines: Vec<&str> = text.lines().collect();
    let prefix = format!("# {id} — ");
    let first = lines.first().copied().unwrap_or("");
    let title = match first.strip_prefix(&prefix) {
        Some(t) if !t.trim().is_empty() => t.trim().to_owned(),
        _ => {
            problems.push(bad(1, format!("the first line is not `{prefix}<title>`")));
            String::new()
        }
    };
    if lines.get(1).is_some_and(|l| !l.trim().is_empty()) {
        problems.push(bad(2, "the second line is not blank".to_owned()));
    }
    // Lines 3 to 7: the five keys, in order. `None` where the line is not
    // the key's (already reported).
    let mut values: Vec<Option<String>> = Vec::new();
    for (i, key) in ["Status", "Milestone", "Track", "Needs", "Size"].into_iter().enumerate() {
        let n = i + 3;
        let line = lines.get(i + 2).copied().unwrap_or("");
        let value = line.strip_prefix(&format!("- {key}:")).map(|v| v.trim().to_owned());
        if value.is_none() {
            problems.push(bad(n, format!("line {n} is not `- {key}: <value>`")));
        }
        values.push(value);
    }
    let mut check = |n: usize, key: &str, allowed: &str, ok: fn(&str) -> bool| {
        let value = values[n - 3].clone();
        if let Some(v) = &value
            && !ok(v)
        {
            problems.push(bad(n, format!("`{key}` is `{v}`, not {allowed}")));
        }
        value.unwrap_or_default()
    };
    let status = check(3, "Status", "one of todo, in-progress, review, done, dropped", |v| {
        STATUSES.contains(&v)
    });
    let milestone = check(4, "Milestone", "`M` and a number", |v| {
        v.strip_prefix('M').is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()))
    });
    let track = check(5, "Track", "one of A to H", |v| TRACKS.contains(&v));
    let needs = check(6, "Needs", "`none` or work-package ids", |_| true);
    let size = check(7, "Size", "one of S, M, L", |v| SIZES.contains(&v));
    let mut need_ids = Vec::new();
    if values[3].is_some() && needs != "none" {
        for part in needs.split(',').map(str::trim) {
            match parse_id(part) {
                Some(n) if exists(&n) => need_ids.push(n),
                Some(n) => {
                    problems
                        .push(bad(6, format!("`Needs` names {n}, which has no file in {DIR}/")));
                }
                None => problems.push(bad(
                    6,
                    format!("`Needs` entry `{part}` is not a work-package id or `none`"),
                )),
            }
        }
    }
    if problems.is_empty() {
        Ok(WorkPackage { id, title, status, milestone, track, needs: need_ids, size })
    } else {
        Err(problems)
    }
}

/// Every work package of `repo`, ordered by id; `Err` with every problem
/// if any file does not follow the format.
pub fn load(repo: &Repo) -> Result<Vec<WorkPackage>, Vec<Violation>> {
    let files = files(repo);
    let exists = |id: &str| repo.is_file(&format!("{DIR}/{id}.md"));
    let mut wps = Vec::new();
    let mut problems = Vec::new();
    for f in files {
        let text = repo.read(f).unwrap_or_default();
        match parse(f, &text, &exists) {
            Ok(wp) => wps.push(wp),
            Err(mut p) => problems.append(&mut p),
        }
    }
    if !problems.is_empty() {
        problems.sort_by(|a, b| (&a.path, a.line).cmp(&(&b.path, b.line)));
        return Err(problems);
    }
    wps.sort_by(|a, b| id_key(&a.id).cmp(&id_key(&b.id)));
    Ok(wps)
}

/// Orders ids by number (`WP-999` before `WP-1000`).
fn id_key(id: &str) -> (usize, &str) {
    (id.len(), id)
}

/// The work packages with status `todo` whose needs are all `done`.
pub fn ready(wps: &[WorkPackage]) -> Vec<&WorkPackage> {
    let done = |id: &String| wps.iter().any(|w| &w.id == id && w.status == "done");
    wps.iter().filter(|w| w.status == "todo" && w.needs.iter().all(done)).collect()
}

/// The table §4 prints: a header row, then one row per work package, with
/// columns id, status, milestone, track, size, needs, title, padded to
/// their widest cell and separated by two spaces. `needs` is the ids
/// joined by `, `, or `none`.
pub fn table(wps: &[&WorkPackage]) -> String {
    let header = ["id", "status", "milestone", "track", "size", "needs", "title"];
    let mut rows: Vec<[String; 7]> = vec![header.map(str::to_owned)];
    for w in wps {
        let needs = if w.needs.is_empty() { "none".to_owned() } else { w.needs.join(", ") };
        rows.push([
            w.id.clone(),
            w.status.clone(),
            w.milestone.clone(),
            w.track.clone(),
            w.size.clone(),
            needs,
            w.title.clone(),
        ]);
    }
    let mut widths = [0usize; 7];
    for r in &rows {
        for (w, c) in widths.iter_mut().zip(r) {
            *w = (*w).max(c.chars().count());
        }
    }
    let mut out = String::new();
    for r in &rows {
        let mut line = String::new();
        for (i, c) in r.iter().enumerate() {
            if i + 1 == r.len() {
                line.push_str(c);
            } else {
                line.push_str(c);
                line.extend(std::iter::repeat_n(' ', widths[i] - c.chars().count() + 2));
            }
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

/// `cargo xtask wp [--ready]`.
pub fn main(root: &Path, args: &[String]) -> ExitCode {
    const USAGE: &str = "usage: cargo xtask wp [--ready]";
    let ready_only = match args {
        [] => false,
        [a] if a == "--ready" => true,
        _ => {
            eprintln!("xtask wp: unexpected arguments `{}`\n{USAGE}", args.join(" "));
            return ExitCode::from(2);
        }
    };
    let repo = match Repo::open(root) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("xtask wp: {e}");
            return ExitCode::from(1);
        }
    };
    match load(&repo) {
        Ok(wps) => {
            let shown: Vec<&WorkPackage> =
                if ready_only { ready(&wps) } else { wps.iter().collect() };
            print!("{}", table(&shown));
            ExitCode::SUCCESS
        }
        Err(problems) => {
            for p in problems {
                eprintln!("{p}");
            }
            ExitCode::from(1)
        }
    }
}
