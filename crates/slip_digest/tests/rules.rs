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
//! - `use <n> via <v> direct <d> [kept_via <k>]`: the rule of line `<n>`
//!   of `rules.txt` counts `via(r) = v`, `direct(r) = d` and `kept_via(r)
//!   = k` (0 when the word is absent) in the post-image
//!   (`spec/verify.md` §9.1, §9.2). Not a line of §14.1: it is read here
//!   and says nothing of equality. The counts are the walk's
//!   (`Output::rule_use`, WP-069, WP-111; `tests/rule_use.rs` has the
//!   cases of §9.1 itself).
//! - `collisions <n>`: the post-image's counter `digest.collisions`
//!   (`spec/digest.md` §12) is `n`. Without the line it must be 0.
//! - `refused <text>`: the table cannot be made: the resolver or
//!   `RuleTable::new` refuses it with a message that contains `<text>`
//!   (a rule `Rule::refusal` refuses, `spec/verify.md` §2.1); and the
//!   same table made without the refusal keys nothing: every unit of the
//!   post-image under it but the order unit is indeterminate. A table
//!   `RuleTable::new` itself refuses (two rules that produce one key,
//!   `spec/verify.md` §2.1, WP-111 review F1) cannot be made without the
//!   refusal: that it is not made is the check. Nothing else is checked.
//! - `made <name>`, `fixed T <name>`, `fixed M <name>` (WP-146 stage 1):
//!   the post-image's lists of ownership (`Output::made`, `Output::fixed`,
//!   `own_lists.rs`), by the name each entry gives; `lists empty` says
//!   both are empty. Optional: with no such line the lists are not
//!   checked; with one, they are exactly the lines given (a name may
//!   repeat). On a mismatch, and under `SLIP_DIGEST_DUMP`, the runner
//!   prints the lists the walk gave.
//!
//! # `kept.txt` (WP-111)
//!
//! `Declared.kept` (`spec/verify.md` §2.3, §9.2): one display name per
//! line (`#` starts a comment), each the name of exactly one record of the
//! post-image under the table and the case's ownership, as `slip declare`
//! resolves a `kept` line; their keys are walked as `kept`. The walk must
//! then say every one of them is seen (`Output::kept`). Absent: `kept` is
//! empty.
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
//!   starts a comment): a header of the case, or `main.cpp` for the main
//!   file of both sides (the runner parses `a.cpp`, `b.cpp` and `t.cpp`
//!   as `main.cpp`, so those names, the runner's own files and a name
//!   that is no file are refused). The post-image is then digested with those files
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

/// The files of a case that are the runner's, not the parse's sources.
const CONTROL: &[&str] =
    &["expect.txt", "expect-owned.txt", "rules.txt", "readonly.txt", "relocate.txt", "kept.txt"];

/// `readonly.txt` (absent: `None`): each line a header of the case or
/// `main.cpp` (the main file of both sides).
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
        // The runner parses each side's main file (`a.cpp`, `b.cpp`, or
        // `t.cpp`) as `main.cpp`: that is the name that makes it
        // read-only. A line that names no file the parse has is refused:
        // it would have no effect.
        if matches!(l, "a.cpp" | "b.cpp" | "t.cpp") {
            return Err(format!(
                "readonly.txt:{}: `{l}` is parsed as `main.cpp`; name `main.cpp` for the main \
                 file of both sides",
                n + 1
            ));
        }
        if CONTROL.contains(&l) || (l != "main.cpp" && !dir.join(l).is_file()) {
            return Err(format!("readonly.txt:{}: `{l}` is no file the case parses", n + 1));
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
    /// `use` lines: the rule's line, `via`, `direct`, `kept_via`.
    uses: Vec<Count>,
    /// `collisions <n>` (absent: 0).
    collisions: u64,
    /// `refused <text>`.
    refused: Option<String>,
    /// The `made` and `fixed` lines, sorted; `lists empty` is an empty
    /// list; `None` when there is no such line.
    lists: Option<Vec<String>>,
}

fn expected(dir: &Path, file: &str) -> Result<Expected, String> {
    let text = std::fs::read_to_string(dir.join(file)).unwrap_or_default();
    let (lines, exact, own) = common::expect_text(&text, |l| {
        l.starts_with("use ")
            || l.starts_with("collisions ")
            || l.starts_with("refused ")
            || l.starts_with("made ")
            || l.starts_with("fixed ")
            || l == "lists empty"
    })?;
    let mut uses = Vec::new();
    let (mut collisions, mut refused) = (0, None);
    let mut lists: Option<Vec<String>> = None;
    for (n, l) in own {
        if l == "lists empty" {
            lists.get_or_insert_with(Vec::new);
            continue;
        }
        if l.starts_with("made ") || l.starts_with("fixed ") {
            let ok =
                l.starts_with("made ") || l.starts_with("fixed T ") || l.starts_with("fixed M ");
            if !ok {
                return Err(format!(
                    "{file}:{n}: `{l}` is not `fixed T <name>` or `fixed M <name>`"
                ));
            }
            lists.get_or_insert_with(Vec::new).push(l.trim().to_owned());
            continue;
        }
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
        let parsed =
            match w.as_slice() {
                ["use", line, "via", v, "direct", d, rest @ ..] => {
                    let k = match rest {
                        [] => Some(0),
                        ["kept_via", k] => k.parse().ok(),
                        _ => None,
                    };
                    line.parse().ok().zip(v.parse().ok()).zip(d.parse().ok()).zip(k).map(
                        |(((line, via), direct), kept_via)| Count { line, via, direct, kept_via },
                    )
                }
                _ => None,
            };
        uses.push(parsed.ok_or_else(|| {
            format!("expect.txt:{n}: `{l}` is not `use <line> via <n> direct <n> [kept_via <n>]`")
        })?);
    }
    if let Some(l) = lists.as_mut() {
        l.sort();
    }
    Ok(Expected { lines, exact, uses, collisions, refused, lists })
}

/// The lines of `rules.txt` (absent: none).
fn lines_of(dir: &Path) -> Result<Vec<(usize, Line)>, String> {
    match std::fs::read_to_string(dir.join("rules.txt")) {
        Ok(text) => read(&text),
        Err(_) => Ok(Vec::new()),
    }
}

/// One `use` line: the rule's line in `rules.txt` and its three counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Count {
    line: usize,
    via: u64,
    direct: u64,
    kept_via: u64,
}

/// `kept.txt` (absent: none): the display names, in order.
fn kept_names(dir: &Path) -> Vec<String> {
    let text = std::fs::read_to_string(dir.join("kept.txt")).unwrap_or_default();
    text.lines()
        .map(|l| l.split('#').next().unwrap_or("").trim())
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect()
}

/// The keys of `names` among `recs`, ascending: each the name of exactly
/// one record.
fn kept_keys(names: &[String], recs: &[Record]) -> Result<Vec<slip_model::Digest>, String> {
    let mut out = Vec::new();
    for n in names {
        let ids: BTreeSet<_> = recs.iter().filter(|r| &r.name == n).map(|r| r.id).collect();
        match ids.len() {
            1 => out.extend(ids),
            0 => return Err(format!("kept.txt: `{n}` names no record of the post-image")),
            _ => return Err(format!("kept.txt: `{n}` names {} keys", ids.len())),
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

fn digest_under(input: &Input<'_>, rules: &RuleTable, read_only: ReadOnly<'_>) -> Output {
    digest_with(input, rules, read_only, false, &[])
}

fn digest_with(
    input: &Input<'_>,
    rules: &RuleTable,
    read_only: ReadOnly<'_>,
    rule_use: bool,
    kept: &[slip_model::Digest],
) -> Output {
    slip_digest::digest_kept(
        input,
        &Options { rules, read_only, detail: Detail::All, names: true, rule_use },
        kept,
    )
}

/// What the post-image gives, from one parse.
struct Post {
    table: Table,
    digested: Digested,
    /// The counts of each `use` line's rule.
    uses: Vec<Count>,
    /// The post-image under no rule, when the table is not empty.
    unruled: Option<Output>,
}

/// The counts of the rule of line `n` in the walk's `rule_use` (0 for a
/// rule with no `Use`).
fn rule_use(out: &Output, t: &Table, n: usize) -> Result<Count, String> {
    let Some((_, rule)) = t.rules.iter().find(|(l, _)| *l == n) else {
        return Err(format!("`use {n}`: no rule on line {n} of rules.txt"));
    };
    let index = t.table.rules().iter().position(|r| r == rule).map(|i| i as u64);
    let u = out.rule_use.iter().find(|u| Some(u.rule) == index);
    Ok(Count {
        line: n,
        via: u.map_or(0, |u| u.via),
        direct: u.map_or(0, |u| u.direct),
        kept_via: u.map_or(0, |u| u.kept_via),
    })
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
        let kept = match common::rules::table_with(&lines, pre, &plain, false) {
            Ok(t) => t,
            // No door makes it (`RuleTable::new`, and `from_sorted` behind a
            // container: `crates/slip_model/tests/keys.rs`).
            Err(e) if e.contains("not a table") => return Ok(()),
            Err(e) => return Err(e),
        };
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
    uses: &[Count],
    own: &Own,
    properties: bool,
) -> Result<Post, String> {
    let lines = lines_of(dir)?;
    common::with_side(dir, Side::B, |input| -> Result<Post, String> {
        let plain = records(input, &RuleTable::empty(), ReadOnly::EVERY_FILE)?;
        let t = table(&lines, pre, &plain)?;
        with_own(input, own, move |ro| -> Result<Post, String> {
            let recs = records(input, &t.table, ro)
                .map_err(|e| format!("records under the table: {e}"))?;
            let kept = kept_keys(&kept_names(dir), &recs)?;
            let first = digest_with(input, &t.table, ro, true, &kept);
            if digest_with(input, &t.table, ro, true, &kept) != first {
                return Err("the post-image under its table: two walks differ".into());
            }
            if first.kept != (0..kept.len() as u64).collect::<Vec<_>>() {
                return Err(format!("kept: {} keys, the walk saw {:?}", kept.len(), first.kept));
            }
            if std::env::var_os("SLIP_DIGEST_DUMP").is_some() {
                // WP-146's inertness check: every field the walk gave
                // before its lists, in full (not the lists themselves).
                let o = &first;
                eprintln!("=== {} post {own:?}", common::rel(dir));
                eprintln!("  digests {:?}", o.digests);
                eprintln!("  counters {:?}", o.counters);
                eprintln!("  causes {:?}", o.causes);
                eprintln!("  names {:?}", o.names);
                eprintln!("  rule_use {:?}", o.rule_use);
                eprintln!("  kept {:?}", o.kept);
                eprintln!("  name_strings {:?}", o.name_strings);
            }
            let off = digest_under(input, &t.table, ro);
            let cleared = Output {
                rule_use: Vec::new(),
                kept: Vec::new(),
                made: Vec::new(),
                fixed: Vec::new(),
                ..first.clone()
            };
            if !off.rule_use.is_empty()
                || !off.kept.is_empty()
                || !off.made.is_empty()
                || !off.fixed.is_empty()
                || cleared != off
            {
                return Err("the post-image under its table: rule_use on and off differ".into());
            }
            slip_digest::records::key_agreement(input, &t.table, ro)
                .map_err(|e| format!("key agreement ({own:?}): {e}"))?;
            let mut got = Vec::new();
            for c in uses {
                got.push(rule_use(&first, &t, c.line)?);
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
    let lines = common::pairs::without_names(&e.lines);
    if lines.is_empty() {
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
        problems.extend(common::pairs::judge_names(&e.lines, a, b, true));
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
        for line in lines.iter().filter(|l| l.is_derived()) {
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
    let mut problems = common::pairs::judge_names(&e.lines, a, b, false);
    if got != lines {
        let missing: Vec<String> = lines.difference(&got).map(Expect::line).collect();
        let extra: Vec<String> = got.difference(&lines).map(Expect::line).collect();
        problems
            .push(format!("expected but not seen: {missing:?}\nseen but not expected: {extra:?}"));
    }
    if problems.is_empty() { Ok(()) } else { Err(problems.join("\n")) }
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
    for (w, g) in e.uses.iter().zip(&post.uses) {
        if w != g {
            problems.push(format!(
                "use {}: expected via {} direct {} kept_via {}, counted via {} direct {} \
                 kept_via {}",
                w.line, w.via, w.direct, w.kept_via, g.via, g.direct, g.kept_via
            ));
        }
    }
    let lists = list_lines(&post.digested.output);
    if let Some(want) = &e.lists
        && *want != lists
    {
        problems.push(format!(
            "lists: expected\n  {}\nthe walk gave\n  {}",
            if want.is_empty() { "lists empty".to_owned() } else { want.join("\n  ") },
            if lists.is_empty() { "lists empty".to_owned() } else { lists.join("\n  ") }
        ));
    }
    if std::env::var_os("SLIP_DIGEST_DUMP").is_some() {
        for l in &lists {
            eprintln!("  lists {} {own:?}: {l}", common::rel(dir));
        }
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

/// The lists of ownership of `o` as `expect.txt` writes them (`made
/// <name>`, `fixed T|M <name>`), sorted.
fn list_lines(o: &Output) -> Vec<String> {
    let mut out: Vec<String> = o.made.iter().map(|m| format!("made {}", m.name)).collect();
    out.extend(o.fixed.iter().map(|f| {
        let t = match f.as_ {
            slip_digest::FixedAs::T => "T",
            slip_digest::FixedAs::M => "M",
        };
        format!("fixed {t} {}", f.name)
    }));
    out.sort();
    out
}

/// Every case of `tests/rules/`.
#[test]
fn rules() {
    common::run_all("rules", case);
}

/// WP-146 stage 1: every entry of the lists of `observers/places`, in
/// full: its rule (made), its test (fixed), its name and its place. The
/// fixed entry `old::f()` is at its first declaration in the read-only
/// d.h, not at its first declaration (in the owned main file).
#[test]
fn wp146_list_places() {
    let dir = common::manifest().join("tests/rules/observers/places");
    let pre = common::with_side(&dir, Side::A, |input| {
        records(input, &RuleTable::empty(), ReadOnly::EVERY_FILE)
    })
    .unwrap()
    .unwrap();
    let own = Own::Listed(readonly_of(&dir).unwrap().unwrap());
    let post = post_side(&dir, &pre, &[], &own, false).unwrap();
    let o = &post.digested.output;
    let place = |f: &[u8], l: u64, c: u64| format!("{}:{l}:{c}", String::from_utf8_lossy(f));
    let made: Vec<String> = o
        .made
        .iter()
        .map(|m| format!("{} rule {} {}", m.name, m.rule, place(&m.file, m.line, m.column)))
        .collect();
    let fixed: Vec<String> = o
        .fixed
        .iter()
        .map(|f| format!("{} {:?} {}", f.name, f.as_, place(&f.file, f.line, f.column)))
        .collect();
    let mut made = made;
    let mut fixed = fixed;
    made.sort();
    fixed.sort();
    assert_eq!(made, PLACES_MADE, "made");
    assert_eq!(fixed, PLACES_FIXED, "fixed");
}

const PLACES_MADE: &[&str] =
    &["fresh::C rule 0 main.cpp:4:26", "fresh::inner2::Z rule 0 main.cpp:4:69"];
const PLACES_FIXED: &[&str] = &["old::f() T d.h:1:22", "old::inner::Y T d.h:2:42"];

/// The reader of `rules.txt` (`common/rules.rs`): every form, and the
/// errors of the text.
/// WP-110 (the re-check): `readonly.txt` refuses a line that would have
/// no effect, with a message; `main.cpp` and a header are accepted.
#[test]
fn readonly_txt_refuses_lines_with_no_effect() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("rules-readonly-txt");
    std::fs::create_dir_all(&dir).unwrap();
    for (name, text) in [("a.cpp", ""), ("b.cpp", ""), ("dep.h", ""), ("rules.txt", "")] {
        std::fs::write(dir.join(name), text).unwrap();
    }
    let with = |lines: &str| {
        std::fs::write(dir.join("readonly.txt"), lines).unwrap();
        readonly_of(&dir)
    };
    assert_eq!(
        with("dep.h\nmain.cpp # both mains\n"),
        Ok(Some(vec!["dep.h".into(), "main.cpp".into()]))
    );
    for (line, says) in [
        ("b.cpp", "is parsed as `main.cpp`"),
        ("a.cpp", "is parsed as `main.cpp`"),
        ("t.cpp", "is parsed as `main.cpp`"),
        ("rules.txt", "is no file the case parses"),
        ("nothere.h", "is no file the case parses"),
    ] {
        let e = with(&format!("dep.h\n{line}\n")).unwrap_err();
        assert!(e.starts_with("readonly.txt:2: ") && e.contains(says), "{line}: {e}");
    }
    assert_eq!(with("# nothing\n"), Err("readonly.txt names no file".into()));
}

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
