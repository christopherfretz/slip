//! Operand units (WP-156, `spec/digest.md` §2.4, `spec/verify.md` §4
//! step 3): snippets under `tests/operands/_/<slug>/`, each a `t.cpp`
//! digested as side A and as side B (`B` defined; `relocate.txt` as in a
//! pair test), judged by `common::pairs::judge_operands`: every unit both
//! sides have is equal, a unit one side alone has is the operand unit of
//! a unit both sides have, and no unit is indeterminate for a cause other
//! than `Pending`. The class of a pair whose one difference is an operand
//! unit one side has and the other does not (S2, S8: no finding).
//!
//! The lines of `expect.txt` name, by its parent's name, every operand
//! unit of either side:
//!
//! | Line | Holds when |
//! |---|---|
//! | `operands <name>` | both sides have the operand unit of `<name>` (equal) |
//! | `operands-a <name>` | side A alone has it |
//! | `operands-b <name>` | side B alone has it |
//! | `count <counter> <value>` | each side's counter `<counter>` is `<value>` (absent: 0) |
//!
//! A file of comments alone says neither side has an operand unit. A
//! `count` line pins how many entries a site writes, which no set of
//! names and no pair shows: an entry written twice on both sides
//! (WP-156 fix round 1, mutations R09 and R10).

mod common;

use std::collections::BTreeSet;
use std::path::Path;

use common::Side;

/// What `expect.txt` says: the parents side A names, side B's, and the
/// counters by name.
type Expected = (BTreeSet<String>, BTreeSet<String>, Vec<(String, u64)>);

/// The lines of `expect.txt`.
fn lines(dir: &Path) -> Result<Expected, String> {
    let text = std::fs::read_to_string(dir.join("expect.txt"))
        .map_err(|e| format!("expect.txt: {e} (an operands test names its operand units)"))?;
    let (mut a, mut b, mut counts) = (BTreeSet::new(), BTreeSet::new(), Vec::new());
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (word, name) =
            line.split_once(' ').ok_or_else(|| format!("expect.txt:{}: no name", i + 1))?;
        let name = name.trim().to_owned();
        match word {
            "operands" => {
                a.insert(name.clone());
                b.insert(name);
            }
            "operands-a" => {
                a.insert(name);
            }
            "operands-b" => {
                b.insert(name);
            }
            "count" => {
                let (counter, value) = name
                    .split_once(' ')
                    .and_then(|(c, v)| Some((c.to_owned(), v.trim().parse::<u64>().ok()?)))
                    .ok_or_else(|| format!("expect.txt:{}: `count <counter> <value>`", i + 1))?;
                counts.push((counter, value));
            }
            _ => return Err(format!("expect.txt:{}: unknown word `{word}`", i + 1)),
        }
    }
    Ok((a, b, counts))
}

fn one(dir: &Path) -> Result<(), String> {
    let (want_a, want_b, counts) = lines(dir)?;
    let a = common::digest_side(dir, Side::A)?;
    let b = common::digest_side(dir, Side::B)?;
    let (got_a, got_b) = common::pairs::judge_operands(&a, &b)?;
    let mut problems = Vec::new();
    for (side, d) in [("A", &a), ("B", &b)] {
        for (counter, want) in &counts {
            let got = d
                .output
                .counters
                .iter()
                .find(|c| &c.name == counter)
                .map_or(0, |c| c.value);
            if got != *want {
                problems.push(format!("side {side}: counter `{counter}` is {got}, not {want}"));
            }
        }
    }
    for (side, want, got) in [("A", &want_a, &got_a), ("B", &want_b, &got_b)] {
        for n in got.difference(want) {
            problems.push(format!("side {side}: an operand unit of `{n}` no line names"));
        }
        for n in want.difference(got) {
            problems.push(format!("side {side}: no operand unit of `{n}`"));
        }
    }
    if problems.is_empty() { Ok(()) } else { Err(problems.join("\n")) }
}

#[test]
fn operands() {
    common::run_all("operands", one);
}
