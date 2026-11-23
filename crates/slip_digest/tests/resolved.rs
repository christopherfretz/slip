//! Resolved units (`spec/digest-kinds.md` §11.1, "Tests"; digest §2.2,
//! §14): pairs under `tests/resolved/<property>/<slug>/`, side B with a
//! use that makes Kiln compute a fact (or with a fact that differs), and
//! the resolved units that come and go with it.
//!
//! The lines of `expect.txt`:
//!
//! | Line | Holds when |
//! |---|---|
//! | `resolved <name>` | B has a resolved unit of that name and A has none |
//! | `present <name>` | both sides have a resolved unit of that name |
//! | `changed <name>` | both sides have a unit of that name, of any class, and its digest or flags differ |
//! | `added <name>` | B has a declared unit of that name and A has none (the function that holds the use) |
//!
//! Beyond the lines: every unit both sides have is the same on both
//! (digest, flags and class) unless a `changed` line names it, as the
//! demand runner asks; the resolved units only B has are exactly the
//! `resolved` lines and A has none B lacks; the declared units only B has
//! are exactly the `added` lines and A has none B lacks; no unit is
//! indeterminate for a cause other than `Pending`. A resolved unit is told
//! by its name, the owner's followed by `::<property>` (`names.rs`); a
//! `default_argument` unit (§11.4), named `<function>::<default_argument
//! n>`, is read as one.

mod common;

use std::collections::BTreeSet;
use std::path::Path;

use slip_model::digests::GroupClass;

/// The properties of §11.1, as names write them.
const PROPERTIES: [&str; 7] = [
    "exception_spec",
    "value",
    "key_function",
    "dtor_delete",
    "inherited_constructor",
    "default_immediates",
    "definition",
];

/// Whether `name` is a resolved unit's (`<owner>::<property>` or
/// `<owner>::<property n>`) or a `default_argument` unit's
/// (`<function>::<default_argument n>`, digest-kinds §11.4): the runner
/// treats the two alike.
fn is_resolved(name: &str) -> bool {
    let Some(rest) = name.strip_suffix('>') else { return false };
    let Some(at) = rest.rfind("::<") else { return false };
    let word = &rest[at + 3..];
    let word = word.split_once(' ').map_or(word, |(w, _)| w);
    PROPERTIES.contains(&word) || word == "default_argument"
}

/// One line of a `resolved` test's `expect.txt`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Line {
    Resolved(String),
    Present(String),
    Changed(String),
    Added(String),
}

fn lines(dir: &Path) -> Result<BTreeSet<Line>, String> {
    let text = std::fs::read_to_string(dir.join("expect.txt"))
        .map_err(|e| format!("expect.txt: {e} (a resolved test names what it shows)"))?;
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
            "resolved" => Line::Resolved(name),
            "present" => Line::Present(name),
            "changed" => Line::Changed(name),
            "added" => Line::Added(name),
            _ => return Err(format!("expect.txt:{}: unknown word `{word}`", i + 1)),
        });
    }
    if out.is_empty() {
        return Err("a resolved test needs at least one line in expect.txt".into());
    }
    Ok(out)
}

/// The names of the units of `d` whose key `other` lacks, with `class`
/// (or resolved, when `resolved`).
fn only(d: &common::Digested, other: &common::Digested, resolved: bool) -> BTreeSet<String> {
    d.units
        .iter()
        .filter(|(k, _)| !other.units.contains_key(*k))
        .filter(|(k, v)| {
            let name = d.name(k);
            if resolved {
                is_resolved(&name)
            } else {
                v.iter().any(|u| u.class == GroupClass::Declared)
            }
        })
        .map(|(k, _)| d.name(k))
        .collect()
}

fn one(dir: &Path) -> Result<(), String> {
    let expect = lines(dir)?;
    let a = common::digest_side(dir, common::Side::A)?;
    let b = common::digest_side(dir, common::Side::B)?;
    let mut problems = Vec::new();
    for (side, d) in [("A", &a), ("B", &b)] {
        for u in common::hard_indeterminate(d) {
            problems.push(format!("side {side}: indeterminate {u}"));
        }
    }
    let changed: BTreeSet<&String> = expect
        .iter()
        .filter_map(|l| match l {
            Line::Changed(n) => Some(n),
            Line::Resolved(_) | Line::Present(_) | Line::Added(_) => None,
        })
        .collect();
    let mut seen_changed: BTreeSet<String> = BTreeSet::new();
    for (k, ua) in &a.units {
        let Some(ub) = b.units.get(k) else { continue };
        let name = a.name(k);
        if ua != ub {
            if changed.contains(&name) {
                seen_changed.insert(name);
            } else {
                problems.push(format!("differs: {name}"));
            }
        }
    }
    for n in changed {
        if !seen_changed.contains(n) {
            problems.push(format!("`changed {n}`: no unit of both sides of that name differs"));
        }
    }
    let want = |f: fn(&Line) -> Option<&String>| -> BTreeSet<String> {
        expect.iter().filter_map(f).cloned().collect()
    };
    let resolved = want(|l| match l {
        Line::Resolved(n) => Some(n),
        Line::Present(_) | Line::Changed(_) | Line::Added(_) => None,
    });
    let got = only(&b, &a, true);
    if got != resolved {
        problems.push(format!("resolved units only B has: expected {resolved:?}, saw {got:?}"));
    }
    let only_a = only(&a, &b, true);
    if !only_a.is_empty() {
        problems.push(format!("resolved units only A has: {only_a:?}"));
    }
    for n in want(|l| match l {
        Line::Present(n) => Some(n),
        Line::Resolved(_) | Line::Changed(_) | Line::Added(_) => None,
    }) {
        let has = |d: &common::Digested| d.units.keys().any(|k| d.name(k) == n);
        if !is_resolved(&n) || !has(&a) || !has(&b) {
            problems.push(format!("`present {n}`: not a resolved unit of both sides"));
        }
    }
    let added = want(|l| match l {
        Line::Added(n) => Some(n),
        Line::Resolved(_) | Line::Present(_) | Line::Changed(_) => None,
    });
    let got = only(&b, &a, false);
    if got != added {
        problems.push(format!("declared units only B has: expected {added:?}, saw {got:?}"));
    }
    let only_a = only(&a, &b, false);
    if !only_a.is_empty() {
        problems.push(format!("declared units only A has: {only_a:?}"));
    }
    if problems.is_empty() { Ok(()) } else { Err(problems.join("\n")) }
}

#[test]
fn resolved() {
    common::run_all("resolved", one);
}

// The runner's own reading of names: a resolved unit's name ends in
// `::<property>`, with a discriminator after a space.
#[test]
fn resolved_names() {
    assert!(is_resolved("S::<key_function>"));
    assert!(is_resolved("f(int)::<default_immediates 0>"));
    assert!(is_resolved("X<int>::f()::<definition>"));
    assert!(is_resolved("X<int>::f(int)::<default_argument 0>"));
    assert!(is_resolved("X<int>::f(int)::<default_argument 0>::<default_immediates 0>"));
    assert!(!is_resolved("S::<static_assert>"));
    assert!(!is_resolved("operator<"));
    assert!(!is_resolved("S<int>"));
}
