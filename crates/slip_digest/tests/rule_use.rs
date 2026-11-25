//! How each rule was used (`spec/verify.md` §9.1 "Rule use", §9.5; work
//! package WP-069): one directory per case under
//! `tests/rule_use/<group>/<case>/`, in the form of `tests/rules.rs`.
//!
//! A case is a pre-image `a.cpp` (whose records resolve `relocate` lines),
//! a post-image `b.cpp`, `rules.txt` (the table, in the form
//! `common/rules.rs` defines) and `expect.txt`. Any other file is a
//! header both sides see.
//!
//! # `expect.txt`
//!
//! A line that is blank or starts with `#` is none. Otherwise:
//!
//! - `use <n> via <v> direct <d> [kept_via <k>]`: the rule of line `<n>`
//!   of `rules.txt` has `via(r) = v`, `direct(r) = d` and `kept_via(r) =
//!   k` (0 when the word is absent) in the post-image's
//!   `Output::rule_use` (`spec/verify.md` §9.2 for a namespace rule). A
//!   rule with no line has no `Use` (all counts 0); so has one whose line
//!   says `via 0 direct 0`. Every file is owned and `Declared.kept` is
//!   empty here: `tests/rules.rs` has the cases with ownership and
//!   `kept`.
//! - `collisions <n>`: the post-image's counter `digest.collisions` is
//!   `n`. Without the line it must be 0.
//! - `indeterminate <name>`: the post-image under the table has a unit of
//!   that display name and it is indeterminate (`spec/digest.md` §10)
//!   for a cause other than `Pending`; the units with no such line are
//!   not.
//! - `refused`: the table holds a rule `Rule::refusal` refuses, kept as a
//!   container may hold it (`common::rules::table_with`).
//!
//! `SLIP_DIGEST_ONLY=<text>` narrows the cases as for the other runners;
//! `SLIP_DIGEST_DUMP=1` prints the records of both sides.
//!
//! # What every case also checks
//!
//! - `Output::rule_use` is strictly ascending by rule, holds no `Use`
//!   with all counts 0 and no rule past the table (`spec/verify.md`
//!   §9.1 "Invariants").
//! - The post-image with `rule_use` off is the same `Output` with an
//!   empty `rule_use`: nothing else moves (§9.5).
//! - Two walks with `rule_use` on give one `Output`.
//! - The table walked is the one a run reads: written in a `Declared` and
//!   decoded. The table `RuleTable::new` builds gives the same `Output`.
//!
//! Beside the cases, [`pair_tests_on_and_off`] walks every source of the
//! pair tests with `rule_use` on and off, under a table of one rule that
//! matches nothing there (so the counting runs on every key), and finds
//! the outputs equal but for `rule_use`, which is empty; `tests/rules.rs`
//! does the same for the post-image of each of its cases, under its
//! table.

mod common;

use std::path::Path;

use common::Side;
use common::rules::{read, table_with};
use slip_digest::records::records;
use slip_digest::{Cause, Detail, Input, Options, Output};
use slip_model::agreement::Use;
use slip_model::declared::Declared;
use slip_model::keys::{KeyKind, Name, Parent, Rule, RuleTable};

fn walk(input: &Input<'_>, rules: &RuleTable, rule_use: bool) -> Output {
    slip_digest::digest(
        input,
        &Options {
            rules,
            // Every file owned (WP-110): §9.1's counts, as before ownership.
            read_only: slip_digest::ReadOnly::NO_FILE,
            detail: Detail::All,
            names: true,
            rule_use,
        },
    )
}

/// What a case's `expect.txt` says.
#[derive(Default)]
struct Expected {
    /// `(line, via, direct, kept_via)`.
    uses: Vec<(usize, u64, u64, u64)>,
    collisions: u64,
    refused: bool,
    /// `indeterminate <name>`, sorted.
    indeterminate: Vec<String>,
}

fn expected(dir: &Path) -> Result<Expected, String> {
    let text =
        std::fs::read_to_string(dir.join("expect.txt")).map_err(|e| format!("expect.txt: {e}"))?;
    let mut out = Expected::default();
    for (number, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line.strip_prefix("indeterminate ") {
            out.indeterminate.push(name.trim().to_owned());
            continue;
        }
        let words: Vec<&str> = line.split_whitespace().collect();
        let bad =
            || format!("expect.txt:{}: `{line}` is not a line of tests/rule_use.rs", number + 1);
        match words.as_slice() {
            ["use", rule, "via", via, "direct", direct, rest @ ..] => {
                let rule = rule.parse().map_err(|_| bad())?;
                let via = via.parse().map_err(|_| bad())?;
                let direct = direct.parse().map_err(|_| bad())?;
                let kept_via = match rest {
                    [] => 0,
                    ["kept_via", k] => k.parse().map_err(|_| bad())?,
                    _ => return Err(bad()),
                };
                out.uses.push((rule, via, direct, kept_via));
            }
            ["collisions", count] => out.collisions = count.parse().map_err(|_| bad())?,
            ["refused"] => out.refused = true,
            _ => return Err(bad()),
        }
    }
    out.indeterminate.sort();
    Ok(out)
}

/// §9.1's invariants of `uses` under a table of `n` rules.
fn invariants(uses: &[Use], n: usize) -> Result<(), String> {
    if !uses.windows(2).all(|w| w[0].rule < w[1].rule) {
        return Err(format!("rule_use is not strictly ascending by rule: {uses:?}"));
    }
    if let Some(u) = uses.iter().find(|u| u.via == 0 && u.direct == 0 && u.kept_via == 0) {
        return Err(format!("a Use with all counts 0: {u:?}"));
    }
    if let Some(u) = uses.iter().find(|u| u.rule >= n as u64) {
        return Err(format!("a Use of a rule past the table of {n}: {u:?}"));
    }
    Ok(())
}

/// §9.5: with `rule_use` off the output is `on`'s with an empty
/// `rule_use`.
fn same_but_rule_use(on: &Output, off: &Output) -> Result<(), String> {
    if !off.rule_use.is_empty() {
        return Err(format!("rule_use off, yet counted: {:?}", off.rule_use));
    }
    let cleared = Output {
        rule_use: Vec::new(),
        kept: Vec::new(),
        made: Vec::new(),
        fixed: Vec::new(),
        rebuilt: Default::default(),
        ..on.clone()
    };
    if cleared != *off {
        return Err("the output with rule_use on differs from the one with it off".into());
    }
    Ok(())
}

/// `table` as a run reads it: in a `Declared`, written to bytes and read
/// back (`RuleTable::from_sorted`, through the container reader).
fn decoded(table: &RuleTable) -> Result<RuleTable, String> {
    // A container with rules holds the run's files (WP-110); these own
    // everything, as the walks of this file do.
    let declared = Declared::new(table.rules().to_vec(), vec![], vec![], vec![])
        .map_err(|e| format!("Declared::new: {e}"))?
        .with_files(Some(slip_model::transform::Ownership {
            owned: vec![b"/".to_vec()],
            readonly: vec![],
            rebuilt: Vec::new(),
        }));
    let read = Declared::from_bytes(&declared.to_bytes())
        .map_err(|e| format!("the declared change read back: {e}"))?;
    Ok(read.rules().clone())
}

fn case(dir: &Path) -> Result<(), String> {
    let e = expected(dir)?;
    let text =
        std::fs::read_to_string(dir.join("rules.txt")).map_err(|x| format!("rules.txt: {x}"))?;
    let lines = read(&text)?;
    let pre = common::with_side(dir, Side::A, |input| {
        records(input, &RuleTable::empty(), slip_digest::ReadOnly::EVERY_FILE)
    })?
    .map_err(|x| format!("records of the pre-image: {x}"))?;
    common::with_side(dir, Side::B, |input| -> Result<(), String> {
        let plain = records(input, &RuleTable::empty(), slip_digest::ReadOnly::EVERY_FILE)?;
        if std::env::var_os("SLIP_DIGEST_DUMP").is_some() {
            for (side, recs) in [("A", &pre), ("B", &plain)] {
                for r in recs {
                    eprintln!("--- {} {side}: {:?} {}", common::rel(dir), r.kind, r.name);
                }
            }
        }
        let t = table_with(&lines, &pre, &plain, !e.refused)?;
        if e.refused != t.table.refused().is_some() {
            return Err(format!(
                "`refused` is {}, the table's refusal {:?}",
                e.refused,
                t.table.refused()
            ));
        }
        // A run's table is read from a container, not built by
        // `RuleTable::new`: walk the one decoded from a `Declared` written
        // out, and require `RuleTable::new`'s to count the same.
        let decoded = decoded(&t.table)?;
        let on = walk(input, &decoded, true);
        if walk(input, &decoded, true) != on {
            return Err("two walks with rule_use on differ".into());
        }
        if walk(input, &t.table, true) != on {
            return Err("RuleTable::new's table and the decoded one give two outputs".into());
        }
        same_but_rule_use(&on, &walk(input, &decoded, false))?;
        invariants(&on.rule_use, t.table.rules().len())?;
        let mut want: Vec<Use> = Vec::new();
        for &(n, via, direct, kept_via) in &e.uses {
            let Some((_, rule)) = t.rules.iter().find(|(l, _)| *l == n) else {
                return Err(format!("`use {n}`: no rule on line {n} of rules.txt"));
            };
            let index = t
                .table
                .rules()
                .iter()
                .position(|r| r == rule)
                .ok_or_else(|| format!("`use {n}`: the rule is not in the table"))?;
            if via != 0 || direct != 0 || kept_via != 0 {
                want.push(Use { rule: index as u64, via, direct, kept_via });
            }
        }
        want.sort();
        let mut problems = Vec::new();
        if on.rule_use != want {
            let line = |r: u64| {
                let rule = &t.table.rules()[r as usize];
                t.rules.iter().find(|(_, x)| x == rule).map_or(0, |(l, _)| *l)
            };
            let show = |u: &[Use]| -> Vec<String> {
                u.iter()
                    .map(|x| {
                        format!(
                            "use {} via {} direct {} kept_via {}",
                            line(x.rule),
                            x.via,
                            x.direct,
                            x.kept_via
                        )
                    })
                    .collect()
            };
            problems.push(format!("expected {:?}\ncounted  {:?}", show(&want), show(&on.rule_use)));
        }
        let collisions =
            on.counters.iter().find(|c| c.name == "digest.collisions").map_or(0, |c| c.value);
        if collisions != e.collisions {
            problems.push(format!("digest.collisions is {collisions}, expected {}", e.collisions));
        }
        if !e.refused {
            let mut indet: Vec<String> = on
                .causes
                .iter()
                .filter(|(_, c)| !matches!(c, Cause::Pending(_)))
                .flat_map(|(k, _)| on.names.iter().filter(move |(x, _)| x == k))
                .map(|(_, n)| n.clone())
                .filter(|n| n != "<order>")
                .collect();
            indet.sort();
            indet.dedup();
            if indet != e.indeterminate {
                problems
                    .push(format!("indeterminate units {indet:?}, expected {:?}", e.indeterminate));
            }
        }
        if problems.is_empty() { Ok(()) } else { Err(problems.join("\n")) }
    })?
}

/// Every case of `tests/rule_use/`.
#[test]
fn rule_use() {
    common::run_all("rule_use", case);
}

/// A table of one rule on a function no pair test declares: not empty,
/// so the walk counts (every key is looked up in `map_traced` and
/// `produces`), and it matches nothing.
fn unmatched_table() -> RuleTable {
    let name = |s: &str| Name::Identifier(s.as_bytes().to_vec());
    RuleTable::new(vec![Rule {
        parent: Parent::Root,
        kind: KeyKind::Function,
        name: name("slip_rule_use_unmatched"),
        disc: None,
        to_parent: Parent::Root,
        to_name: name("slip_rule_use_unmatched_old"),
    }])
    .expect("a valid rule")
}

/// §9.5 (`spec/verify.md` "Tests"): every source of every pair test,
/// walked with `rule_use` on and off under [`unmatched_table`], gives one
/// output but for `rule_use`, which is empty; and its digests, causes
/// and names are those the empty table gives.
#[test]
fn pair_tests_on_and_off() {
    let mut dirs = Vec::new();
    for root in ["differ", "equal", "demand", "indeterminate", "resolved"] {
        dirs.extend(common::dirs(root).into_iter().filter(|d| !common::is_pending(d)));
    }
    let threads = common::pool_size().unwrap_or_else(|e| panic!("{e}"));
    let table = unmatched_table();
    let outcome = common::run_dirs("pairs", &dirs, threads, |dir| {
        for side in [Side::A, Side::B] {
            let r = common::with_side(dir, side, |input| -> Result<(), String> {
                let on = walk(input, &table, true);
                same_but_rule_use(&on, &walk(input, &table, false))?;
                if !on.rule_use.is_empty() {
                    return Err(format!("a rule no pair declares was counted: {:?}", on.rule_use));
                }
                // The digests, causes and names are the empty table's. The
                // counters are not: under a table that is not empty the
                // walk also reads the keys of instantiated members' pattern
                // members (`follow_patterns`), which `digest.bytes` counts.
                let e = walk(input, &RuleTable::empty(), false);
                if (&on.digests, &on.causes, &on.names) != (&e.digests, &e.causes, &e.names) {
                    return Err("the unmatched rule changed a digest, cause or name".into());
                }
                Ok(())
            });
            // A side that does not parse is the pair runner's to report.
            if let Ok(r) = r {
                r.map_err(|e| format!("side {side:?}: {e}"))?;
            }
        }
        Ok(Vec::new())
    });
    outcome.finish();
}
