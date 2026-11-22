//! The `doc-links` lint (`spec/harness.md` §3): every relative Markdown
//! link in `docs/`, `spec/` and `README.md` points at an existing file.
//! Link extraction follows Kiln's `xtask/src/doclinks.rs`.
//!
//! The documents are the listed `*.md` files under `docs/` and `spec/` and
//! the root `README.md`. A link is the target of an inline link or image
//! (`[text](target)`, `![alt](target "title")`, `<target>` accepted) or of
//! a reference definition (`[label]: target`), outside fenced code blocks
//! and inline code spans. A target with a scheme (`://`, `mailto:`), one
//! that is only an anchor (`#x`) and one that starts with `/` is not
//! relative and is not checked. Of a checked target, the `#anchor` and
//! `?query` are dropped and `%20` is decoded; it is resolved against the
//! document's directory and must name a listed file, a directory holding
//! listed files, or a path inside a submodule that exists on disk.
//! Anchors are not checked.

use super::{Violation, prose_lines};
use crate::repo::Repo;
use std::path::Path;

/// `doc-links`.
pub fn check(repo: &Repo) -> Vec<Violation> {
    const LINT: &str = "doc-links";
    let docs: Vec<&str> = repo
        .files()
        .filter(|f| {
            Path::new(f).extension().is_some_and(|e| e == "md")
                && (*f == "README.md" || f.starts_with("docs/") || f.starts_with("spec/"))
        })
        .collect();
    let mut out = Vec::new();
    for doc in docs {
        let Some(text) = repo.read(doc) else { continue };
        for (n, target) in links(&text) {
            if target.is_empty()
                || target.starts_with('#')
                || target.starts_with('/')
                || target.contains("://")
                || target.starts_with("mailto:")
            {
                continue;
            }
            let path = target.split(['#', '?']).next().unwrap_or("").replace("%20", " ");
            if path.is_empty() {
                continue;
            }
            let ok = match resolve(doc, &path) {
                None => false,
                Some(r) => {
                    repo.is_file(&r)
                        || repo.is_dir(&r)
                        || (repo.in_submodule(&r) && repo.root().join(&r).exists())
                }
            };
            if !ok {
                out.push(Violation::new(
                    doc,
                    Some(n),
                    LINT,
                    format!("link `{target}` points at no file"),
                ));
            }
        }
    }
    out
}

/// The link targets of `text`, each with its line.
pub fn links(text: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for (n, line) in prose_lines(text) {
        // A reference definition: up to three spaces, `[label]:`, target.
        let t = line.trim_start();
        if line.len() - t.len() <= 3
            && t.starts_with('[')
            && let Some(end) = t.find("]:")
            && !t[1..end].contains(']')
        {
            if let Some(target) = t[end + 2..].split_whitespace().next() {
                out.push((n, strip_angles(target).to_owned()));
            }
            continue;
        }
        // Outside inline code: the even-numbered backtick-separated pieces.
        for (i, piece) in line.split('`').enumerate() {
            if i % 2 == 1 {
                continue;
            }
            for (at, _) in piece.match_indices("](") {
                let rest = &piece[at + 2..];
                let target = if let Some(inner) = rest.strip_prefix('<') {
                    inner.split('>').next().unwrap_or("")
                } else {
                    let len = rest.find(|c: char| c == ')' || c.is_whitespace()).unwrap_or(0);
                    &rest[..len]
                };
                if !target.is_empty() {
                    out.push((n, target.to_owned()));
                }
            }
        }
    }
    out
}

fn strip_angles(t: &str) -> &str {
    t.strip_prefix('<').and_then(|t| t.strip_suffix('>')).unwrap_or(t)
}

/// `rel` resolved against the directory of `from` (both relative to the
/// root, `/`-separated), without a trailing `/`; `None` if it climbs above
/// the root.
pub fn resolve(from: &str, rel: &str) -> Option<String> {
    let mut parts: Vec<&str> = from.split('/').collect();
    parts.pop();
    for c in rel.split('/') {
        match c {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            c => parts.push(c),
        }
    }
    Some(parts.join("/"))
}
