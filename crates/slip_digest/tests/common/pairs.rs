//! The judgments of the pair runners (`spec/digest.md` §14,
//! `spec/digest-kinds.md` §1.5, `spec/landed/harness/WP-047.md`), apart
//! from the runners so that `tests/runner.rs` can put them to its own
//! fixture directories: `differ`, `demand` and `equal` over one
//! directory, and the pure checks under them over two digested sides.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use slip_model::Digest;
use slip_model::digests::GroupClass;

use slip_digest::Cause;

use super::{
    Digested, Expect, Outcome, Side, derived_differences, digest_side, exact_derived, expect,
};

/// The suffix of a report line a `derived-changed` line covers only
/// loosely (an appearance or a disappearance).
const LOOSE: &str = " (written derived-changed)";

/// The units of `d` under `key` that are not derived.
fn declared(d: &Digested, key: &Digest) -> Vec<(GroupClass, Digest, u64)> {
    d.units
        .get(key)
        .into_iter()
        .flatten()
        .filter(|u| u.class != GroupClass::Derived)
        .map(|u| (u.class, u.digest, u.flags))
        .collect()
}

/// Whether a `derived-*` line holds without `exact derived`, given the
/// derived differences `dd`: `derived-added` and `derived-removed` as
/// written; `derived-changed` when a key of its name differs in any way,
/// presence included (its meaning before WP-047, kept for open
/// branches).
fn holds_loosely(line: &Expect, dd: &BTreeSet<Expect>) -> bool {
    match line {
        Expect::DerivedAdded(_) | Expect::DerivedRemoved(_) => dd.contains(line),
        Expect::DerivedChanged(n) => {
            dd.contains(&Expect::DerivedAdded(n.clone()))
                || dd.contains(&Expect::DerivedRemoved(n.clone()))
                || dd.contains(&Expect::DerivedChanged(n.clone()))
        }
        _ => false,
    }
}

/// The side errors every pair runner reports: units indeterminate for a
/// cause other than `Pending`.
fn hard(a: &Digested, b: &Digested) -> Vec<String> {
    let mut problems = Vec::new();
    for (side, d) in [("A", a), ("B", b)] {
        for u in super::hard_indeterminate(d) {
            problems.push(format!("side {side}: indeterminate {u}"));
        }
    }
    problems
}

/// The `indeterminate` lines of a `differ` test (`spec/digest.md` §14.1
/// rule 3, WP-124): every unit of either side flagged for a cause other
/// than `Pending` must be named by one, as `<name>` or as `<name>
/// (<cause>)`, and every line must name such a unit on both sides. The
/// keys of the units the lines name are left out of the comparison (an
/// indeterminate unit equals nothing). Returns the problems and those keys.
fn held_indeterminate(
    expect: &BTreeSet<Expect>,
    a: &Digested,
    b: &Digested,
) -> (Vec<String>, BTreeSet<Digest>) {
    let lines: BTreeSet<&str> = expect
        .iter()
        .filter_map(|e| if let Expect::Indeterminate(n) = e { Some(n.as_str()) } else { None })
        .collect();
    let mut problems = Vec::new();
    let mut held = BTreeSet::new();
    for (side, d) in [("A", a), ("B", b)] {
        let mut named = BTreeSet::new();
        for (k, v) in &d.units {
            for u in v {
                if u.flags & 1 == 0 || matches!(u.cause, Some(Cause::Pending(_)) | None) {
                    continue;
                }
                let with =
                    format!("{} ({})", u.name, u.cause.map(|c| c.to_string()).unwrap_or_default());
                match lines.iter().find(|l| **l == u.name || **l == with) {
                    Some(l) => {
                        named.insert(*l);
                        held.insert(*k);
                    }
                    None => problems.push(format!("side {side}: indeterminate {with}")),
                }
            }
        }
        for l in lines.difference(&named) {
            problems.push(format!("side {side}: `indeterminate {l}` names no unit flagged"));
        }
    }
    (problems, held)
}

/// A must-differ test (`tests/differ.rs`): over declared and `tu` units,
/// the keys whose units differ are exactly the `changed`, `added` and
/// `removed` lines; every `derived-added` / `derived-removed` line holds;
/// a `derived-changed` line holds when a key of its name differs in any
/// way; with `exact derived` the derived differences are exactly the
/// `derived-*` lines, `derived-changed` meaning both sides have it. The
/// `indeterminate` lines name the units that may be flagged, each on both
/// sides ([`held_indeterminate`]); any other flagged unit fails the test.
///
/// `Ok` holds the derived differences no line names, each as the line
/// that would name it (for `SLIP_DIGEST_DERIVED=report`); one a
/// `derived-changed` line covers only loosely ends in
/// ` (written derived-changed)`.
pub fn differ(dir: &Path) -> Result<Vec<String>, String> {
    let expect = expect(dir)?;
    let exact = exact_derived(dir)?;
    if expect.iter().all(|e| matches!(e, Expect::Indeterminate(_))) {
        return Err(if exact {
            "a differ test needs at least one line in expect.txt besides `exact derived` and `indeterminate`".into()
        } else {
            "a differ test needs at least one line in expect.txt besides `indeterminate`".into()
        });
    }
    let a = digest_side(dir, Side::A)?;
    let b = digest_side(dir, Side::B)?;
    judge_differ(&expect, exact, &a, &b)
}

/// [`differ`] over two digested sides.
pub fn judge_differ(
    all: &BTreeSet<Expect>,
    exact: bool,
    a: &Digested,
    b: &Digested,
) -> Result<Vec<String>, String> {
    let expect = &without_names(all);
    let (mut problems, held) = held_indeterminate(expect, a, b);
    problems.extend(judge_names(all, a, b, false));
    let keys: BTreeSet<_> = a.units.keys().chain(b.units.keys()).copied().collect();
    let mut got = BTreeSet::new();
    for k in keys.iter().filter(|k| !held.contains(*k)) {
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
    let dd = derived_differences(a, b);
    if exact {
        got.extend(dd.iter().cloned());
    } else {
        got.extend(expect.iter().filter(|e| holds_loosely(e, &dd)).cloned());
    }
    let want: BTreeSet<Expect> =
        expect.iter().filter(|e| !matches!(e, Expect::Indeterminate(_))).cloned().collect();
    if got != want {
        let missing: Vec<_> = want.difference(&got).collect();
        let extra: Vec<_> = got.difference(&want).collect();
        problems
            .push(format!("expected but not seen: {missing:?}\nseen but not expected: {extra:?}"));
    }
    if !problems.is_empty() {
        return Err(problems.join("\n"));
    }
    let loose: BTreeSet<&str> = expect
        .iter()
        .filter_map(|e| if let Expect::DerivedChanged(n) = e { Some(n.as_str()) } else { None })
        .collect();
    Ok(dd
        .iter()
        .filter(|e| !expect.contains(e))
        .map(|e| if loose.contains(e.name()) { format!("{}{LOOSE}", e.line()) } else { e.line() })
        .collect())
}

/// A demand-independence test (`tests/demand.rs`, `spec/digest.md` §14):
/// every key in both sides has the same units; the declared keys only B
/// has are exactly the `added` lines and A has none B lacks; a key both
/// sides declare is in the declared group of one file on both, and a
/// declared group that holds no key of one side only has the same sum on
/// both (§11). Every `derived-added` line holds, and with `exact derived`
/// the derived differences are exactly those lines.
pub fn demand(dir: &Path) -> Result<(), String> {
    let expect = expect(dir)?;
    let exact = exact_derived(dir)?;
    let a = digest_side(dir, Side::A)?;
    let b = digest_side(dir, Side::B)?;
    judge_demand(&expect, exact, &a, &b)
}

/// [`demand`] over two digested sides.
pub fn judge_demand(
    expect: &BTreeSet<Expect>,
    exact: bool,
    a: &Digested,
    b: &Digested,
) -> Result<(), String> {
    let mut problems = hard(a, b);
    for (k, ua) in &a.units {
        if let Some(ub) = b.units.get(k)
            && ua != ub
        {
            problems.push(format!("differs: {}", a.name(k)));
        }
    }
    if let Some(e) =
        expect.iter().find(|e| !matches!(e, Expect::Added(_) | Expect::DerivedAdded(_)))
    {
        return Err(format!("{e:?}: a demand test has `added` and `derived-added` lines only"));
    }
    let added: BTreeSet<Expect> =
        expect.iter().filter(|e| matches!(e, Expect::Added(_))).cloned().collect();
    let declared = |d: &Digested, other: &Digested| -> BTreeSet<String> {
        d.units
            .iter()
            .filter(|(k, v)| {
                !other.units.contains_key(*k) && v.iter().any(|u| u.class == GroupClass::Declared)
            })
            .map(|(k, _)| d.name(k))
            .collect()
    };
    let got: BTreeSet<Expect> = declared(b, a).into_iter().map(Expect::Added).collect();
    if got != added {
        problems.push(format!("declared keys only B has: expected {added:?}, saw {got:?}"));
    }
    let only_a = declared(a, b);
    if !only_a.is_empty() {
        problems.push(format!("declared keys only A has: {only_a:?}"));
    }
    let want_derived: BTreeSet<Expect> =
        expect.iter().filter(|e| matches!(e, Expect::DerivedAdded(_))).cloned().collect();
    let dd = derived_differences(a, b);
    let got_derived: BTreeSet<Expect> =
        if exact { dd } else { want_derived.iter().filter(|e| dd.contains(e)).cloned().collect() };
    if got_derived != want_derived {
        let missing: Vec<_> = want_derived.difference(&got_derived).collect();
        let extra: Vec<_> = got_derived.difference(&want_derived).collect();
        problems.push(format!(
            "derived: expected but not seen: {missing:?}\nseen but not expected: {extra:?}"
        ));
    }
    let (fa, fb) = (declared_files(a), declared_files(b));
    for (k, f) in &fa {
        if let Some(g) = fb.get(k)
            && f != g
        {
            problems.push(format!("declared in another file: {}", a.name(k)));
        }
    }
    let one_side: BTreeSet<&Vec<u8>> = fa
        .iter()
        .filter(|(k, _)| !fb.contains_key(*k))
        .chain(fb.iter().filter(|(k, _)| !fa.contains_key(*k)))
        .map(|(_, f)| f)
        .collect();
    let sums = |d: &Digested| -> BTreeMap<Vec<u8>, (u64, u64, Digest)> {
        d.output
            .digests
            .groups
            .iter()
            .filter(|g| g.class == GroupClass::Declared && !one_side.contains(&g.file))
            .map(|g| (g.file.clone(), (g.count, g.indeterminate, g.digest)))
            .collect()
    };
    if sums(a) != sums(b) {
        problems.push("a declared group's sum differs".into());
    }
    if problems.is_empty() { Ok(()) } else { Err(problems.join("\n")) }
}

/// The file of the declared group that holds each declared key.
fn declared_files(d: &Digested) -> BTreeMap<Digest, Vec<u8>> {
    let mut out = BTreeMap::new();
    for g in d.output.digests.groups.iter().filter(|g| g.class == GroupClass::Declared) {
        for u in g.units.iter().flatten() {
            out.insert(u.key, g.file.clone());
        }
    }
    out
}

/// The lines of `expect.txt` for a runner that reads only the lines
/// `reads` accepts and not the directive (`equal`, `indeterminate`): any
/// other line, and `exact derived`, is an error of the test naming it.
/// In an `equal` test every unit both sides have is equal already and a
/// derived unit only one side has is a failure, not something to name
/// (`spec/digest.md` §14).
pub fn lines_read(
    dir: &Path,
    what: &str,
    reads: impl Fn(&Expect) -> bool,
) -> Result<BTreeSet<Expect>, String> {
    if exact_derived(dir)? {
        return Err(format!("`exact derived`: not a line of {what}"));
    }
    let lines = expect(dir)?;
    if let Some(e) = lines.iter().find(|e| !reads(e)) {
        return Err(format!("`{}`: not a line of {what}", e.line()));
    }
    Ok(lines)
}

/// A must-equal test (`tests/equal.rs`, `spec/digest.md` §14): the two
/// sides' `Digests` are equal with `Detail::All`. A unit, declared or
/// derived, that only one side has makes them differ. `expect.txt` has
/// no line (comments and `# pending` only).
pub fn equal(dir: &Path) -> Result<(), String> {
    let lines = lines_read(dir, "an equal test", Expect::is_names)?;
    let a = digest_side(dir, Side::A)?;
    let b = digest_side(dir, Side::B)?;
    let mut problems: Vec<String> = judge_equal(&a, &b).err().into_iter().collect();
    problems.extend(judge_names(&lines, &a, &b, true));
    if problems.is_empty() { Ok(()) } else { Err(problems.join("\n")) }
}

/// The lines of `all` but the `names-changed` lines.
pub fn without_names(all: &BTreeSet<Expect>) -> BTreeSet<Expect> {
    all.iter().filter(|e| !e.is_names()).cloned().collect()
}

/// The names digests of `d` by key (`Output::name_strings`).
fn names_of(d: &Digested) -> BTreeMap<Digest, Digest> {
    d.output.name_strings.iter().map(|n| (n.key, n.names)).collect()
}

/// The names judgment of every pair runner (`spec/digest.md` §14.1,
/// WP-112): the keys both sides give a names digest and whose names
/// digests differ are exactly the `names-changed` lines of `expect`. In
/// an equal judgment (`equal`), a key with a names digest on one side only
/// is a failure too: the units are equal, so the strings they hold are
/// the same strings. Returns the problems.
pub fn judge_names(
    expect: &BTreeSet<Expect>,
    a: &Digested,
    b: &Digested,
    equal: bool,
) -> Vec<String> {
    let (na, nb) = (names_of(a), names_of(b));
    let mut got = BTreeSet::new();
    let mut problems = Vec::new();
    for (k, x) in &na {
        match nb.get(k) {
            Some(y) if x != y => {
                got.insert(Expect::NamesChanged(a.name(k)));
            }
            None if equal => problems.push(format!("names digest only in A: {}", a.name(k))),
            Some(_) | None => {}
        }
    }
    if equal {
        for k in nb.keys().filter(|k| !na.contains_key(*k)) {
            problems.push(format!("names digest only in B: {}", b.name(k)));
        }
    }
    let want: BTreeSet<Expect> = expect.iter().filter(|e| e.is_names()).cloned().collect();
    if got != want {
        let missing: Vec<String> = want.difference(&got).map(Expect::line).collect();
        let extra: Vec<String> = got.difference(&want).map(Expect::line).collect();
        problems.push(format!(
            "names: expected but not seen: {missing:?}\nnames: seen but not expected: {extra:?}"
        ));
    }
    problems
}

/// An indeterminate test (`tests/indeterminate.rs`, `spec/digest.md` §10,
/// §14): side A only; the units flagged for a cause other than `Pending`
/// are exactly the `indeterminate` lines, and none of those has the cause
/// `Pending`. A unit indeterminate only for `Pending` is ignored while
/// `Area` exists. `expect.txt` has `indeterminate` lines only.
pub fn indeterminate(dir: &Path) -> Result<(), String> {
    let expect =
        lines_read(dir, "an indeterminate test", |e| matches!(e, Expect::Indeterminate(_)))?;
    let want: BTreeSet<String> = expect.iter().map(|e| e.name().to_owned()).collect();
    if want.is_empty() {
        return Err("an indeterminate test needs at least one `indeterminate` line".into());
    }
    let a = digest_side(dir, Side::A)?;
    let mut got = BTreeSet::new();
    let mut pending = BTreeSet::new();
    for v in a.units.values() {
        for u in v {
            if u.flags & 1 == 0 {
                continue;
            }
            match u.cause {
                Some(Cause::Pending(_)) | None => {
                    pending.insert(u.name.clone());
                }
                Some(_) => {
                    got.insert(u.name.clone());
                }
            }
        }
    }
    if got == want {
        return Ok(());
    }
    let missing: Vec<_> = want.difference(&got).collect();
    let extra: Vec<_> = got.difference(&want).collect();
    let only_pending: Vec<_> = want.intersection(&pending).collect();
    Err(format!(
        "expected but not flagged: {missing:?} (of which only pending: {only_pending:?})\nflagged but not expected: {extra:?}"
    ))
}

/// Whether the value of `SLIP_DIGEST_DERIVED` asks for the report of
/// unnamed derived differences (`report`).
pub fn report_mode(var: Option<&std::ffi::OsStr>) -> bool {
    var.is_some_and(|v| v == "report")
}

/// The prefix of a note of the report.
const UNNAMED: &str = "unnamed ";

/// What the `differ` runner does with a directory: [`differ`], and in
/// report mode its unnamed derived differences as notes
/// `unnamed <line>`.
pub fn differ_noted(dir: &Path, report: bool) -> Result<Vec<String>, String> {
    differ(dir).map(|lines| {
        if report {
            lines.into_iter().map(|l| format!("{UNNAMED}{l}")).collect()
        } else {
            Vec::new()
        }
    })
}

/// The summary the `differ` runner prints in report mode after a run of
/// [`differ_noted`]: the count of unnamed derived differences and of
/// their directories, the count by kind, and the twenty directories with
/// the most (ties by directory).
pub fn report_summary(outcome: &Outcome) -> String {
    let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
    let mut by_dir: BTreeMap<&str, usize> = BTreeMap::new();
    for n in &outcome.notes {
        let Some((dir, line)) = n.split_once(&format!(": {UNNAMED}")) else { continue };
        let word = line.split(' ').next().unwrap_or_default();
        let loose = if line.ends_with(LOOSE) { LOOSE } else { "" };
        *kinds.entry(format!("{word}{loose}")).or_default() += 1;
        *by_dir.entry(dir).or_default() += 1;
    }
    let mut most: Vec<_> = by_dir.iter().collect();
    most.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
    let mut lines = vec![format!(
        "SLIP_DIGEST_DERIVED=report: {} unnamed derived differences in {} directories",
        kinds.values().sum::<usize>(),
        by_dir.len()
    )];
    lines.extend(kinds.iter().map(|(k, n)| format!("  {n:>6}  {k}")));
    lines.push("the directories with the most:".to_owned());
    lines.extend(most.iter().take(20).map(|(d, n)| format!("  {n:>6}  {d}")));
    let mut text = lines.join("\n");
    text.push('\n');
    text
}

/// [`equal`] over two digested sides.
pub fn judge_equal(a: &Digested, b: &Digested) -> Result<(), String> {
    let mut problems = hard(a, b);
    if a.output.digests != b.output.digests {
        for (k, ua) in &a.units {
            match b.units.get(k) {
                Some(ub) if ua == ub => {}
                Some(_) => problems.push(format!("differs: {}", a.name(k))),
                None => problems.push(format!("only in A: {}", a.name(k))),
            }
        }
        for k in b.units.keys() {
            if !a.units.contains_key(k) {
                problems.push(format!("only in B: {}", b.name(k)));
            }
        }
        if problems.is_empty() {
            problems.push("the digests differ (groups or files)".into());
        }
    }
    if problems.is_empty() { Ok(()) } else { Err(problems.join("\n")) }
}

/// The suffix of an operand unit's name (`names.rs`, WP-156).
pub const OPERANDS: &str = "::<operands>";

/// The key of the operand unit of the unit whose key is `parent`
/// (`spec/digest-kinds.md` §11.10): no rule maps it further.
pub fn operand_key(parent: &Digest) -> Digest {
    use slip_model::keys::{Key, KeyKind, Name, Parent};
    Key { kind: KeyKind::Operands, parent: Parent::Unit(*parent), name: Name::Operands, disc: None }
        .id()
}

/// The operand-unit class of pairs (`tests/operands.rs`, WP-156 item 6,
/// `spec/verify.md` §4 step 3, S8): every unit both sides have is equal;
/// every unit one side alone has is the operand unit of a unit both sides
/// have; no unit is indeterminate for a cause other than `Pending`. `Ok`
/// holds the names of the parents of each side's operand units (the
/// operand unit's name without [`OPERANDS`]): side A's, then side B's.
pub fn judge_operands(
    a: &Digested,
    b: &Digested,
) -> Result<(BTreeSet<String>, BTreeSet<String>), String> {
    let mut problems = hard(a, b);
    for (k, ua) in &a.units {
        if let Some(ub) = b.units.get(k)
            && ua != ub
        {
            problems.push(format!("differs: {}", a.name(k)));
        }
    }
    let both: BTreeSet<&Digest> = a.units.keys().filter(|k| b.units.contains_key(*k)).collect();
    let of_both: BTreeSet<Digest> = both.iter().map(|p| operand_key(p)).collect();
    for (side, d, other) in [("A", a, b), ("B", b, a)] {
        for k in d.units.keys() {
            if !other.units.contains_key(k) && !of_both.contains(k) {
                problems.push(format!(
                    "only in {side}: {} (not the operand unit of a unit both sides have)",
                    d.name(k)
                ));
            }
        }
    }
    let parents = |d: &Digested| -> BTreeSet<String> {
        d.units
            .values()
            .flatten()
            .filter_map(|u| u.name.strip_suffix(OPERANDS).map(str::to_owned))
            .collect()
    };
    if problems.is_empty() { Ok((parents(a), parents(b))) } else { Err(problems.join("\n")) }
}
