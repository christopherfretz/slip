//! The crate lints of `spec/harness.md` §3: `kiln-boundary`,
//! `forbid-unsafe` and `dependencies` (with the allow list of §3.1).
//!
//! The workspace members are the directories matched by the root
//! `Cargo.toml`'s `[workspace] members` (a `*` matches within one path
//! component) and not by its `exclude`, that hold a listed `Cargo.toml`;
//! the root is one too when its manifest has a `[package]`.

use super::Violation;
use super::manifest::{self, Entry};
use crate::repo::Repo;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::Path;

/// The only manifest that may name a `kiln_*` dependency.
pub const KILN_ADAPTER_MANIFEST: &str = "crates/slip_kiln/Cargo.toml";

/// The crates that may depend on `slip_kiln` (and `slip_kiln` itself): the
/// crates that link Kiln.
pub const KILN_LINKING: &[&str] = &["slip_kiln", "slip_digest", "slip_script", "slip_cli"];

/// The modules of `slip_kiln` that name Kiln's types, and the files
/// outside `crates/slip_kiln/` that may name them (`kiln-boundary`, third
/// rule): the prefixes of the paths allowed, and for `snippet` also the
/// `tests/` of every crate in [`KILN_LINKING`].
pub const KILN_MODULE_USERS: &[&str] = &["crates/slip_digest/", "crates/slip_script/"];

/// §3.1: the third-party crates allowed in `Cargo.lock`, any version.
pub const ALLOWED_CRATES: &[&str] = &["signal-hook", "libc", "signal-hook-registry"];

/// Kiln's lock file, whose third-party crates are allowed at the versions
/// it lists (§3.1).
pub const KILN_LOCK: &str = "kiln/Cargo.lock";

/// One workspace member.
#[derive(Debug)]
struct Member {
    /// Its directory (`""` for the root).
    dir: String,
    /// Its `Cargo.toml`.
    manifest: String,
    /// `[package] name`, if there is one.
    name: Option<String>,
    deps: Vec<Dep>,
}

/// One dependency named in a manifest.
#[derive(Debug)]
struct Dep {
    /// The crate's name (the `package` of a renamed dependency).
    name: String,
    line: usize,
}

fn members(repo: &Repo) -> Vec<Member> {
    let Some(text) = repo.read("Cargo.toml") else { return Vec::new() };
    let (headers, entries) = manifest::parse(&text);
    let list = |key: &str| -> Vec<String> {
        entries
            .iter()
            .find(|e| e.table == "workspace" && e.key == key)
            .map(|e| manifest::strings(&e.value))
            .unwrap_or_default()
    };
    let (include, exclude) = (list("members"), list("exclude"));
    let mut dirs: Vec<String> = repo
        .files()
        .filter_map(|f| f.strip_suffix("/Cargo.toml"))
        .filter(|d| include.iter().any(|p| glob(p, d)) && !exclude.iter().any(|p| glob(p, d)))
        .map(str::to_owned)
        .collect();
    if headers.iter().any(|h| h.table == "package") {
        dirs.insert(0, String::new());
    }
    dirs.into_iter()
        .filter_map(|dir| {
            let manifest =
                if dir.is_empty() { "Cargo.toml".to_owned() } else { format!("{dir}/Cargo.toml") };
            let text = repo.read(&manifest)?;
            let (headers, entries) = manifest::parse(&text);
            let name = entries
                .iter()
                .find(|e| e.table == "package" && e.key == "name")
                .and_then(|e| manifest::strings(&e.value).into_iter().next());
            let deps = dependencies_of(&headers, &entries);
            Some(Member { dir, manifest, name, deps })
        })
        .collect()
}

/// Whether `table` is a dependency table: `[dependencies]`,
/// `[dev-dependencies]`, `[build-dependencies]`, or one of those under
/// `[target.<cfg>]`. `[workspace.dependencies]` declares, it does not
/// depend, and is not one.
fn is_dep_table(table: &str) -> bool {
    let last = table.rsplit('.').next().unwrap_or(table);
    let kind = matches!(last, "dependencies" | "dev-dependencies" | "build-dependencies");
    kind && (table == last || table.starts_with("target."))
}

fn dependencies_of(headers: &[manifest::Header], entries: &[Entry]) -> Vec<Dep> {
    let mut deps = Vec::new();
    for e in entries.iter().filter(|e| is_dep_table(&e.table)) {
        // `a = ...`, `a.workspace = true`, `a = { package = "b" }`.
        let key = e.key.split('.').next().unwrap_or(&e.key);
        let name = manifest::inline_string(&e.value, "package").unwrap_or_else(|| key.to_owned());
        deps.push(Dep { name, line: e.line });
    }
    // `[dependencies.a]`, possibly with `package = "b"`.
    for h in headers {
        let Some((table, key)) = h.table.rsplit_once('.') else { continue };
        if !is_dep_table(table) {
            continue;
        }
        let key = key.trim_matches(|c| c == '"' || c == '\'');
        let name = entries
            .iter()
            .find(|e| e.table == h.table && e.key == "package")
            .and_then(|e| manifest::strings(&e.value).into_iter().next())
            .unwrap_or_else(|| key.to_owned());
        deps.push(Dep { name, line: h.line });
    }
    deps.sort_by_key(|d| d.line);
    deps
}

/// `pattern` matches `path` component by component; `*` in a component
/// matches any run of characters within it.
fn glob(pattern: &str, path: &str) -> bool {
    let p: Vec<&str> = pattern.trim_end_matches('/').split('/').collect();
    let s: Vec<&str> = path.split('/').collect();
    p.len() == s.len() && p.iter().zip(&s).all(|(p, s)| glob_component(p, s))
}

fn glob_component(p: &str, s: &str) -> bool {
    match p.split_once('*') {
        None => p == s,
        Some((head, tail)) => {
            let Some(rest) = s.strip_prefix(head) else { return false };
            (0..=rest.len())
                .filter(|&i| rest.is_char_boundary(i))
                .any(|i| glob_component(tail, &rest[i..]))
        }
    }
}

/// `kiln-boundary`.
pub fn kiln_boundary(repo: &Repo) -> Vec<Violation> {
    const LINT: &str = "kiln-boundary";
    let members = members(repo);
    let mut out = Vec::new();
    for m in &members {
        if m.manifest == KILN_ADAPTER_MANIFEST {
            continue;
        }
        for d in m.deps.iter().filter(|d| d.name.starts_with("kiln_")) {
            out.push(Violation::new(
                &m.manifest,
                Some(d.line),
                LINT,
                format!("names Kiln crate `{}`; only {KILN_ADAPTER_MANIFEST} may", d.name),
            ));
        }
    }
    // The member graph, by package name.
    let by_name: BTreeMap<&str, &Member> =
        members.iter().filter_map(|m| Some((m.name.as_deref()?, m))).collect();
    for m in &members {
        let Some(name) = m.name.as_deref() else { continue };
        if KILN_LINKING.contains(&name) {
            continue;
        }
        if let Some(path) = path_to(&by_name, name, "slip_kiln") {
            let first = &path[1];
            let line = m.deps.iter().find(|d| d.name == *first).map(|d| d.line);
            out.push(Violation::new(
                &m.manifest,
                line,
                LINT,
                format!(
                    "`{name}` depends on `slip_kiln` ({}); only slip_digest, slip_script and \
                     slip_cli may",
                    path.join(" -> ")
                ),
            ));
        }
    }
    out.extend(kiln_modules(repo));
    out
}

/// `kiln-boundary`'s third rule: outside `crates/slip_kiln/`, the paths
/// `slip_kiln::kiln`, `slip_kiln::snippet` and `slip_kiln::ranges` (or a
/// `use slip_kiln::{…}` group naming one; `ranges` takes Kiln's source
/// manager and locations, which a caller can reach without naming
/// `kiln`, by inference) appear only in the `.rs` files that may name them.
fn kiln_modules(repo: &Repo) -> Vec<Violation> {
    let mut out = Vec::new();
    for path in repo.files().filter(|f| {
        Path::new(f).extension().is_some_and(|e| e == "rs") && !f.starts_with("crates/slip_kiln/")
    }) {
        let users = KILN_MODULE_USERS.iter().any(|p| path.starts_with(p));
        let tests = KILN_LINKING.iter().any(|c| path.starts_with(&format!("crates/{c}/tests/")));
        if users {
            continue;
        }
        let Some(text) = repo.read(path) else { continue };
        for (module, line) in kiln_module_paths(&text) {
            if module == "snippet" && tests {
                continue;
            }
            let allowed = if module == "snippet" {
                "crates/slip_digest/, crates/slip_script/ and the tests/ of slip_kiln, \
                 slip_digest, slip_script and slip_cli may"
            } else {
                "crates/slip_digest/ and crates/slip_script/ may"
            };
            out.push(Violation::new(
                path,
                Some(line),
                "kiln-boundary",
                format!("names `slip_kiln::{module}`; only {allowed}"),
            ));
        }
    }
    out
}

/// Every `slip_kiln::kiln`, `slip_kiln::snippet` and `slip_kiln::ranges` in
/// Rust source `text`, as the module named and the line of its name: the
/// path form, and the items of a `slip_kiln::{…}` group that start with
/// one. Comments and
/// literals are not code.
fn kiln_module_paths(text: &str) -> Vec<(&'static str, usize)> {
    let module = |t: &str| ["kiln", "snippet", "ranges"].into_iter().find(|&m| m == t);
    let toks = super::rust_tokens::tokens(text);
    let mut out = Vec::new();
    for (i, t) in toks.iter().enumerate() {
        if t.text != "slip_kiln" || toks.get(i + 1).is_none_or(|t| t.text != "::") {
            continue;
        }
        let Some(next) = toks.get(i + 2) else { continue };
        if let Some(m) = module(next.text) {
            out.push((m, next.line));
            continue;
        }
        if next.text != "{" {
            continue;
        }
        // The group's items at its own depth: a name after `{` or `,`.
        let mut depth = 0usize;
        for (j, t) in toks.iter().enumerate().skip(i + 2) {
            match t.text {
                "{" => depth += 1,
                "}" => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                text if depth == 1 && matches!(toks[j - 1].text, "{" | ",") => {
                    if let Some(m) = module(text) {
                        out.push((m, t.line));
                    }
                }
                _ => {}
            }
        }
    }
    out
}

/// The shortest dependency path from `from` to `to` through members, both
/// ends included.
fn path_to<'a>(
    members: &BTreeMap<&'a str, &'a Member>,
    from: &'a str,
    to: &str,
) -> Option<Vec<&'a str>> {
    let mut prev: BTreeMap<&str, &str> = BTreeMap::new();
    let mut seen = BTreeSet::from([from]);
    let mut queue = VecDeque::from([from]);
    while let Some(at) = queue.pop_front() {
        let Some(m) = members.get(at) else { continue };
        for d in &m.deps {
            let Some((&next, _)) = members.get_key_value(d.name.as_str()) else { continue };
            if !seen.insert(next) {
                continue;
            }
            prev.insert(next, at);
            if next == to {
                let mut path = vec![next];
                let mut cur = next;
                while let Some(&p) = prev.get(cur) {
                    path.push(p);
                    cur = p;
                }
                path.reverse();
                return Some(path);
            }
            queue.push_back(next);
        }
    }
    None
}

/// `forbid-unsafe`.
pub fn forbid_unsafe(repo: &Repo) -> Vec<Violation> {
    let mut out = Vec::new();
    for m in members(repo) {
        let src = if m.dir.is_empty() { "src".to_owned() } else { format!("{}/src", m.dir) };
        let bin = format!("{src}/bin/");
        let roots: Vec<&str> = repo
            .files()
            .filter(|f| {
                *f == format!("{src}/lib.rs")
                    || *f == format!("{src}/main.rs")
                    || (f.starts_with(&bin) && Path::new(f).extension().is_some_and(|e| e == "rs"))
            })
            .collect();
        for root in roots {
            let text = repo.read(root).unwrap_or_default();
            let has = text.lines().any(|l| l.trim_start().starts_with("#![forbid(unsafe_code)]"));
            if !has {
                out.push(Violation::new(
                    root,
                    None,
                    "forbid-unsafe",
                    "crate root does not contain `#![forbid(unsafe_code)]`",
                ));
            }
        }
    }
    out
}

/// A `[[package]]` of a lock file.
#[derive(Debug)]
struct Locked {
    name: String,
    version: String,
    /// `source`, absent for workspace and path crates.
    source: Option<String>,
    line: usize,
}

fn locked(text: &str) -> Vec<Locked> {
    let (headers, entries) = manifest::parse(text);
    let starts: Vec<usize> =
        headers.iter().filter(|h| h.array && h.table == "package").map(|h| h.line).collect();
    let mut out = Vec::new();
    for (i, &start) in starts.iter().enumerate() {
        let end = starts.get(i + 1).copied().unwrap_or(usize::MAX);
        let field = |key: &str| {
            entries
                .iter()
                .find(|e| e.table == "package" && e.key == key && e.line > start && e.line < end)
        };
        let Some(name) = field("name") else { continue };
        out.push(Locked {
            name: manifest::strings(&name.value).into_iter().next().unwrap_or_default(),
            version: field("version")
                .and_then(|e| manifest::strings(&e.value).into_iter().next())
                .unwrap_or_default(),
            source: field("source").and_then(|e| manifest::strings(&e.value).into_iter().next()),
            line: name.line,
        });
    }
    out
}

/// `dependencies`.
pub fn dependencies(repo: &Repo) -> Vec<Violation> {
    const LINT: &str = "dependencies";
    let Some(lock) = repo.read("Cargo.lock") else {
        // No lock file, no third-party crates locked; a missing lock is
        // cargo's business, not this lint's.
        return Vec::new();
    };
    // Kiln's lock is in the submodule: read from disk, not from the list.
    let kiln = std::fs::read_to_string(repo.root().join(KILN_LOCK)).ok();
    let kiln_allowed: BTreeSet<(String, String)> = kiln
        .as_deref()
        .map(locked)
        .unwrap_or_default()
        .into_iter()
        .filter(|p| p.source.is_some())
        .map(|p| (p.name, p.version))
        .collect();
    let mut out = Vec::new();
    for p in locked(&lock).into_iter().filter(|p| p.source.is_some()) {
        if ALLOWED_CRATES.contains(&p.name.as_str())
            || kiln_allowed.contains(&(p.name.clone(), p.version.clone()))
        {
            continue;
        }
        let note =
            if kiln.is_none() { format!(" ({KILN_LOCK} cannot be read)") } else { String::new() };
        out.push(Violation::new(
            "Cargo.lock",
            Some(p.line),
            LINT,
            format!(
                "third-party crate `{} {}` is not allowed by spec/harness.md §3.1{note}",
                p.name, p.version
            ),
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globs() {
        assert!(glob("crates/*", "crates/slip_model"));
        assert!(!glob("crates/*", "crates/a/b"));
        assert!(glob("xtask", "xtask"));
        assert!(glob("c*/s*_m", "crates/slip_m"));
        assert!(!glob("crates/*", "xtask"));
    }

    #[test]
    fn dependency_tables() {
        assert!(is_dep_table("dependencies"));
        assert!(is_dep_table("target.'cfg(unix)'.dev-dependencies"));
        assert!(!is_dep_table("workspace.dependencies"));
        assert!(!is_dep_table("package"));
    }
}
