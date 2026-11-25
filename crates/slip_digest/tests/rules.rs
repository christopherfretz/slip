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
//!   checked; with one, they are exactly the lines given, in the lists'
//!   order (made, then fixed, each by its entries' order: key first), not
//!   sorted by the runner. A made entry with no post-image key (`Made::post`
//!   `None`) is `made <name> no-post`. Whatever `expect.txt` says, each
//!   list must be strictly ascending. On a mismatch, and under
//!   `SLIP_DIGEST_DUMP`, the runner prints the lists the walk gave.
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
//!
//! # Rebuilt files (WP-118, `spec/verify.md` §2.1)
//!
//! - `rebuilt.txt`: the case's rebuilt files, in the form of
//!   `readonly.txt` and with its refusals; a name in both files is
//!   refused. Such a case is digested (1) with its `readonly.txt` files
//!   read-only and its `rebuilt.txt` files rebuilt, judged by
//!   `expect.txt`; (2) with the `rebuilt.txt` files read-only as well,
//!   judged by `expect-undeclared.txt`; (3) with every file owned, judged
//!   by `expect-owned.txt`. Both files are required.
//! - P1: with the `rebuilt.txt` files owned and the rest as (1), the
//!   digests and the rule use equal (1)'s, unless the case has
//!   `expect-as-owned.txt`, which then judges that ownership (an entity
//!   with C language linkage declared in a rebuilt file, R2.2).
//! - `relied <file>…` (the entries relied on, in `rebuilt.txt`'s sorted
//!   order, one line) and `declares <file> rule <n> mapped <m> left <l>
//!   [left_kept <k>]` (`<n>` the rule's line in `rules.txt`, as in `use`)
//!   are lines of `expect.txt`, checked in every case that has
//!   `rebuilt.txt` against the walk's `Output::rebuilt`: no line means
//!   none, and a line the walk does not give, or a row with no line, fails
//!   the case. The other files of such a case, and every case without
//!   `rebuilt.txt`, give none.
//! - P2: every case that has `readonly.txt` and no `rebuilt.txt` is
//!   digested once more with its `readonly.txt` files rebuilt: the
//!   digests and the rule use equal the run with every file owned, unless
//!   the case has `rebuilt-differs.txt`, whose first line is the reason
//!   (it must name C language linkage) and whose other lines judge that
//!   run in `expect.txt`'s form.

mod common;

use std::collections::BTreeSet;
use std::path::Path;

use common::rules::{Line, Table, read, table};
use common::{Digested, Expect, Side};
use slip_digest::records::{Record, records};
use slip_digest::{Cause, Detail, Input, Options, Output, ReadOnly, ReadOnlyFiles, RebuiltOutput};
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
    /// WP-118: the files of `readonly` read-only, those of `rebuilt`
    /// rebuilt, the rest owned.
    Rebuilt { readonly: Vec<String>, rebuilt: Vec<String> },
}

/// The files of a case that are the runner's, not the parse's sources.
const CONTROL: &[&str] = &[
    "expect.txt",
    "expect-owned.txt",
    "rules.txt",
    "readonly.txt",
    "relocate.txt",
    "kept.txt",
    "rebuilt.txt",
    "expect-undeclared.txt",
    "expect-as-owned.txt",
    "rebuilt-differs.txt",
];

/// `readonly.txt` (absent: `None`): each line a header of the case or
/// `main.cpp` (the main file of both sides).
fn readonly_of(dir: &Path) -> Result<Option<Vec<String>>, String> {
    listed_of(dir, "readonly.txt")
}

/// `rebuilt.txt` (absent: `None`), in the form of `readonly.txt` (WP-118).
fn rebuilt_of(dir: &Path) -> Result<Option<Vec<String>>, String> {
    listed_of(dir, "rebuilt.txt")
}

/// A list of the case's files, `file` (absent: `None`): each line a
/// header of the case or `main.cpp` (the main file of both sides).
fn listed_of(dir: &Path, file: &str) -> Result<Option<Vec<String>>, String> {
    let Ok(text) = std::fs::read_to_string(dir.join(file)) else {
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
                "{file}:{}: `{l}` is parsed as `main.cpp`; name `main.cpp` for the main \
                 file of both sides",
                n + 1
            ));
        }
        if CONTROL.contains(&l) || (l != "main.cpp" && !dir.join(l).is_file()) {
            return Err(format!("{file}:{}: `{l}` is no file the case parses", n + 1));
        }
        out.push(l.to_owned());
    }
    if out.is_empty() {
        return Err(format!("{file} names no file"));
    }
    out.sort();
    out.dedup();
    Ok(Some(out))
}

/// The case's subject root.
const ROOT: &str = "/case";

/// The read-only files of `input` as the verify job decides them
/// (`ReadOnlyFiles::new`): an `Ownership` in subject paths, the case's
/// files under [`ROOT`], the listed ones read-only; a buffer that is no
/// file has the empty path.
fn listed_files(input: &Input<'_>, listed: &[String], rebuilt: &[String]) -> ReadOnlyFiles {
    let paths = |l: &[String]| {
        let mut v: Vec<Vec<u8>> = l.iter().map(|n| format!("{ROOT}/{n}").into_bytes()).collect();
        v.sort();
        v
    };
    let (readonly, rebuilt) = (paths(listed), paths(rebuilt));
    let o = Ownership { owned: vec![ROOT.as_bytes().to_vec()], readonly, rebuilt };
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
            let files = listed_files(input, l, &[]);
            f(ReadOnly::files(&files))
        }
        Own::Rebuilt { readonly, rebuilt } => {
            let files = listed_files(input, readonly, rebuilt);
            f(ReadOnly::files(&files))
        }
    }
}

/// The rebuilt files of `own`, sorted (the order of `Ownership.rebuilt`).
fn rebuilt_names(own: &Own) -> &[String] {
    match own {
        Own::Rebuilt { rebuilt, .. } => rebuilt,
        Own::All | Own::Listed(_) => &[],
    }
}

/// The `relied` and `declares` lines of `r` (WP-118), as `expect.txt`
/// writes them: the entries by their names in `own`'s rebuilt list, the
/// files by their names in the case, the rules by their lines in
/// `rules.txt`.
fn rebuilt_lines(t: &Table, own: &Own, r: &RebuiltOutput) -> Result<BTreeSet<String>, String> {
    let names = rebuilt_names(own);
    let mut out = BTreeSet::new();
    if !r.relied.is_empty() {
        let mut words = Vec::new();
        for &i in &r.relied {
            let n = names.get(i as usize).ok_or_else(|| format!("relied: no entry {i}"))?;
            words.push(n.clone());
        }
        out.insert(format!("relied {}", words.join(" ")));
    }
    if !r.declares.windows(2).all(|w| (&w[0].file, w[0].rule) < (&w[1].file, w[1].rule)) {
        return Err(format!("declares is not strictly ascending: {:?}", r.declares));
    }
    for d in &r.declares {
        if d.mapped == 0 && d.left == 0 && d.left_kept == 0 {
            return Err(format!("a declares row counts nothing: {d:?}"));
        }
        let file = String::from_utf8_lossy(&d.file).into_owned();
        let rule =
            t.table.rules().get(d.rule).ok_or_else(|| format!("declares: no rule {}", d.rule))?;
        let line = t
            .rules
            .iter()
            .find(|(_, x)| x == rule)
            .map(|(l, _)| *l)
            .ok_or_else(|| format!("declares: rule {} is on no line", d.rule))?;
        let mut l = format!("declares {file} rule {line} mapped {} left {}", d.mapped, d.left);
        if d.left_kept > 0 {
            l.push_str(&format!(" left_kept {}", d.left_kept));
        }
        out.insert(l);
    }
    Ok(out)
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
    /// The `made` and `fixed` lines, in their order; `lists empty` is an
    /// empty list; `None` when there is no such line.
    lists: Option<Vec<String>>,
    /// The `relied` and `declares` lines (WP-118), their words joined by
    /// one space.
    rebuilt: BTreeSet<String>,
}

fn expected(dir: &Path, file: &str) -> Result<Expected, String> {
    let text = std::fs::read_to_string(dir.join(file)).unwrap_or_default();
    expected_text(&text, file)
}

fn expected_text(text: &str, file: &str) -> Result<Expected, String> {
    let (lines, exact, own) = common::expect_text(text, |l| {
        l.starts_with("use ")
            || l.starts_with("collisions ")
            || l.starts_with("refused ")
            || l.starts_with("made ")
            || l.starts_with("fixed ")
            || l == "lists empty"
            || l.starts_with("relied ")
            || l.starts_with("declares ")
    })?;
    let mut rebuilt = BTreeSet::new();
    let mut uses = Vec::new();
    let (mut collisions, mut refused) = (0, None);
    let mut lists: Option<Vec<String>> = None;
    for (n, l) in own {
        if l == "lists empty" {
            lists.get_or_insert_with(Vec::new);
            continue;
        }
        if l.starts_with("relied ") || l.starts_with("declares ") {
            let w: Vec<&str> = l.split_whitespace().collect();
            let ok = match w.as_slice() {
                ["relied", _, ..] => true,
                ["declares", _, "rule", r, "mapped", m, "left", x, rest @ ..] => {
                    [r, m, x].iter().all(|v| v.parse::<u64>().is_ok())
                        && match rest {
                            [] => true,
                            ["left_kept", k] => k.parse::<u64>().is_ok_and(|k| k > 0),
                            _ => false,
                        }
                }
                _ => false,
            };
            if !ok {
                return Err(format!(
                    "{file}:{n}: `{l}` is not `relied <file>…` or `declares <file> rule <n> \
                     mapped <n> left <n> [left_kept <n>]`"
                ));
            }
            if !rebuilt.insert(w.join(" ")) {
                return Err(format!("{file}:{n}: `{l}` is given twice"));
            }
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
    Ok(Expected { lines, exact, uses, collisions, refused, lists, rebuilt })
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
    /// The walk's `relied` and `declares` lines (WP-118).
    rebuilt: BTreeSet<String>,
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
        .with_files(Some(Ownership {
            owned: vec![ROOT.as_bytes().to_vec()],
            readonly: vec![],
            rebuilt: Vec::new(),
        }));
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
                rebuilt: RebuiltOutput::default(),
                ..first.clone()
            };
            if !off.rule_use.is_empty()
                || !off.kept.is_empty()
                || !off.made.is_empty()
                || !off.fixed.is_empty()
                || off.rebuilt != RebuiltOutput::default()
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
            let rebuilt = rebuilt_lines(&t, own, &first.rebuilt)?;
            Ok(Post { table: t, digested: common::digested(first), uses: got, unruled, rebuilt })
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
    let (readonly, rebuilt) = (readonly_of(dir)?, rebuilt_of(dir)?);
    for f in ["expect-undeclared.txt", "expect-as-owned.txt"] {
        if rebuilt.is_none() && dir.join(f).is_file() {
            return Err(format!("{f} in a case without rebuilt.txt"));
        }
    }
    if (readonly.is_none() || rebuilt.is_some()) && dir.join("rebuilt-differs.txt").is_file() {
        return Err(
            "rebuilt-differs.txt in a case without readonly.txt, or with rebuilt.txt".into()
        );
    }
    let runs: Vec<(Own, &str)> = match (&readonly, &rebuilt) {
        (None, None) => vec![(Own::All, "expect.txt")],
        (Some(l), None) => {
            if !dir.join("expect-owned.txt").is_file() {
                return Err("a case with readonly.txt has an expect-owned.txt".into());
            }
            vec![(Own::Listed(l.clone()), "expect.txt"), (Own::All, "expect-owned.txt")]
        }
        (ro, Some(b)) => {
            let ro = ro.clone().unwrap_or_default();
            if let Some(n) = b.iter().find(|n| ro.contains(n)) {
                return Err(format!("`{n}` is in readonly.txt and in rebuilt.txt"));
            }
            for f in ["expect-undeclared.txt", "expect-owned.txt"] {
                if !dir.join(f).is_file() {
                    return Err(format!("a case with rebuilt.txt has an {f}"));
                }
            }
            let mut both = ro.clone();
            both.extend(b.iter().cloned());
            both.sort();
            vec![
                (Own::Rebuilt { readonly: ro, rebuilt: b.clone() }, "expect.txt"),
                (Own::Listed(both), "expect-undeclared.txt"),
                (Own::All, "expect-owned.txt"),
            ]
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
    if let Err(p) = rebuilt_properties(dir, &pre, &a, readonly, rebuilt) {
        problems.push(p);
    }
    if problems.is_empty() { Ok(()) } else { Err(problems.join("\n")) }
}

/// P1 and P2 of the head of this file (WP-118 R3.3): moving the rebuilt
/// files to owned, or the read-only files to rebuilt, changes no digest
/// and no rule use, unless the case says why it does.
fn rebuilt_properties(
    dir: &Path,
    pre: &[Record],
    a: &Digested,
    readonly: Option<Vec<String>>,
    rebuilt: Option<Vec<String>>,
) -> Result<(), String> {
    // The digests and the rule use of `own`'s run.
    let run = |own: &Own| -> Result<Output, String> {
        post_side(dir, pre, &[], own, false).map(|p| p.digested.output)
    };
    let same = |x: &Output, y: &Output| {
        x.digests == y.digests && x.causes == y.causes && x.rule_use == y.rule_use
    };
    match (readonly, rebuilt) {
        (ro, Some(b)) => {
            let ro = ro.unwrap_or_default();
            let declared = Own::Rebuilt { readonly: ro.clone(), rebuilt: b };
            let as_owned = Own::Rebuilt { readonly: ro, rebuilt: Vec::new() };
            if dir.join("expect-as-owned.txt").is_file() {
                let e = expected(dir, "expect-as-owned.txt")?;
                return one(dir, pre, a, &e, &as_owned, false)
                    .map_err(|p| format!("[expect-as-owned.txt, {as_owned:?}] {p}"));
            }
            if !same(&run(&declared)?, &run(&as_owned)?) {
                return Err(format!(
                    "P1: the rebuilt files read as owned change the digests or the rule use \
                     ({as_owned:?}); a case where that is right has expect-as-owned.txt"
                ));
            }
            Ok(())
        }
        (Some(ro), None) => {
            let as_rebuilt = Own::Rebuilt { readonly: Vec::new(), rebuilt: ro };
            if let Ok(text) = std::fs::read_to_string(dir.join("rebuilt-differs.txt")) {
                let (reason, rest) = text.split_once('\n').unwrap_or((&text, ""));
                if !reason.contains("C language linkage") {
                    return Err(format!(
                        "rebuilt-differs.txt: the first line names no C language linkage: \
                         `{reason}`"
                    ));
                }
                let e = expected_text(rest, "rebuilt-differs.txt")?;
                return one(dir, pre, a, &e, &as_rebuilt, false)
                    .map_err(|p| format!("[rebuilt-differs.txt, {as_rebuilt:?}] {p}"));
            }
            if !same(&run(&as_rebuilt)?, &run(&Own::All)?) {
                return Err(format!(
                    "P2: the read-only files read as rebuilt do not give the digests or the \
                     rule use of every file owned ({as_rebuilt:?}); a case where that is right \
                     has rebuilt-differs.txt"
                ));
            }
            Ok(())
        }
        (None, None) => Ok(()),
    }
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
    // WP-118: the `relied` and `declares` lines, in every run (no line
    // means none).
    if post.rebuilt != e.rebuilt {
        let missing: Vec<&String> = e.rebuilt.difference(&post.rebuilt).collect();
        let extra: Vec<&String> = post.rebuilt.difference(&e.rebuilt).collect();
        problems.push(format!(
            "rebuilt lines: expected but not given: {missing:?}; given but not expected: {extra:?}"
        ));
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
    let o = &post.digested.output;
    if !o.made.windows(2).all(|w| w[0] < w[1]) || !o.fixed.windows(2).all(|w| w[0] < w[1]) {
        problems.push("the lists are not strictly ascending".into());
    }
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
/// <name>`, `made <name> no-post`, `fixed T|M <name>`), in their order.
fn list_lines(o: &Output) -> Vec<String> {
    let mut out: Vec<String> = o
        .made
        .iter()
        .map(|m| {
            if m.post.is_some() {
                format!("made {}", m.name)
            } else {
                format!("made {} no-post", m.name)
            }
        })
        .collect();
    out.extend(o.fixed.iter().map(|f| {
        let t = match f.as_ {
            slip_digest::FixedAs::T => "T",
            slip_digest::FixedAs::M => "M",
        };
        format!("fixed {t} {}", f.name)
    }));
    out
}

/// Every case of `tests/rules/`.
#[test]
fn rules() {
    common::run_all("rules", case);
}

/// The post-image's output of case `dir` under its own ownership (the
/// listed files read-only if it has `readonly.txt`, else every file
/// owned): the observer the case stands for.
fn observer(dir: &Path) -> Output {
    let pre = common::with_side(dir, Side::A, |input| {
        records(input, &RuleTable::empty(), ReadOnly::EVERY_FILE)
    })
    .unwrap()
    .unwrap();
    let own = match readonly_of(dir).unwrap() {
        Some(l) => Own::Listed(l),
        None => Own::All,
    };
    post_side(dir, &pre, &[], &own, false).unwrap().digested.output
}

fn observers_dir(name: &str) -> std::path::PathBuf {
    common::manifest().join("tests/rules/observers").join(name)
}

/// WP-118 A7 (R3.3): the same entity has the same key in an ordinary
/// observer and a unity observer (its main file rebuilt): `Mine`'s key id
/// under the rules is one over the two. With the glue undeclared
/// (read-only) the unity observer keeps `Mine` unmapped: another id. (The
/// precompiled-header observer, `-include` of a rebuilt file, needs a real
/// parse: the snippet front end refuses `-include`; `slip_cli`'s
/// `tests/rebuilt.rs` has it.)
#[test]
fn wp118_a7_one_key() {
    let key = |case: &str, undeclared: bool| -> slip_model::Digest {
        let dir = observers_dir(case);
        let pre = common::with_side(&dir, Side::A, |input| {
            records(input, &RuleTable::empty(), ReadOnly::EVERY_FILE)
        })
        .unwrap()
        .unwrap();
        let rebuilt = rebuilt_of(&dir).unwrap().unwrap_or_default();
        let own = if undeclared {
            Own::Listed(rebuilt)
        } else {
            Own::Rebuilt { readonly: Vec::new(), rebuilt }
        };
        let o = post_side(&dir, &pre, &[], &own, false).unwrap().digested.output;
        let ids: BTreeSet<_> =
            o.names.iter().filter(|(_, n)| n == "acme::Mine").map(|(k, _)| *k).collect();
        assert_eq!(ids.len(), 1, "{case}: {ids:?}");
        ids.into_iter().next().unwrap()
    };
    let cases = ["wp118-a7-ordinary", "wp118-a7-unity"];
    let declared: BTreeSet<_> = cases.iter().map(|c| key(c, false)).collect();
    assert_eq!(declared.len(), 1, "one key id over the observers");
    let ordinary = key(cases[0], true);
    assert_eq!(ordinary, key(cases[0], false));
    for c in &cases[1..] {
        assert_ne!(key(c, true), ordinary, "{c}: undeclared glue keeps Mine unmapped");
    }
}

/// WP-146 stage 1: every entry of the lists of some cases, in full and in
/// the lists' order: its rule (made), its test (fixed), its name and its
/// place. In `places` the fixed entry `old::f()` is at its first
/// declaration in a read-only file (d.h, before e.h), not at its first
/// declaration (in the owned main file); in `f1-rename-x` the two made
/// entries have two rules.
#[test]
fn wp146_list_entries() {
    let place = |f: &[u8], l: u64, c: u64| format!("{}:{l}:{c}", String::from_utf8_lossy(f));
    let mut got = Vec::new();
    for case in ENTRIES_CASES {
        let o = observer(&observers_dir(case));
        for m in &o.made {
            got.push(format!(
                "{case}: made {} rule {} {}",
                m.name,
                m.rule,
                place(&m.file, m.line, m.column)
            ));
        }
        for f in &o.fixed {
            got.push(format!(
                "{case}: fixed {} {:?} {}",
                f.name,
                f.as_,
                place(&f.file, f.line, f.column)
            ));
        }
    }
    assert_eq!(got, ENTRIES, "\n{}", got.join("\n"));
}

const ENTRIES_CASES: &[&str] = &["places", "f1-rename-x"];
const ENTRIES: &[&str] = &[
    "places: made fresh::C rule 0 main.cpp:5:26",
    "places: made fresh::inner2::Z rule 0 main.cpp:5:69",
    "places: fixed old::f() T d.h:1:22",
    "places: fixed old::inner::Y T d.h:2:42",
    "f1-rename-x: made fresh::D2 rule 1 main.cpp:1:26",
    "f1-rename-x: made fresh::k rule 0 main.cpp:1:45",
];

/// WP-146 ruling 2, F1: a made entity whose post-image key cannot be
/// formed. `tests/own_lists/deep-no-post` (outside `tests/rules/`: the
/// runner's key agreement refuses a key the walk formed only through its
/// memo) nests `fresh::X` 205 namespaces deep. The walk keys each level
/// as it reaches it, so `X`'s own key is formed; its key in the dependency
/// view climbs every level at once, past the depth limit (200): the entry
/// is in `made` with `post` `None`, not dropped.
#[test]
fn wp146_made_without_post() {
    let dir = common::manifest().join("tests/own_lists/deep-no-post");
    let pre = common::with_side(&dir, Side::A, |input| {
        records(input, &RuleTable::empty(), ReadOnly::EVERY_FILE)
    })
    .unwrap()
    .unwrap();
    let lines = lines_of(&dir).unwrap();
    let made = common::with_side(&dir, Side::B, |input| {
        let plain = records(input, &RuleTable::empty(), ReadOnly::EVERY_FILE).unwrap();
        let t = table(&lines, &pre, &plain).unwrap();
        digest_with(input, &t.table, ReadOnly::NO_FILE, true, &[]).made
    })
    .unwrap();
    let got: Vec<(String, bool)> = made
        .iter()
        .map(|m| (m.name.rsplit("::").next().unwrap_or("").to_owned(), m.post.is_some()))
        .collect();
    assert_eq!(got, [("X".to_owned(), false)], "{made:?}");
}

/// One row of [`wp146_joins`]: the keys stage 3's join reads, by shape.
enum Join {
    /// T: the made entry (case, name)'s `key` is the fixed T entry (case,
    /// name)'s key.
    T(&'static str, &'static str, &'static str, &'static str),
    /// M: the made entry's `post` is the fixed M entry's key.
    M(&'static str, &'static str, &'static str, &'static str),
    /// No join: no key of either case's made entries (`key` or `post`) is
    /// the key of a fixed entry of the other.
    No(&'static str, &'static str),
}

/// WP-146 ruling 2, F2: per shape, the keys the join of stage 3 reads
/// (`made.key == fixed.key` for T, `made.post == fixed.key` for M), over
/// the observers cases. A helper standing for the join, not stage 3.
#[test]
fn wp146_joins() {
    use std::collections::HashMap;
    let mut outs: HashMap<&str, Output> = HashMap::new();
    let mut get = |c: &'static str| -> Output {
        outs.entry(c).or_insert_with(|| observer(&observers_dir(c))).clone()
    };
    let mut problems = Vec::new();
    for row in JOINS {
        match *row {
            Join::T(x, made, y, fixed) | Join::M(x, made, y, fixed) => {
                let m_out = get(x);
                let f_out = get(y);
                let (want_as, label) = match row {
                    Join::T(..) => (slip_digest::FixedAs::T, "T"),
                    _ => (slip_digest::FixedAs::M, "M"),
                };
                let Some(m) = m_out.made.iter().find(|m| m.name == made) else {
                    problems.push(format!("{x}: no made {made}"));
                    continue;
                };
                let Some(f) = f_out.fixed.iter().find(|f| f.name == fixed && f.as_ == want_as)
                else {
                    problems.push(format!("{y}: no fixed {label} {fixed}"));
                    continue;
                };
                let joined = match want_as {
                    slip_digest::FixedAs::T => m.key == f.key,
                    slip_digest::FixedAs::M => m.post == Some(f.key),
                };
                if !joined {
                    problems
                        .push(format!("{x} made {made} and {y} fixed {label} {fixed}: no join"));
                }
            }
            Join::No(a, b) => {
                let (oa, ob) = (get(a), get(b));
                for (p, q, pn, qn) in [(&oa, &ob, a, b), (&ob, &oa, b, a)] {
                    for m in &p.made {
                        for f in &q.fixed {
                            if f.key == m.key || Some(f.key) == m.post {
                                problems.push(format!(
                                    "{pn} made {} and {qn} fixed {:?} {}: a join",
                                    m.name, f.as_, f.name
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
}

const JOINS: &[Join] = &[
    Join::T("s1-u1", "fresh::hook(int)", "s1-u0", "old::hook(int)"),
    Join::T("s2-u1", "fresh::W", "s2-u0", "old::W"),
    Join::T("s3-u1", "fresh::level", "s3-u0", "old::level"),
    Join::T("s4-u1", "fresh::conv", "s4-u0", "old::conv"),
    Join::No("s5-u1", "s5-u0"),
    Join::T("s6-u1", "fresh::twice(int)", "s6-u0", "old::twice(int)"),
    Join::T("s6-u0-both", "fresh::twice(int)", "s6-u0-both", "old::twice(int)"),
    Join::T("s7-u1", "fresh::box", "s7-u0", "old::box"),
    Join::T("s7-u1", "fresh::box", "s7-h-u1", "old::box"),
    Join::No("s7-h-u1", "s7-h-u1"),
    Join::T("s8-u1", "fresh::peek(const old::D &)", "s8-u0", "old::peek(const old::D &)"),
    Join::No("s8-u1", "s8-u1"),
    Join::M("s9-u1", "fresh::g(int)", "s9-u0", "fresh::g(int)"),
    Join::M("s10-u1", "fresh::g(int)", "s10-u0", "fresh::g(int)"),
    Join::M("s10-own-key-u1", "fresh::g(int)", "s10-own-key-u0", "fresh::g(int)"),
    Join::No("s11-u0", "s11-u1"),
    Join::No("s11-u0", "s11-u0"),
    Join::No("s12-u2", "s1-u0"),
    Join::No("s12-u2", "s1-u1"),
    Join::No("s13-u0", "s13-u1"),
    Join::No("s13-u0", "s13-u0"),
    Join::No("s14-u1", "s14-u0"),
    Join::No("s15-u0", "s15-u0"),
    Join::No("s16-u0", "s16-u0"),
    Join::No("s17-direct-first", "s17-direct-first"),
    Join::M("s17-direct-first", "fresh::UserData", "s17-through-first", "fresh::UserData"),
    Join::M("s18-own-macro", "fresh::Thing", "s18-dep-macro", "fresh::Thing"),
    Join::No("s19-u0", "s19-u1"),
    Join::No("s20-u0", "s20-u1"),
    Join::T("s21-u1", "fresh::id_t", "s21-u0", "old::id_t"),
    Join::T("s22-u1", "fresh::peek(old::D<int> &)", "s22-u0", "old::peek(old::D<int> &)"),
    Join::No("s22-u1", "s22-u1"),
    Join::T("s23-u1", "fresh::hook(int)", "s23-u0", "old::hook(int)"),
    Join::No("places", "places"),
    // Ruling 2, F1: a correct edit, renamed while moved onto a name a
    // dependency already has in the target (or moved to the root): two
    // entities before and after, no join.
    Join::No("f1-rename-x", "f1-rename-y"),
    Join::No("f1-root-x", "f1-root-y"),
];

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
