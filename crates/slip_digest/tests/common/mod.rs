//! What the pair-test runners share (WP-024, `spec/digest.md` §14,
//! `spec/digest-kinds.md` §1.5): reading a test directory, parsing a side
//! with `slip_kiln::snippet`, digesting it, and reading `expect.txt`.
//!
//! A directory is `tests/<root>/<short>/<row>/` or `tests/<root>/_/<slug>/`.
//! It holds `t.cpp` (which begins `#include "side.h"`; the runner supplies
//! `side.h`, empty for side A and `#define B 1` for side B) or `a.cpp` and
//! `b.cpp`; any other file is a header both sides see. The main file's
//! first lines may say `// std: c++17` and `// errors: yes`. Both sides'
//! main file is parsed under the path `main.cpp`, so that their groups
//! have the same subject path.

#![allow(dead_code, reason = "each runner uses a part")]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use slip_digest::{Cause, Detail, Input, Options, Output};
use slip_model::Digest;
use slip_model::digests::GroupClass;
use slip_model::keys::RuleTable;

/// Which side of a pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    A,
    B,
}

/// One unit of a digested side, by key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitInfo {
    pub class: GroupClass,
    pub digest: Digest,
    pub flags: u64,
    pub name: String,
    pub cause: Option<Cause>,
}

/// A digested side.
#[derive(Debug)]
pub struct Digested {
    pub output: Output,
    /// Every unit by key; a key with several units has all of them.
    pub units: BTreeMap<Digest, Vec<UnitInfo>>,
    pub errors: u32,
    pub diagnostics: String,
}

impl Digested {
    /// The name of a key (the first of its units).
    pub fn name(&self, k: &Digest) -> String {
        self.units
            .get(k)
            .and_then(|v| v.first())
            .map_or_else(|| format!("{k:?}"), |u| u.name.clone())
    }
}

/// The crate directory.
pub fn manifest() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Every test directory under `tests/<root>`, sorted: `<short>/<row>`
/// and `_/<slug>`.
pub fn dirs(root: &str) -> Vec<PathBuf> {
    let base = manifest().join("tests").join(root);
    let mut out = Vec::new();
    let Ok(shorts) = std::fs::read_dir(&base) else { return out };
    for short in shorts.flatten() {
        if !short.path().is_dir() {
            continue;
        }
        for row in std::fs::read_dir(short.path()).into_iter().flatten().flatten() {
            if row.path().is_dir() {
                out.push(row.path());
            }
        }
    }
    out.sort();
    out
}

/// The directory relative to `tests/`, for messages.
pub fn rel(dir: &Path) -> String {
    dir.strip_prefix(manifest().join("tests")).unwrap_or(dir).display().to_string()
}

/// The files of a test directory, by name.
fn files(dir: &Path) -> Result<BTreeMap<String, String>, String> {
    let mut out = BTreeMap::new();
    for e in std::fs::read_dir(dir).map_err(|e| e.to_string())?.flatten() {
        if e.path().is_file() {
            let name = e.file_name().to_string_lossy().into_owned();
            let text = std::fs::read_to_string(e.path()).map_err(|e| format!("{name}: {e}"))?;
            out.insert(name, text);
        }
    }
    Ok(out)
}

/// The header lines of a main file: `// std: X` and `// errors: yes`.
fn header(main: &str) -> (String, bool) {
    let mut std = "c++20".to_owned();
    let mut errors = false;
    for line in main.lines().take(4) {
        if let Some(v) = line.strip_prefix("// std:") {
            v.trim().clone_into(&mut std);
        } else if line.trim() == "// errors: yes" {
            errors = true;
        }
    }
    (std, errors)
}

/// Whether the test is marked `# pending` in its `expect.txt`: the
/// runner skips it and lists it.
pub fn is_pending(dir: &Path) -> bool {
    std::fs::read_to_string(dir.join("expect.txt"))
        .is_ok_and(|t| t.lines().any(|l| l.trim() == "# pending"))
}

/// Parses and digests one side of the test in `dir`.
pub fn digest_side(dir: &Path, side: Side) -> Result<Digested, String> {
    digest_side_with(dir, side, &RuleTable::empty())
}

/// [`digest_side`] under the rules `rules`.
pub fn digest_side_with(dir: &Path, side: Side, rules: &RuleTable) -> Result<Digested, String> {
    let all = files(dir)?;
    let mut list: Vec<(String, String)> = Vec::new();
    let main_text;
    if let Some(t) = all.get("t.cpp") {
        main_text = t.clone();
        let side_h = match side {
            Side::A => "",
            Side::B => "#define B 1\n",
        };
        list.push(("side.h".to_owned(), side_h.to_owned()));
    } else {
        let name = match side {
            Side::A => "a.cpp",
            Side::B => "b.cpp",
        };
        main_text = all.get(name).cloned().ok_or_else(|| format!("no t.cpp and no {name}"))?;
    }
    list.push(("main.cpp".to_owned(), main_text.clone()));
    for (name, text) in &all {
        if !matches!(name.as_str(), "t.cpp" | "a.cpp" | "b.cpp" | "expect.txt") {
            list.push((name.clone(), text.clone()));
        }
    }
    let (std, errors_allowed) = header(&main_text);
    let refs: Vec<(&str, &str)> = list.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
    let snippet =
        slip_kiln::snippet::Snippet { files: &refs, main: "main.cpp", include: &[""], std: &std };
    let out = slip_kiln::snippet::parse(&snippet, |p| {
        let path = |f| p.path(f).map(|s: &str| s.as_bytes().to_vec()).unwrap_or_default();
        let input = Input { ast: p.ast, sm: p.sm, subject_path: &path };
        let options = Options { rules, detail: Detail::All, names: true };
        (slip_digest::digest(&input, &options), p.errors, p.diagnostics.clone())
    })?;
    let (output, errors, diagnostics) = out;
    if errors > 0 && !errors_allowed {
        return Err(format!("side {side:?} has {errors} error(s):\n{diagnostics}"));
    }
    let units = index(&output);
    if std::env::var_os("SLIP_DIGEST_DUMP").is_some() {
        eprintln!("--- {} side {side:?}", rel(dir));
        for (k, v) in &units {
            for u in v {
                eprintln!(
                    "  {:?} {} {:?} flags={} {:?}",
                    u.class,
                    u.name,
                    &k.0[..4],
                    u.flags,
                    u.cause
                );
            }
        }
    }
    Ok(Digested { output, units, errors, diagnostics })
}

/// Every unit of an output by key.
pub fn index(o: &Output) -> BTreeMap<Digest, Vec<UnitInfo>> {
    let names: BTreeMap<Digest, String> = o.names.iter().cloned().collect();
    let causes: BTreeMap<Digest, Cause> = o.causes.iter().copied().collect();
    let mut out: BTreeMap<Digest, Vec<UnitInfo>> = BTreeMap::new();
    for g in &o.digests.groups {
        for u in g.units.iter().flatten() {
            out.entry(u.key).or_default().push(UnitInfo {
                class: g.class,
                digest: u.digest,
                flags: u.flags,
                name: names.get(&u.key).cloned().unwrap_or_default(),
                cause: causes.get(&u.key).copied(),
            });
        }
    }
    for v in out.values_mut() {
        v.sort_by_key(|a| (a.class, a.digest, a.flags));
    }
    out
}

/// One line of `expect.txt`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Expect {
    Changed(String),
    Added(String),
    Removed(String),
    Indeterminate(String),
    DerivedChanged(String),
}

/// The lines of `expect.txt` (absent: none).
pub fn expect(dir: &Path) -> Result<BTreeSet<Expect>, String> {
    let Ok(text) = std::fs::read_to_string(dir.join("expect.txt")) else {
        return Ok(BTreeSet::new());
    };
    let mut out = BTreeSet::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (word, name) =
            line.split_once(' ').ok_or_else(|| format!("expect.txt:{}: no name", i + 1))?;
        let name = name.trim().to_owned();
        out.insert(match word {
            "changed" => Expect::Changed(name),
            "added" => Expect::Added(name),
            "removed" => Expect::Removed(name),
            "indeterminate" => Expect::Indeterminate(name),
            "derived-changed" => Expect::DerivedChanged(name),
            _ => return Err(format!("expect.txt:{}: unknown word `{word}`", i + 1)),
        });
    }
    Ok(out)
}

/// The units of a side that are indeterminate for a cause other than
/// `Pending` (`Pending` is tolerated while `Area` exists).
pub fn hard_indeterminate(d: &Digested) -> Vec<String> {
    let mut out = Vec::new();
    for v in d.units.values() {
        for u in v {
            if u.flags & 1 != 0 && !matches!(u.cause, Some(Cause::Pending(_)) | None) {
                out.push(format!(
                    "{} ({})",
                    u.name,
                    u.cause.map(|c| c.to_string()).unwrap_or_default()
                ));
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Runs `f` on every directory of `root`, collects every failure with its
/// directory, and panics with all of them; lists the skipped ones.
pub fn run_all(root: &str, f: impl Fn(&Path) -> Result<(), String>) {
    let mut failures = Vec::new();
    let mut skipped = Vec::new();
    let mut ran = 0;
    for dir in dirs(root) {
        if is_pending(&dir) {
            skipped.push(rel(&dir));
            continue;
        }
        ran += 1;
        if let Err(e) = f(&dir) {
            failures.push(format!("{}: {e}", rel(&dir)));
        }
    }
    for s in &skipped {
        eprintln!("skipped (pending): {s}");
    }
    eprintln!("{root}: {ran} run, {} skipped, {} failed", skipped.len(), failures.len());
    assert!(failures.is_empty(), "{} of {ran} failed:\n{}", failures.len(), failures.join("\n\n"));
}
