//! Mapped keys under rules (`spec/digest.md` §3.4, `spec/verify.md` §2.1,
//! §9.1; WP-103): one directory per case under `tests/rules/<group>/<case>/`.
//!
//! A case is a pre-image `a.cpp` (digested with no rule), a post-image
//! `b.cpp` (digested under the case's table), `rules.txt` (the table, in
//! the form `common/rules.rs` defines; absent: no rule) and `expect.txt`.
//! Any other file is a header both sides see.
//!
//! # `expect.txt`
//!
//! The lines of `spec/digest.md` §14.1 and the directive `exact derived`,
//! read as a pair test reads them, and one more form:
//!
//! - No line: the case is **equal**: the two sides' `Digests` are equal
//!   (detail `all`), as a must-equal pair's are, and no unit of either is
//!   indeterminate for a cause other than `Pending`.
//! - Otherwise the case **differs**, exactly as the lines say: over
//!   declared and `tu` units, the keys that differ are the `changed`,
//!   `added` and `removed` lines; the units of either side that are
//!   indeterminate for a cause other than `Pending` are the
//!   `indeterminate` lines (by the name the side gives them), and their
//!   keys are left out of the other lines (an indeterminate unit is
//!   unequal to everything, `spec/digest.md` §10); the `derived-*` lines
//!   are as in a `differ` pair (§14.1 rule 3).
//! - `use <n> via <v> direct <d>`: the rule of line `<n>` of `rules.txt`
//!   counts `via(r) = v` and `direct(r) = d` in the post-image
//!   (`spec/verify.md` §9.1). Not a line of §14.1: it is read here and
//!   says nothing of equality. The walk does not count rule use yet
//!   (WP-069), so the counts are this runner's own, from the records of
//!   the post-image: under the table with that rule's `to_name` replaced
//!   by a name no C++ entity has (`slip probe <n>`), `via` is the number
//!   of records with the key the rule then produces, and `direct` the
//!   number with the key it produces under the table itself
//!   (`(to_parent, kind, to_name, disc)`). An entity counted is a record:
//!   a unit, declared or derived, once whatever number of declarations it
//!   has.
//!
//! # What every case also checks
//!
//! - The table is the same (`RuleTable::id`) from the lines of `rules.txt`
//!   in their order and in the reverse order (WP-103 items 3 and 11).
//! - The post-image digested under the table twice, and under the table
//!   of the reversed lines, gives the same `Output` each time (item 11).
//! - Every record of the post-image under the table has a `Key` whose id
//!   is its `KeyId` (`records` is `Ok`: `spec/digest.md` §4.4).

mod common;

use std::collections::BTreeSet;
use std::path::Path;

use common::rules::{Line, Table, read, table};
use common::{Digested, Expect, Side};
use slip_digest::records::{Record, records};
use slip_digest::{Cause, Detail, Input, Options, Output};
use slip_model::digests::GroupClass;
use slip_model::keys::{Key, Name, Rule, RuleTable};

/// What a case's `expect.txt` says.
struct Expected {
    lines: BTreeSet<Expect>,
    exact: bool,
    /// `use` lines: the rule's line, `via`, `direct`.
    uses: Vec<(usize, u64, u64)>,
}

fn expected(dir: &Path) -> Result<Expected, String> {
    let text = std::fs::read_to_string(dir.join("expect.txt")).unwrap_or_default();
    let (lines, exact, own) = common::expect_text(&text, |l| l.starts_with("use "))?;
    let mut uses = Vec::new();
    for (n, l) in own {
        let w: Vec<&str> = l.split_whitespace().collect();
        let parsed = match w.as_slice() {
            ["use", line, "via", v, "direct", d] => {
                line.parse().ok().zip(v.parse().ok()).zip(d.parse().ok()).map(|((a, b), c)| (a, b, c))
            }
            _ => None,
        };
        uses.push(parsed.ok_or_else(|| {
            format!("expect.txt:{n}: `{l}` is not `use <line> via <n> direct <n>`")
        })?);
    }
    Ok(Expected { lines, exact, uses })
}

/// The lines of `rules.txt` (absent: none).
fn lines_of(dir: &Path) -> Result<Vec<(usize, Line)>, String> {
    match std::fs::read_to_string(dir.join("rules.txt")) {
        Ok(text) => read(&text),
        Err(_) => Ok(Vec::new()),
    }
}

fn digest_under(input: &Input<'_>, rules: &RuleTable) -> Output {
    slip_digest::digest(input, &Options { rules, detail: Detail::All, names: true })
}

/// The name no C++ entity has that replaces rule `n`'s target in its
/// probe table.
fn probe_name(n: usize) -> Name {
    Name::Identifier(format!("slip probe {n}").into_bytes())
}

/// The key a rule gives an entity it matches.
fn produced(r: &Rule) -> Key {
    Key { kind: r.kind, parent: r.to_parent, name: r.to_name.clone(), disc: r.disc }
}

/// What the post-image gives, from one parse.
struct Post {
    table: Table,
    digested: Digested,
    /// `(line, via, direct)` for each `use` line.
    uses: Vec<(usize, u64, u64)>,
    /// The post-image under no rule, when the table is not empty.
    unruled: Option<Output>,
}

/// `via` and `direct` of the rule of line `n` (see the head of this file).
fn rule_use(
    input: &Input<'_>,
    t: &Table,
    n: usize,
) -> Result<(u64, u64), String> {
    let Some((_, rule)) = t.rules.iter().find(|(l, _)| *l == n) else {
        return Err(format!("`use {n}`: no rule on line {n} of rules.txt"));
    };
    let probe = Rule { to_name: probe_name(n), ..rule.clone() };
    let probed: Vec<Rule> = t
        .rules
        .iter()
        .map(|(l, r)| if *l == n { probe.clone() } else { r.clone() })
        .collect();
    let probed = RuleTable::new(probed).map_err(|e| e.to_string())?;
    let recs = records(input, &probed)?;
    let count = |k: &Key| recs.iter().filter(|r| r.id == k.id()).count() as u64;
    Ok((count(&produced(&probe)), count(&produced(rule))))
}

fn post_side(dir: &Path, pre: &[Record], uses: &[(usize, u64, u64)]) -> Result<Post, String> {
    let lines = lines_of(dir)?;
    common::with_side(dir, Side::B, |input| -> Result<Post, String> {
        let plain = records(input, &RuleTable::empty())?;
        let t = table(&lines, pre, &plain)?;
        let mut reversed = lines.clone();
        reversed.reverse();
        let r = table(&reversed, pre, &plain)?;
        if r.table.id() != t.table.id() {
            return Err(format!(
                "the lines in reverse order give another table:\n{:#?}\n{:#?}",
                t.table.rules(),
                r.table.rules()
            ));
        }
        let first = digest_under(input, &t.table);
        if digest_under(input, &t.table) != first {
            return Err("the post-image under its table: two walks differ".into());
        }
        if digest_under(input, &r.table) != first {
            return Err("the post-image under the reversed table: the walks differ".into());
        }
        records(input, &t.table).map_err(|e| format!("records under the table: {e}"))?;
        let mut got = Vec::new();
        for &(n, _, _) in uses {
            let (v, d) = rule_use(input, &t, n)?;
            got.push((n, v, d));
        }
        let unruled = (!t.table.is_empty()).then(|| digest_under(input, &RuleTable::empty()));
        Ok(Post { table: t, digested: common::digested(first), uses: got, unruled })
    })?
}

/// The names of the units of `d` indeterminate for a cause other than
/// `Pending`, and their keys. A key of several units (a collision) has
/// every name the side gives its units.
fn hard(d: &Digested) -> (BTreeSet<String>, BTreeSet<slip_model::Digest>) {
    let (mut names, mut keys) = (BTreeSet::new(), BTreeSet::new());
    for (k, v) in &d.units {
        if v.iter().any(|u| u.flags & 1 != 0 && !matches!(u.cause, Some(Cause::Pending(_)) | None)) {
            keys.insert(*k);
            names.extend(d.output.names.iter().filter(|(x, _)| x == k).map(|(_, n)| n.clone()));
        }
    }
    (names, keys)
}

fn declared(d: &Digested, key: &slip_model::Digest) -> Vec<(GroupClass, slip_model::Digest, u64)> {
    d.units
        .get(key)
        .into_iter()
        .flatten()
        .filter(|u| u.class != GroupClass::Derived)
        .map(|u| (u.class, u.digest, u.flags))
        .collect()
}

/// The judgment of a case (the head of this file).
fn judge(e: &Expected, a: &Digested, b: &Digested) -> Result<(), String> {
    if e.lines.is_empty() {
        if e.exact {
            return Err("`exact derived` in an equal case".into());
        }
        let (ia, _) = hard(a);
        let (ib, _) = hard(b);
        let mut problems: Vec<String> = ia.iter().map(|n| format!("side A: indeterminate {n}")).collect();
        problems.extend(ib.iter().map(|n| format!("side B: indeterminate {n}")));
        if let Err(p) = common::pairs::judge_equal(a, b) {
            problems.push(p);
        }
        return if problems.is_empty() { Ok(()) } else { Err(problems.join("\n")) };
    }
    let (ia, ka) = hard(a);
    let (ib, kb) = hard(b);
    let indet: BTreeSet<_> = ka.union(&kb).copied().collect();
    let mut got: BTreeSet<Expect> = ia.union(&ib).cloned().map(Expect::Indeterminate).collect();
    let keys: BTreeSet<_> = a.units.keys().chain(b.units.keys()).copied().collect();
    for k in keys.iter().filter(|k| !indet.contains(k)) {
        let (da, db) = (declared(a, k), declared(b, k));
        let name = if a.units.contains_key(k) { a.name(k) } else { b.name(k) };
        match (da.is_empty(), db.is_empty()) {
            (true, true) => {}
            (false, true) => {
                got.insert(Expect::Removed(name));
            }
            (true, false) => {
                got.insert(Expect::Added(name));
            }
            (false, false) => {
                if da != db {
                    got.insert(Expect::Changed(name));
                }
            }
        }
    }
    let skipped: BTreeSet<String> = ia.union(&ib).cloned().collect();
    let dd: BTreeSet<Expect> = common::derived_differences(a, b)
        .into_iter()
        .filter(|x| !skipped.contains(x.name()))
        .collect();
    if e.exact {
        got.extend(dd);
    } else {
        for line in e.lines.iter().filter(|l| l.is_derived()) {
            let n = line.name().to_owned();
            let holds = match line {
                Expect::DerivedAdded(_) | Expect::DerivedRemoved(_) => dd.contains(line),
                _ => [Expect::DerivedAdded(n.clone()), Expect::DerivedRemoved(n.clone()), Expect::DerivedChanged(n)]
                    .iter()
                    .any(|x| dd.contains(x)),
            };
            if holds {
                got.insert(line.clone());
            }
        }
    }
    if got == e.lines {
        return Ok(());
    }
    let missing: Vec<String> = e.lines.difference(&got).map(Expect::line).collect();
    let extra: Vec<String> = got.difference(&e.lines).map(Expect::line).collect();
    Err(format!("expected but not seen: {missing:?}\nseen but not expected: {extra:?}"))
}

/// One case: its table, its two sides, the judgment and the counts.
fn case(dir: &Path) -> Result<(), String> {
    let e = expected(dir)?;
    let (pre, a) = common::with_side(dir, Side::A, |input| {
        let recs = records(input, &RuleTable::empty());
        (recs, digest_under(input, &RuleTable::empty()))
    })?;
    let pre = pre.map_err(|x| format!("records of the pre-image: {x}"))?;
    let a = common::digested(a);
    let post = post_side(dir, &pre, &e.uses)?;
    let mut problems = Vec::new();
    if let Err(p) = judge(&e, &a, &post.digested) {
        problems.push(p);
    }
    // An equal case under rules that would be equal without them shows
    // nothing of the rules, unless `rules.txt` says so (`# vacuous`).
    let vacuous_ok = std::fs::read_to_string(dir.join("rules.txt"))
        .is_ok_and(|t| t.lines().any(|l| l.trim() == "# vacuous"));
    if e.lines.is_empty()
        && !vacuous_ok
        && post.unruled.as_ref().is_some_and(|u| u.digests == a.output.digests)
    {
        problems.push("vacuous: the post-image without its rules is equal already".into());
    }
    for (&(n, v, d), &(_, gv, gd)) in e.uses.iter().zip(&post.uses) {
        if (v, d) != (gv, gd) {
            problems.push(format!("use {n}: expected via {v} direct {d}, counted via {gv} direct {gd}"));
        }
    }
    if std::env::var_os("SLIP_DIGEST_DUMP").is_some() {
        eprintln!("--- {}: {} rules", common::rel(dir), post.table.rules.len());
        for (n, r) in &post.table.rules {
            eprintln!("  line {n}: {r:?}");
        }
        for (side, d) in [("A", &a), ("B", &post.digested)] {
            for (k, v) in &d.units {
                for u in v {
                    eprintln!("  {side} {:?} {} {:?} flags={} {:?}", u.class, u.name, &k.0[..4], u.flags, u.cause);
                }
            }
        }
    }
    if problems.is_empty() { Ok(()) } else { Err(problems.join("\n")) }
}

/// Every case of `tests/rules/`.
#[test]
fn rules() {
    common::run_all("rules", case);
}

/// The reader of `rules.txt` (`common/rules.rs`): every form, and the
/// errors of the text.
#[test]
fn rules_txt_forms() {
    use common::rules::{DiscRef, Entity, Image, NameRef, ParentRef};
    let ent = |k: Option<slip_model::keys::KeyKind>, t: &str| Entity { kind: k, text: t.to_owned() };
    let text = "# a comment\n\n  relocate acme => :: legacy\nrelocate [function] a::f(int) => b :: =\n\
                rule function | mapped b | id g | disc-of pre a::f(int) | pre a | name-of pre a::f(int)\n\
                rule class | root | name-of post (anonymous struct for Q) | none | c_linkage | id P\n";
    let got = read(text).expect("the text reads");
    assert_eq!(got.len(), 4);
    assert_eq!(
        got[0],
        (3, Line::Relocate { entity: ent(None, "acme"), parent: None, name: Some("legacy".into()) })
    );
    assert_eq!(
        got[1],
        (
            4,
            Line::Relocate {
                entity: ent(Some(slip_model::keys::KeyKind::Function), "a::f(int)"),
                parent: Some(ent(None, "b")),
                name: None
            }
        )
    );
    assert_eq!(
        got[2].1,
        Line::Rule {
            kind: slip_model::keys::KeyKind::Function,
            parent: ParentRef::Mapped(ent(None, "b")),
            name: NameRef::Id("g".into()),
            disc: DiscRef::Of(Image::Pre, ent(None, "a::f(int)")),
            to_parent: ParentRef::Of(Image::Pre, ent(None, "a")),
            to_name: NameRef::Of(Image::Pre, ent(None, "a::f(int)")),
        }
    );
    assert_eq!(
        got[3].1,
        Line::Rule {
            kind: slip_model::keys::KeyKind::Class,
            parent: ParentRef::Root,
            name: NameRef::Of(Image::Post, ent(None, "(anonymous struct for Q)")),
            disc: DiscRef::None,
            to_parent: ParentRef::CLinkage,
            to_name: NameRef::Id("P".into()),
        }
    );
    for bad in [
        "move a => :: b",
        "relocate a :: b",
        "relocate a => b",
        "relocate a => :: 1b",
        "relocate [nokind] a => :: b",
        "relocate => :: b",
        "rule function | root | id f | none | root",
        "rule nokind | root | id f | none | root | id g",
        "rule function | elsewhere a | id f | none | root | id g",
        "rule function | root | id 1f | none | root | id g",
        "rule function | root | id f | some | root | id g",
    ] {
        assert!(read(bad).is_err(), "{bad}");
    }
}
