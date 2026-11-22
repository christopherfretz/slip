//! The specification lints of `spec/harness.md` §3: `spec-index`,
//! `spec-shape` and `landed`. A *spec* is a listed `spec/*.md` other than
//! `spec/README.md` (files in subdirectories of `spec/`, such as
//! `spec/landed/`, are not specs).

use super::{Violation, prose_lines};
use crate::repo::Repo;
use crate::wp::parse_id;
use std::collections::BTreeSet;

const INDEX: &str = "spec/README.md";
const STATUSES: &[&str] = &["not written", "draft", "stable"];

fn specs(repo: &Repo) -> impl Iterator<Item = &str> {
    repo.files_in("spec", ".md").filter(|f| *f != INDEX)
}

/// `spec-index`. The index is the first table after the heading
/// `## Index`, up to the next `## ` heading; its first column names the
/// file (a link's target, or the plain text) relative to `spec/`, its last
/// column is the status. A parenthesised note after the status
/// (`not written (M1)`) is not part of it.
pub fn spec_index(repo: &Repo) -> Vec<Violation> {
    const LINT: &str = "spec-index";
    let Some(text) = repo.read(INDEX) else {
        return vec![Violation::new(INDEX, None, LINT, "spec/README.md is missing")];
    };
    let lines = prose_lines(&text);
    let Some(start) = lines.iter().position(|(_, l)| l.trim_end() == "## Index") else {
        return vec![Violation::new(INDEX, None, LINT, "no `## Index` heading")];
    };
    let mut out = Vec::new();
    let mut rows = BTreeSet::new();
    let mut table_lines = 0;
    for &(n, line) in &lines[start + 1..] {
        if line.starts_with("## ") {
            break;
        }
        let t = line.trim();
        if !t.starts_with('|') {
            if table_lines > 0 {
                break;
            }
            continue;
        }
        table_lines += 1;
        // The header row, then the delimiter row.
        if table_lines <= 2 {
            continue;
        }
        let cells: Vec<&str> = t.trim_matches('|').split('|').map(str::trim).collect();
        let file = cell_file(cells.first().copied().unwrap_or(""));
        let raw = cells.last().copied().unwrap_or("");
        let status = raw.split_once(" (").map_or(raw, |(s, _)| s).trim();
        if !STATUSES.contains(&status) {
            out.push(Violation::new(
                INDEX,
                Some(n),
                LINT,
                format!("status `{raw}` is not `not written`, `draft` or `stable`"),
            ));
        }
        let path = format!("spec/{file}");
        if status != "not written" && !repo.is_file(&path) {
            out.push(Violation::new(
                INDEX,
                Some(n),
                LINT,
                format!("row `{file}` has status `{status}` but {path} does not exist"),
            ));
        }
        rows.insert(path);
    }
    if table_lines == 0 {
        out.push(Violation::new(INDEX, None, LINT, "no table under `## Index`"));
    }
    for spec in specs(repo) {
        if !rows.contains(spec) {
            out.push(Violation::new(spec, None, LINT, "has no row in spec/README.md's index"));
        }
    }
    out
}

/// The file a first cell names: `[x.md](x.md)` → the target, else the
/// text without backticks.
fn cell_file(cell: &str) -> String {
    if let Some((_, rest)) = cell.split_once("](")
        && let Some((target, _)) = rest.split_once(')')
    {
        return target.trim().to_owned();
    }
    cell.trim_matches('`').to_owned()
}

/// `spec-shape`: the headings must be lines of their own, outside fenced
/// code.
pub fn spec_shape(repo: &Repo) -> Vec<Violation> {
    let mut out = Vec::new();
    for spec in specs(repo) {
        let text = repo.read(spec).unwrap_or_default();
        let lines = prose_lines(&text);
        for heading in ["## Tests", "## Open questions"] {
            if !lines.iter().any(|(_, l)| l.trim_end() == heading) {
                out.push(Violation::new(
                    spec,
                    None,
                    "spec-shape",
                    format!("missing the heading `{heading}`"),
                ));
            }
        }
    }
    out
}

/// `landed`: each listed file under `spec/landed/`.
pub fn landed(repo: &Repo) -> Vec<Violation> {
    const LINT: &str = "landed";
    let mut out = Vec::new();
    for file in repo.files().filter(|f| f.starts_with("spec/landed/")) {
        let rest = &file["spec/landed/".len()..];
        let parts: Vec<&str> = rest.split('/').collect();
        let id = parts.get(1).and_then(|n| n.strip_suffix(".md")).and_then(parse_id);
        let ([spec, _], Some(id)) = (parts.as_slice(), id) else {
            out.push(Violation::new(file, None, LINT, "is not `spec/landed/<spec>/WP-NNN.md`"));
            continue;
        };
        let spec = *spec;
        if !repo.is_file(&format!("spec/{spec}.md")) || spec == "README" {
            out.push(Violation::new(
                file,
                None,
                LINT,
                format!("names spec `{spec}`, but spec/{spec}.md does not exist"),
            ));
        }
        let title = format!("# {id} — ");
        let first = repo.read(file).unwrap_or_default();
        let first = first.lines().next().unwrap_or("");
        if !first.starts_with(&title) {
            out.push(Violation::new(file, Some(1), LINT, format!("does not start with `{title}`")));
        }
        let wp = format!("docs/wp/{id}.md");
        if !repo.is_file(&wp) {
            out.push(Violation::new(file, None, LINT, format!("{wp} does not exist")));
        }
    }
    out
}
