//! The mark "emitted unreferenced" (`spec/digest.md` §11 flag bit 1;
//! WP-155, `spec/landed/verify/WP-155.md` §1): snippets under
//! `tests/emitted/_/<slug>/`, each a `t.cpp` digested as side A and as side
//! B (`B` defined), and the units each side marks.
//!
//! The lines of `expect.txt`:
//!
//! | Line | Holds when |
//! |---|---|
//! | `marked <name>` | both sides have a unit of that name, marked |
//! | `marked-a <name>` | side A has a unit of that name, marked |
//! | `marked-b <name>` | side B has a unit of that name, marked |
//!
//! Beyond the lines: the units each side marks are exactly its lines (a
//! file of comments alone says nothing is marked); no unit is
//! indeterminate for a cause other than `Pending`; a unit both sides have
//! is marked in both or in neither (the mark is a fact of the entity, not
//! of the side's other code); and the mark is no other flag.

mod common;

use std::collections::BTreeSet;
use std::path::Path;

use slip_model::digests::{GroupClass, Unit};

/// The names each side must mark: side A's and side B's.
fn lines(dir: &Path) -> Result<(BTreeSet<String>, BTreeSet<String>), String> {
    let text = std::fs::read_to_string(dir.join("expect.txt"))
        .map_err(|e| format!("expect.txt: {e} (an emitted test names what it marks)"))?;
    let (mut a, mut b) = (BTreeSet::new(), BTreeSet::new());
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (word, name) =
            line.split_once(' ').ok_or_else(|| format!("expect.txt:{}: no name", i + 1))?;
        let name = name.trim().to_owned();
        match word {
            "marked" => {
                a.insert(name.clone());
                b.insert(name);
            }
            "marked-a" => {
                a.insert(name);
            }
            "marked-b" => {
                b.insert(name);
            }
            _ => return Err(format!("expect.txt:{}: unknown word `{word}`", i + 1)),
        }
    }
    Ok((a, b))
}

/// The names of the units of `d` that carry the mark.
fn marked(d: &common::Digested) -> BTreeSet<String> {
    d.units
        .iter()
        .filter(|(_, v)| v.iter().any(|u| u.flags & Unit::EMITTED != 0))
        .map(|(k, _)| d.name(k))
        .collect()
}

fn one(dir: &Path) -> Result<(), String> {
    let (want_a, want_b) = lines(dir)?;
    let a = common::digest_side(dir, common::Side::A)?;
    let b = common::digest_side(dir, common::Side::B)?;
    let mut problems = Vec::new();
    for (side, d, want) in [("A", &a, &want_a), ("B", &b, &want_b)] {
        for u in common::hard_indeterminate(d) {
            problems.push(format!("side {side}: indeterminate {u}"));
        }
        let got = marked(d);
        if &got != want {
            problems.push(format!("side {side} marks {got:?}, expected {want:?}"));
        }
        // The counter `digest.units.derived.emitted` (spec/digest.md §12).
        let derived_marked = d
            .output
            .digests
            .groups
            .iter()
            .filter(|g| g.class == GroupClass::Derived)
            .flat_map(|g| g.units.as_deref().unwrap_or(&[]))
            .filter(|u| u.is_emitted())
            .count() as u64;
        let counted = d.output.counters.iter().find(|c| c.name == "digest.units.derived.emitted");
        if counted.map(|c| c.value) != Some(derived_marked) {
            problems.push(format!(
                "side {side}: counter {counted:?}, {derived_marked} marked derived units"
            ));
        }
        for (k, v) in &d.units {
            if v.iter().any(|u| u.flags & !Unit::FLAGS != 0) {
                problems.push(format!("side {side}: {} has a flag of no meaning", d.name(k)));
            }
        }
    }
    for (k, ua) in &a.units {
        let Some(ub) = b.units.get(k) else { continue };
        let mark = |v: &[common::UnitInfo]| v.iter().any(|u| u.flags & Unit::EMITTED != 0);
        if mark(ua) != mark(ub) {
            problems.push(format!("{}: marked in one side only", a.name(k)));
        }
    }
    if problems.is_empty() { Ok(()) } else { Err(problems.join("\n")) }
}

#[test]
fn emitted() {
    common::run_all("emitted", one);
}
