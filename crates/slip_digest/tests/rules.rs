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
//!   says nothing of equality. The counts are the walk's
//!   (`Output::rule_use`, WP-069; `tests/rule_use.rs` has the cases of
//!   §9.1 itself).
//! - `collisions <n>`: the post-image's counter `digest.collisions`
//!   (`spec/digest.md` §12) is `n`. Without the line it must be 0.
//! - `refused <text>`: the table cannot be made: the resolver or
//!   `RuleTable::new` refuses it with a message that contains `<text>`
//!   (a rule `Rule::refusal` refuses, `spec/verify.md` §2.1); and the
//!   same table made without the refusal keys nothing: every unit of the
//!   post-image under it but the order unit is indeterminate. Nothing
//!   else is checked.
//!
//! # What every case also checks
//!
//! - The post-image digested under the table twice gives the same
//!   `Output` (item 11), and digested with `rule_use` off the same
//!   `Output` but for an empty `rule_use` (`spec/verify.md` §9.5). (A table made from the lines in another order
//!   is not a second check of the walk: `RuleTable::new` sorts its rules,
//!   and under `spec/verify.md` §2.3 the order of `relocate` lines is not
//!   free.)
//! - Every record of the post-image under the table has a `Key` whose id
//!   is its `KeyId` (`records` is `Ok`: `spec/digest.md` §4.4).
//!
//! # Ownership (WP-110, `spec/verify.md` §2.1)
//!
//! - `readonly.txt`: the case's read-only files, one name per line (`#`
//!   starts a comment). The post-image is then digested with those files
//!   read-only, as the verify job gives them (`ReadOnlyFiles` over an
//!   `Ownership` in subject paths, the case's directory as the root
//!   `/case`), and judged by `expect.txt`; and with every file owned,
//!   judged by `expect-owned.txt`, which such a case must have: the pair
//!   shows what ownership changes.
//! - A case without `readonly.txt` is digested with every file owned and
//!   judged by `expect.txt`, as before WP-110.
//! - Every case not refused: under its table with every file read-only,
//!   the post-image's digests are those under no rule (every key is the
//!   key of the empty table, rule 1 and rule 2 together).
//! - Every case, under each of its ownerships: `key_agreement` holds (a
//!   key the walk memoised for a reference is the key the entity gets
//!   alone, in a fresh walk, in another order).

mod common;

use std::collections::BTreeSet;
use std::path::Path;

use common::rules::{Line, Table, read, table};
use common::{Digested, Expect, Side};
use slip_digest::records::{Record, records};
use slip_digest::{Cause, Detail, Input, Options, Output, ReadOnly, ReadOnlyFiles};
use slip_model::digests::GroupClass;
use slip_model::keys::RuleTable;
use slip_model::transform::Ownership;

/// Which files of a case's post-image are read-only.
#[derive(Clone, Debug)]
enum Own {
    /// Every file owned (WP-103's behaviour).
    All,
    /// The files of `readonly.txt`.
    Listed(Vec<String>),
}

/// `readonly.txt` (absent: `None`).
fn readonly_of(dir: &Path) -> Result<Option<Vec<String>>, String> {
    let Ok(text) = std::fs::read_to_string(dir.join("readonly.txt")) else {
        return Ok(None);
    };
    let mut out = Vec::new();
    for (n, l) in text.lines().enumerate() {
        let l = l.split('#').next().unwrap_or("").trim();
        if l.is_empty() {
            continue;
        }
        if !dir.join(l).is_file() {
            return Err(format!("readonly.txt:{}: `{l}` is no file of the case", n + 1));
        }
        out.push(l.to_owned());
    }
    if out.is_empty() {
        return Err("readonly.txt names no file".into());
    }
    Ok(Some(out))
}

/// The case's subject root.
const ROOT: &str = "/case";

/// The read-only files of `input` as the verify job decides them
/// (`ReadOnlyFiles::new`): an `Ownership` in subject paths, the case's
/// files under [`ROOT`], the listed ones read-only; a buffer that is no
/// file has the empty path.
fn listed_files(input: &Input<'_>, listed: &[String]) -> ReadOnlyFiles {
    let mut readonly: Vec<Vec<u8>> =
        listed.iter().map(|n| format!("{ROOT}/{n}").into_bytes()).collect();
    readonly.sort();
    let o = Ownership { owned: vec![ROOT.as_bytes().to_vec()], readonly };
    let subject = |f| {
        let p = (input.subject_path)(f);
        if p.is_empty() { p } else { [format!("{ROOT}/").as_bytes(), &p].concat() }
    };
    ReadOnlyFiles::new(Some(&o), input.sm, &subject)
}

/// Runs `f` with the `ReadOnly` of `own` over `input`.
fn with_own<R>(input: &Input<'_>, own: &Own, f: impl FnOnce(ReadOnly<'_>) -> R) -> R {
    match own {
        Own::All => f(ReadOnly::NO_FILE),
        Own::Listed(l) => {
            let files = listed_files(input, l);
            f(ReadOnly::files(&files))
        }
    }
}

/// What a case's `expect.txt` says.
struct Expected {
    lines: BTreeSet<Expect>,
    exact: bool,
    /// `use` lines: the rule's line, `via`, `direct`.
    uses: Vec<(usize, u64, u64)>,
    /// `collisions <n>` (absent: 0).
    collisions: u64,
    /// `refused <text>`.
    refused: Option<String>,
}

fn expected(dir: &Path, file: &str) -> Result<Expected, String> {
    let text = std::fs::read_to_string(dir.join(file)).unwrap_or_default();
    let (lines, exact, own) = common::expect_text(&text, |l| {
        l.starts_with("use ") || l.starts_with("collisions ") || l.starts_with("refused ")
    })?;
    let mut uses = Vec::new();
    let (mut collisions, mut refused) = (0, None);
    for (n, l) in own {
        if let Some(c) = l.strip_prefix("collisions ") {
            collisions = c
                .trim()
                .parse()
                .map_err(|_| format!("expect.txt:{n}: `{l}` is not `collisions <n>`"))?;
            continue;
        }
        if let Some(t) = l.strip_prefix("refused ") {
            refused = Some(t.trim().to_owned());
            continue;
        }
        let w: Vec<&str> = l.split_whitespace().collect();
        let parsed = match w.as_slice() {
            ["use", line, "via", v, "direct", d] => line
                .parse()
                .ok()
                .zip(v.parse().ok())
                .zip(d.parse().ok())
                .map(|((a, b), c)| (a, b, c)),
            _ => None,
        };
        uses.push(parsed.ok_or_else(|| {
            format!("expect.txt:{n}: `{l}` is not `use <line> via <n> direct <n>`")
        })?);
    }
    Ok(Expected { lines, exact, uses, collisions, refused })
}

/// The lines of `rules.txt` (absent: none).
fn lines_of(dir: &Path) -> Result<Vec<(usize, Line)>, String> {
    match std::fs::read_to_string(dir.join("rules.txt")) {
        Ok(text) => read(&text),
        Err(_) => Ok(Vec::new()),
    }
}

fn digest_under(input: &Input<'_>, rules: &RuleTable, read_only: ReadOnly<'_>) -> Output {
    digest_with(input, rules, read_only, false)
}

fn digest_with(
    input: &Input<'_>,
    rules: &RuleTable,
    read_only: ReadOnly<'_>,
    rule_use: bool,
) -> Output {
    slip_digest::digest(
        input,
        &Options { rules, read_only, detail: Detail::All, names: true, rule_use },
    )
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

/// `via` and `direct` of the rule of line `n` in the walk's `rule_use`
/// (0 for a rule with no `Use`).
fn rule_use(out: &Output, t: &Table, n: usize) -> Result<(u64, u64), String> {
    let Some((_, rule)) = t.rules.iter().find(|(l, _)| *l == n) else {
        return Err(format!("`use {n}`: no rule on line {n} of rules.txt"));
    };
    let index = t.table.rules().iter().position(|r| r == rule).map(|i| i as u64);
    Ok(out.rule_use.iter().find(|u| Some(u.rule) == index).map_or((0, 0), |u| (u.via, u.direct)))
}

/// The table of a case whose `expect.txt` says `refused <text>`: it must
/// be refused, with `<text>` in the message. And the same table made
/// without the refusal (as a container may hold it, `Rule::refusal`)
/// must key nothing: every unit of the post-image under it but the
/// order unit is indeterminate, under the table as a `Declared` container
/// writes and reads it.
fn refused_side(dir: &Path, pre: &[Record], text: &str) -> Result<(), String> {
    let lines = lines_of(dir)?;
    common::with_side(dir, Side::B, |input| -> Result<(), String> {
        let plain = records(input, &RuleTable::empty(), ReadOnly::EVERY_FILE)?;
        match table(&lines, pre, &plain) {
            Ok(t) => return Err(format!("the table is not refused: {:#?}", t.table.rules())),
            Err(e) if e.contains(text) => {}
            Err(e) => return Err(format!("refused, but not with `{text}`: {e}")),
        }
        let kept = common::rules::table_with(&lines, pre, &plain, false)?;
        if kept.table.refused().is_none() {
            return Err("the table made without the refusal holds no refused rule".into());
        }
        // Through the container end to end, as `slip verify` reads it: a
        // `Declared` holding the table (and, as it must, the run's files),
        // written and read back.
        let declared = slip_model::declared::Declared::new(
            kept.table.rules().to_vec(),
            vec![],
            vec![],
            vec![],
        )
        .map_err(|e| format!("the container refuses the table: {e}"))?
        .with_files(Some(Ownership { owned: vec![ROOT.as_bytes().to_vec()], readonly: vec![] }));
        let read = slip_model::declared::Declared::from_bytes(&declared.to_bytes())
            .map_err(|e| format!("the container cannot be read: {e}"))?;
        if read.rules().refused().is_none() {
            return Err("the table read from the container holds no refused rule".into());
        }
        let d = common::digested(digest_under(input, read.rules(), ReadOnly::NO_FILE));
        let keyed: Vec<&str> = d
            .units
            .values()
            .flatten()
            .filter(|u| u.flags & 1 == 0 && u.name != "<order>")
            .map(|u| u.name.as_str())
            .collect();
        if !keyed.is_empty() {
            return Err(format!("units keyed under a refused table: {keyed:?}"));
        }
        Ok(())
    })?
}

/// The post-image under the case's table with the files of `own`
/// read-only; and, once per case (`properties`), the two properties of
/// the head of this file.
fn post_side(
    dir: &Path,
    pre: &[Record],
    uses: &[(usize, u64, u64)],
    own: &Own,
    properties: bool,
) -> Result<Post, String> {
    let lines = lines_of(dir)?;
    common::with_side(dir, Side::B, |input| -> Result<Post, String> {
        let plain = records(input, &RuleTable::empty(), ReadOnly::EVERY_FILE)?;
        let t = table(&lines, pre, &plain)?;
        with_own(input, own, move |ro| -> Result<Post, String> {
            let first = digest_with(input, &t.table, ro, true);
            if digest_with(input, &t.table, ro, true) != first {
                return Err("the post-image under its table: two walks differ".into());
            }
            let off = digest_under(input, &t.table, ro);
            if !off.rule_use.is_empty() || (Output { rule_use: Vec::new(), ..first.clone() }) != off
            {
                return Err("the post-image under its table: rule_use on and off differ".into());
            }
            records(input, &t.table, ro).map_err(|e| format!("records under the table: {e}"))?;
            slip_digest::records::key_agreement(input, &t.table, ro)
                .map_err(|e| format!("key agreement ({own:?}): {e}"))?;
            let mut got = Vec::new();
            for &(n, _, _) in uses {
                let (v, d) = rule_use(&first, &t, n)?;
                got.push((n, v, d));
            }
            let unruled = (!t.table.is_empty())
                .then(|| digest_under(input, &RuleTable::empty(), ReadOnly::EVERY_FILE));
            if properties && let Some(u) = &unruled {
                // Every file read-only: every key is the empty table's.
                let none = digest_under(input, &t.table, ReadOnly::EVERY_FILE);
                if none.digests != u.digests || none.causes != u.causes {
                    return Err(
                        "every file read-only: the digests are not those under no rule".into()
                    );
                }
                slip_digest::records::key_agreement(input, &t.table, ReadOnly::EVERY_FILE)
                    .map_err(|e| format!("key agreement (every file read-only): {e}"))?;
            }
            Ok(Post { table: t, digested: common::digested(first), uses: got, unruled })
        })
    })?
}

/// The names of the units of `d` indeterminate for a cause other than
/// `Pending`, and their keys. A key of several units (a collision) has
/// every name the side gives its units.
fn hard(d: &Digested) -> (BTreeSet<String>, BTreeSet<slip_model::Digest>) {
    let (mut names, mut keys) = (BTreeSet::new(), BTreeSet::new());
    for (k, v) in &d.units {
        if v.iter().any(|u| u.flags & 1 != 0 && !matches!(u.cause, Some(Cause::Pending(_)) | None))
        {
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
        let mut problems: Vec<String> =
            ia.iter().map(|n| format!("side A: indeterminate {n}")).collect();
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
                _ => [
                    Expect::DerivedAdded(n.clone()),
                    Expect::DerivedRemoved(n.clone()),
                    Expect::DerivedChanged(n),
                ]
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

/// One case: its table, its two sides, the judgment and the counts, under
/// each of its ownerships (`readonly.txt`: the listed files read-only by
/// `expect.txt`, every file owned by `expect-owned.txt`; else every file
/// owned by `expect.txt`).
fn case(dir: &Path) -> Result<(), String> {
    let (pre, a) = common::with_side(dir, Side::A, |input| {
        let recs = records(input, &RuleTable::empty(), ReadOnly::EVERY_FILE);
        (recs, digest_under(input, &RuleTable::empty(), ReadOnly::EVERY_FILE))
    })?;
    let pre = pre.map_err(|x| format!("records of the pre-image: {x}"))?;
    let a = common::digested(a);
    let runs: Vec<(Own, &str)> = match readonly_of(dir)? {
        None => vec![(Own::All, "expect.txt")],
        Some(l) => {
            if !dir.join("expect-owned.txt").is_file() {
                return Err("a case with readonly.txt has an expect-owned.txt".into());
            }
            vec![(Own::Listed(l), "expect.txt"), (Own::All, "expect-owned.txt")]
        }
    };
    let mut problems = Vec::new();
    for (i, (own, file)) in runs.iter().enumerate() {
        let e = expected(dir, file)?;
        if let Some(text) = &e.refused {
            return refused_side(dir, &pre, text);
        }
        if let Err(p) = one(dir, &pre, &a, &e, own, i == 0) {
            problems.push(format!("[{file}, {own:?}] {p}"));
        }
    }
    if problems.is_empty() { Ok(()) } else { Err(problems.join("\n")) }
}

/// One ownership of a case, judged by `e`.
fn one(
    dir: &Path,
    pre: &[Record],
    a: &Digested,
    e: &Expected,
    own: &Own,
    properties: bool,
) -> Result<(), String> {
    let post = post_side(dir, pre, &e.uses, own, properties)?;
    let mut problems = Vec::new();
    let collisions = post
        .digested
        .output
        .counters
        .iter()
        .find(|c| c.name == "digest.collisions")
        .map_or(0, |c| c.value);
    if collisions != e.collisions {
        problems.push(format!("digest.collisions is {collisions}, expected {}", e.collisions));
    }
    if let Err(p) = judge(e, a, &post.digested) {
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
            problems.push(format!(
                "use {n}: expected via {v} direct {d}, counted via {gv} direct {gd}"
            ));
        }
    }
    if std::env::var_os("SLIP_DIGEST_DUMP").is_some() {
        eprintln!("--- {}: {} rules, {own:?}", common::rel(dir), post.table.rules.len());
        for (n, r) in &post.table.rules {
            eprintln!("  line {n}: {r:?}");
        }
        for (side, d) in [("A", a), ("B", &post.digested)] {
            for (k, v) in &d.units {
                for u in v {
                    eprintln!(
                        "  {side} {:?} {} {:?} flags={} {:?}",
                        u.class,
                        u.name,
                        &k.0[..4],
                        u.flags,
                        u.cause
                    );
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
    let ent =
        |k: Option<slip_model::keys::KeyKind>, t: &str| Entity { kind: k, text: t.to_owned() };
    let text = "# a comment\n\n  relocate acme => :: legacy\nrelocate [function] a::f(int) => b :: =\n\
                rule function | mapped b | id g | disc-of pre a::f(int) | pre a | name-of pre a::f(int)\n\
                rule class | root | name-of post (anonymous struct for Q) | none | c_linkage | id P\n";
    let got = read(text).expect("the text reads");
    assert_eq!(got.len(), 4);
    assert_eq!(
        got[0],
        (
            3,
            Line::Relocate { entity: ent(None, "acme"), parent: None, name: Some("legacy".into()) }
        )
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
