//! `cargo xtask invariants a|b <corpus>` (`spec/harness.md` §9): the two
//! corpus invariants of M1.
//!
//! ```text
//! cargo xtask invariants a <corpus> [--slip <path>] [--only <substring>]… [--limit <n>] [-j <n>]
//! cargo xtask invariants b <corpus> [--slip <path>] [--only <substring>]… [--limit <n>] [-j <n>]
//! ```
//!
//! - (a), §9.1: two map runs of the corpus into two fresh stores give
//!   equal shards, observer by observer ([`compare_runs`]).
//! - (b), §9.2: the identity image verifies with nothing declared: every
//!   selected observer whose map shard is `ok` is `verified` in the report
//!   of `slip verify` ([`check_identity`]).
//!
//! Both pure parts take what [`perf::load`] reads from a store, and (b)
//! the report as [`json::parse`] reads it, so the tests need neither
//! `slip` nor a corpus. The command runs `slip` through [`slip_run`].
//!
//! **This run's shard.** An observer's result in a run is the `shard` of
//! its last `Job` entry in the run record (`spec/model.md` §9). The shard
//! the store holds is read as that result only if its content id is that
//! digest, the rule `slip verify` applies to its own run
//! (`spec/verify.md` §7); anything else is reported, never compared.

use crate::corpus::{self, Corpus, Layout, rows};
use crate::perf::{self, Loaded, render::text, status_name};
use crate::slip_run::{self, Ran};
use slip_exec::store::{Access, Store};
use slip_model::declared::Declared;
use slip_model::json::{self, Value};
use slip_model::{Digest, End, Phase, Plan, RunRecord, Status};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fmt::Write as _;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "\
usage: cargo xtask invariants a <corpus> [--slip <path>] [--only <substring>]… [--limit <n>] [-j <n>]
       cargo xtask invariants b <corpus> [--slip <path>] [--only <substring>]… [--limit <n>] [-j <n>]";

/// At most this many observers are listed under `different` (§9.1) and
/// under the counts of (b) (§9.2).
pub const LISTED: usize = 50;

/// The report format (b) reads (`spec/verify.md` §7.1).
pub const REPORT: &str = "slip.verify.v1";

// ---------------------------------------------------------------------------
// §9.1 Invariant (a)

/// An observer's result in one run, as (a) compares it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// The record has no entry for the observer.
    NoJob,
    /// Its last entry names no shard; the entry's end.
    NoShard(End),
    /// Its last entry names this shard, and the store has no shard of the
    /// observer for the run's phase and round.
    Absent(Digest),
    /// Its last entry names this shard, and the store's shard of the
    /// observer has another content id: it is not this run's.
    Other(Digest),
    /// The store's shard is this run's: its status and content id.
    Shard(Status, Digest),
}

impl Side {
    /// The text of one side of a `different` line.
    pub fn text(&self) -> String {
        match self {
            Side::NoJob => "no job".to_owned(),
            Side::NoShard(end) => format!("no shard {}", perf::end_name(*end)),
            Side::Absent(id) => format!("absent {id}"),
            Side::Other(id) => format!("other {id}"),
            Side::Shard(status, id) => format!("{} {id}", status_name(*status)),
        }
    }

    /// The status of the run's own shard, if there is one.
    pub fn status(&self) -> Option<Status> {
        match self {
            Side::Shard(status, _) => Some(*status),
            _ => None,
        }
    }
}

/// The result of `observer` in the run `run` (see the module's "This run's
/// shard").
pub fn side(run: &Loaded, observer: &Digest) -> Side {
    let Some(last) = run.record.jobs.iter().rev().find(|j| j.observer == *observer) else {
        return Side::NoJob;
    };
    let Some(id) = last.shard else {
        return Side::NoShard(last.end);
    };
    match run.shards.get(observer) {
        Some(Some(shard)) => {
            if slip_model::formats::content_id(&shard.to_bytes()).ok() == Some(id) {
                Side::Shard(shard.outcome.status, id)
            } else {
                Side::Other(id)
            }
        }
        _ => Side::Absent(id),
    }
}

/// An observer that is neither equal nor failed in both runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Different {
    pub observer: Digest,
    /// Its main file, from the plan.
    pub main: Vec<u8>,
    pub first: Side,
    pub second: Side,
}

/// What (a) found over the selected observers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct A {
    /// The selected observers of the plan.
    pub observers: u64,
    pub equal: u64,
    pub failed_both: u64,
    /// In plan order; every one, not only the first [`LISTED`].
    pub different: Vec<Different>,
}

/// §9.1 over the observers of `plan` that are in `selected`, in plan
/// order (a digest of `selected` that is not an observer of the plan is
/// not counted). An observer is equal when both runs have their own shard
/// ([`side`]), neither is `failed`, and the two content ids are equal;
/// failed in both when both have their own shard and both are `failed`;
/// anything else is different.
pub fn compare_runs(plan: &Plan, first: &Loaded, second: &Loaded, selected: &[Digest]) -> A {
    let selected: BTreeSet<&Digest> = selected.iter().collect();
    let mut a = A { observers: 0, equal: 0, failed_both: 0, different: Vec::new() };
    for o in plan.observers.iter().filter(|o| selected.contains(&o.id)) {
        a.observers += 1;
        let (s1, s2) = (side(first, &o.id), side(second, &o.id));
        match (s1, s2) {
            (Side::Shard(Status::Failed, _), Side::Shard(Status::Failed, _)) => a.failed_both += 1,
            (Side::Shard(st1, id1), Side::Shard(st2, id2))
                if st1 != Status::Failed && st2 != Status::Failed && id1 == id2 =>
            {
                a.equal += 1;
            }
            _ => a.different.push(Different {
                observer: o.id,
                main: o.main.path.clone(),
                first: s1,
                second: s2,
            }),
        }
    }
    a
}

/// The output of (a), §9.1. `runs` are the two run record paths as `slip`
/// printed them.
pub fn render_a(corpus: &str, plan: &Digest, a: &A, runs: [&str; 2], selection: bool) -> String {
    let mut o = String::new();
    let _ = writeln!(o, "invariant a {corpus}");
    let _ = writeln!(o, "plan {plan}");
    let _ = writeln!(o, "observers {}", a.observers);
    let _ = writeln!(o, "equal {}", a.equal);
    let _ = writeln!(o, "failed_both {}", a.failed_both);
    let _ = writeln!(o, "different {}", a.different.len());
    for d in a.different.iter().take(LISTED) {
        let _ = writeln!(o, "  {}: {} | {}", text(&d.main), d.first.text(), d.second.text());
    }
    for r in runs {
        let _ = writeln!(o, "run {}", text(r.as_bytes()));
    }
    let verdict = if a.observers == 0 {
        "FAILED (no observer selected)".to_owned()
    } else if a.different.is_empty() {
        "holds".to_owned()
    } else {
        format!("FAILED ({} observer(s) differ)", a.different.len())
    };
    let _ = writeln!(o, "invariant a: {verdict}{}", if selection { " (selection)" } else { "" });
    o
}

/// Whether (a) holds: at least one observer, none different.
pub fn a_holds(a: &A) -> bool {
    a.observers > 0 && a.different.is_empty()
}

// ---------------------------------------------------------------------------
// §9.2 Invariant (b)

/// The verify states of `spec/verify.md` §7, by name.
const STATES: [&str; 7] =
    ["verified", "unchanged", "mismatch", "not_parsed", "unverifiable", "never_parsed", "failed"];

/// A `map_ok` observer that is not verified, as (b) lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotVerified {
    pub observer: Digest,
    pub main: Vec<u8>,
    /// Its state in the report (`mismatch`, `not_parsed`, `unverifiable`
    /// or `failed`).
    pub state: String,
    /// The report's message; for a `mismatch`,
    /// `<findings> finding(s), <indeterminate> indeterminate`.
    pub message: String,
}

/// What (b) found: the numbers of §9.2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct B {
    /// The selected observers (those of the map run).
    pub observers: u64,
    /// Of them, those whose map shard is `ok` (and the map run's own).
    pub map_ok: u64,
    /// The rest, up to `failed`, partition `map_ok`.
    pub verified: u64,
    pub indeterminate_only: u64,
    pub different: u64,
    pub not_parsed: u64,
    pub unverifiable: u64,
    pub failed: u64,
    /// Over every selected observer; not counted against the invariant.
    pub never_parsed: u64,
    /// Every `map_ok` observer that is not verified, in plan order.
    pub not_verified: Vec<NotVerified>,
    /// The report's `run`: the verify run record.
    pub run: Vec<u8>,
}

impl B {
    /// Whether (b) holds: at least one `map_ok` observer, all verified.
    pub fn holds(&self) -> bool {
        self.map_ok > 0 && self.verified == self.map_ok
    }
}

/// A member of a JSON object that must be there.
fn member<'a>(v: &'a Value, path: &str, key: &str) -> Result<&'a Value, String> {
    v.get(key).ok_or_else(|| format!("the report has no `{path}{key}`"))
}

/// A `uv` of `spec/model.md` §10: a number below 2^53, or a decimal string
/// without a leading zero of a value from 2^53 that fits 64 bits.
fn uv(v: &Value, path: &str) -> Result<u64, String> {
    let bad = || format!("the report's `{path}` is not an unsigned number");
    match v {
        Value::Int(_) => v.as_u64().filter(|n| *n < 1 << 53).ok_or_else(bad),
        Value::Str(s) if !s.starts_with('0') && s.bytes().all(|b| b.is_ascii_digit()) => {
            s.parse::<u64>().ok().filter(|n| *n >= 1 << 53).ok_or_else(bad)
        }
        _ => Err(bad()),
    }
}

fn string<'a>(v: &'a Value, path: &str) -> Result<&'a str, String> {
    v.as_str().ok_or_else(|| format!("the report's `{path}` is not a string"))
}

fn digest(v: &Value, path: &str) -> Result<Digest, String> {
    let s = string(v, path)?;
    Digest::from_hex(s).ok_or_else(|| format!("the report's `{path}` is not an id: `{s}`"))
}

/// `bytes`/`path` of `spec/model.md` §10: a string, or `{"hex": …}` of
/// bytes that are not UTF-8.
fn bytes(v: &Value, path: &str) -> Result<Vec<u8>, String> {
    let bad = || format!("the report's `{path}` is not a path");
    match v {
        Value::Str(s) => Ok(s.as_bytes().to_vec()),
        Value::Object(o) => {
            let [(k, Value::Str(hex))] = &o[..] else { return Err(bad()) };
            if k != "hex" || hex.len() % 2 != 0 {
                return Err(bad());
            }
            let digit = |c: u8| match c {
                b'0'..=b'9' => Some(c - b'0'),
                b'a'..=b'f' => Some(c - b'a' + 10),
                _ => None,
            };
            let out = hex
                .as_bytes()
                .chunks(2)
                .map(|p| Some(digit(p[0])? << 4 | digit(p[1])?))
                .collect::<Option<Vec<u8>>>()
                .ok_or_else(bad)?;
            if std::str::from_utf8(&out).is_ok() { Err(bad()) } else { Ok(out) }
        }
        _ => Err(bad()),
    }
}

/// The first line of `s`.
fn first_line(s: &str) -> &str {
    s.lines().next().unwrap_or("")
}

/// §9.2 from the map run `map` and the `report` of `slip verify` over it
/// (`spec/verify.md` §7.1).
///
/// The selected observers are the map run's: the command makes (or reuses)
/// the map run over the same selection as the verify run. The report must
/// be of that plan, round 0, the identity image (`image` empty) and
/// nothing declared (`declared` the id of `Declared::none()`), and must
/// agree with itself and with the map run; anything else is an error
/// naming what disagrees, never a count:
///
/// - `observers.selected` is the number of the map run's observers, and
///   `observers.unchanged` is 0 (an unchanged observer was not compared,
///   and the report does not name it);
/// - every `not_verified` entry is an observer of the map run, once, in
///   plan order, with the plan's main file and a state other than
///   `verified` and `unchanged`; the entries of each state are as many as
///   `observers.<state>` says, `observers.verified` is the rest;
/// - a `mismatch` has at least one finding and no more indeterminate ones
///   than findings, and `observers.indeterminate_only` counts those whose
///   findings are all indeterminate;
/// - the states agree with the map shards (`spec/verify.md` §7): a
///   `mismatch` or `not_parsed` observer, and every observer the report
///   does not list (so `verified`), has an `ok` map shard; a
///   `never_parsed` one has not.
pub fn check_identity(map: &Loaded, report: &Value) -> Result<B, String> {
    let run = check_header(map, report)?;
    let obs = member(report, "", "observers")?;
    let count = |name: &str| -> Result<u64, String> {
        uv(member(obs, "observers.", name)?, &format!("observers.{name}"))
    };
    let selected = count("selected")?;
    let mut claimed: BTreeMap<&str, u64> = BTreeMap::new();
    for s in STATES {
        claimed.insert(s, count(s)?);
    }
    let indeterminate_only = count("indeterminate_only")?;

    // The map run's observers, in plan order.
    let in_run: BTreeSet<Digest> = map.record.jobs.iter().map(|j| j.observer).collect();
    let order: Vec<(usize, &slip_model::Observer)> =
        map.plan.observers.iter().enumerate().filter(|(_, o)| in_run.contains(&o.id)).collect();
    if selected != order.len() as u64 {
        return Err(format!(
            "the report selected {selected} observer(s), the map run has {}: the map run is not \
             of this selection",
            order.len()
        ));
    }
    if claimed["unchanged"] != 0 {
        return Err(format!(
            "the report has {} unchanged observer(s), which (b) cannot check: they were not \
             compared, and the report does not name them",
            claimed["unchanged"]
        ));
    }
    let map_ok = |id: &Digest| side(map, id).status() == Some(Status::Ok);
    let mut b = B {
        observers: selected,
        map_ok: order.iter().filter(|(_, o)| map_ok(&o.id)).count() as u64,
        verified: 0,
        indeterminate_only: 0,
        different: 0,
        not_parsed: 0,
        unverifiable: 0,
        failed: 0,
        never_parsed: 0,
        not_verified: Vec::new(),
        run,
    };
    let listed = read_entries(report, &order, &map_ok, &mut b)?;
    for s in STATES.iter().filter(|s| !matches!(**s, "verified" | "unchanged")) {
        if listed.seen[s] != claimed[s] {
            return Err(format!(
                "the report counts {} `{s}` observer(s) and lists {}",
                claimed[s], listed.seen[s]
            ));
        }
    }
    if claimed["verified"] != selected - listed.observers.len() as u64 {
        return Err(format!(
            "the report counts {} verified observer(s), but {} of its {selected} are not listed \
             as not verified",
            claimed["verified"],
            selected - listed.observers.len() as u64
        ));
    }
    if indeterminate_only != listed.indeterminate_only {
        return Err(format!(
            "the report counts {indeterminate_only} indeterminate_only observer(s), and lists {}",
            listed.indeterminate_only
        ));
    }
    if let Some((_, o)) =
        order.iter().find(|(_, o)| !listed.observers.contains(&o.id) && !map_ok(&o.id))
    {
        return Err(format!(
            "the report has {} verified, but its map shard is not `ok`: the report is not of \
             this map run",
            o.id
        ));
    }
    b.verified = b.map_ok - b.not_verified.len() as u64;
    Ok(b)
}

/// What [`read_entries`] saw of `not_verified`.
struct Listed {
    observers: BTreeSet<Digest>,
    /// `mismatch` entries whose findings are all indeterminate, whatever
    /// their map shard.
    indeterminate_only: u64,
    /// Entries by state.
    seen: BTreeMap<&'static str, u64>,
}

/// Reads and checks the report's `not_verified` against the map run's
/// observers `order` (with their plan positions), adding to `b` the counts and lines of the `map_ok` ones.
fn read_entries(
    report: &Value,
    order: &[(usize, &slip_model::Observer)],
    map_ok: &dyn Fn(&Digest) -> bool,
    b: &mut B,
) -> Result<Listed, String> {
    let position: BTreeMap<Digest, (usize, &slip_model::Observer)> =
        order.iter().map(|(i, o)| (o.id, (*i, *o))).collect();
    let entries = member(report, "", "not_verified")?
        .as_array()
        .ok_or_else(|| "the report's `not_verified` is not an array".to_owned())?;
    let mut seen: BTreeMap<&'static str, u64> = STATES.iter().map(|s| (*s, 0)).collect();
    let mut listed =
        Listed { observers: BTreeSet::new(), indeterminate_only: 0, seen: BTreeMap::new() };
    let mut last: Option<usize> = None;
    for (i, e) in entries.iter().enumerate() {
        let path = format!("not_verified[{i}].");
        let field = |k: &str| member(e, &path, k);
        let at = |k: &str| format!("{path}{k}");
        let observer = digest(field("observer")?, &at("observer"))?;
        let Some(&(pos, o)) = position.get(&observer) else {
            return Err(format!("the report's `{path}observer` {observer} is not of the map run"));
        };
        if last.is_some_and(|l| l >= pos) {
            return Err(format!(
                "the report's `{path}observer` {observer} is out of plan order or repeated"
            ));
        }
        last = Some(pos);
        listed.observers.insert(observer);
        let main = bytes(field("main")?, &at("main"))?;
        if main != o.main.path {
            return Err(format!(
                "the report's `{path}main` {} is not the plan's main file {} of {observer}",
                text(&main),
                text(&o.main.path)
            ));
        }
        let state = string(field("state")?, &at("state"))?;
        let Some(count) = seen.get_mut(state) else {
            return Err(format!("the report's `{path}state` `{state}` is not a state"));
        };
        *count += 1;
        let message = string(field("message")?, &at("message"))?;
        let findings = uv(field("findings")?, &at("findings"))?;
        let indeterminate = uv(field("indeterminate")?, &at("indeterminate"))?;
        uv(field("listed")?, &at("listed"))?;
        let message = match state {
            "verified" | "unchanged" => {
                return Err(format!(
                    "the report lists {observer} as not verified in state `{state}`"
                ));
            }
            "mismatch" => {
                if findings == 0 || indeterminate > findings {
                    return Err(format!(
                        "the report's mismatch of {observer} has {findings} finding(s), \
                         {indeterminate} indeterminate"
                    ));
                }
                if findings == indeterminate {
                    listed.indeterminate_only += 1;
                }
                format!("{findings} finding(s), {indeterminate} indeterminate")
            }
            _ => first_line(message).to_owned(),
        };
        match (state, map_ok(&observer)) {
            ("mismatch" | "not_parsed", false) => {
                return Err(format!(
                    "the report has {observer} `{state}`, but its map shard is not `ok` (a \
                     `{state}` observer has an `ok` map shard): the report is not of this map run"
                ));
            }
            ("never_parsed", true) => {
                return Err(format!(
                    "the report has {observer} `never_parsed`, but its map shard is `ok`: the \
                     report is not of this map run"
                ));
            }
            ("never_parsed", false) => b.never_parsed += 1,
            (_, false) => {}
            (_, true) => {
                match state {
                    "mismatch" if findings == indeterminate => b.indeterminate_only += 1,
                    "mismatch" => b.different += 1,
                    "not_parsed" => b.not_parsed += 1,
                    "unverifiable" => b.unverifiable += 1,
                    "failed" => b.failed += 1,
                    _ => unreachable!("the states are checked above, not `{state}`"),
                }
                b.not_verified.push(NotVerified {
                    observer,
                    main,
                    state: state.to_owned(),
                    message,
                });
            }
        }
    }
    listed.seen = seen;
    Ok(listed)
}

/// The report's header (`report`, `plan`, `round`, `declared`, `image`)
/// is of the map run's plan, round 0, the identity, nothing declared;
/// returns its `run`.
fn check_header(map: &Loaded, report: &Value) -> Result<Vec<u8>, String> {
    if report.as_object().is_none() {
        return Err("the report is not a JSON object".to_owned());
    }
    let kind = string(member(report, "", "report")?, "report")?;
    if kind != REPORT {
        return Err(format!("the report is `{kind}`, not `{REPORT}`"));
    }
    let plan = digest(member(report, "", "plan")?, "plan")?;
    if plan != map.path.plan {
        return Err(format!("the report is of plan {plan}, the map run of plan {}", map.path.plan));
    }
    let round = uv(member(report, "", "round")?, "round")?;
    if round != 0 {
        return Err(format!("the report is of round {round}, not 0"));
    }
    let declared = digest(member(report, "", "declared")?, "declared")?;
    let none = slip_model::formats::content_id(&Declared::none().to_bytes())
        .unwrap_or_else(|e| unreachable!("Declared::none() is a valid DECL: {e}"));
    if declared != none {
        return Err(format!("the report declares {declared}, not nothing ({none})"));
    }
    let image = member(report, "", "image")?
        .as_array()
        .ok_or_else(|| "the report's `image` is not an array".to_owned())?;
    if !image.is_empty() {
        return Err(format!(
            "the report's image has {} root map(s): not the identity",
            image.len()
        ));
    }
    let run = bytes(member(report, "", "run")?, "run")?;
    Ok(run)
}

/// The output of (b), §9.2. `report` is the report's path.
pub fn render_b(corpus: &str, plan: &Digest, b: &B, report: &Path, selection: bool) -> String {
    let mut o = String::new();
    let _ = writeln!(o, "invariant b {corpus}");
    let _ = writeln!(o, "plan {plan}");
    for (name, n) in [
        ("observers", b.observers),
        ("map_ok", b.map_ok),
        ("verified", b.verified),
        ("indeterminate_only", b.indeterminate_only),
        ("different", b.different),
        ("not_parsed", b.not_parsed),
        ("unverifiable", b.unverifiable),
        ("failed", b.failed),
        ("never_parsed", b.never_parsed),
    ] {
        let _ = writeln!(o, "{name} {n}");
    }
    for n in b.not_verified.iter().take(LISTED) {
        let _ = writeln!(o, "  {}: {}: {}", n.state, text(&n.main), text(n.message.as_bytes()));
    }
    let _ = writeln!(o, "run {}", text(&b.run));
    let _ = writeln!(o, "report {}", text(report.as_os_str().as_encoded_bytes()));
    let verdict = if b.map_ok == 0 {
        "FAILED (no observer with an ok map shard)".to_owned()
    } else if b.holds() {
        "holds".to_owned()
    } else {
        format!("FAILED ({} of {} observer(s) not verified)", b.map_ok - b.verified, b.map_ok)
    };
    let _ = writeln!(o, "invariant b: {verdict}{}", if selection { " (selection)" } else { "" });
    o
}

// ---------------------------------------------------------------------------
// The command

/// The options of §9 after `a|b <corpus>`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Options {
    pub slip: Option<PathBuf>,
    pub only: Vec<String>,
    /// As given (passed to `slip` unchanged) and as read.
    pub limit: Option<(String, u64)>,
    pub jobs: Option<String>,
}

impl Options {
    /// Whether the run is over a selection (`--only` or `--limit`).
    pub fn selection(&self) -> bool {
        !self.only.is_empty() || self.limit.is_some()
    }

    /// The options passed to `slip` (`--only`, `--limit`, `-j`), unchanged.
    fn slip_args(&self) -> Vec<OsString> {
        let mut v = Vec::new();
        for o in &self.only {
            v.extend(["--only".into(), o.into()]);
        }
        if let Some((l, _)) = &self.limit {
            v.extend(["--limit".into(), l.into()]);
        }
        if let Some(j) = &self.jobs {
            v.extend(["-j".into(), j.into()]);
        }
        v
    }

    /// The observers of `plan` the options select, in plan order, as
    /// `slip` selects them (`spec/exec.md` §8.2): those whose main file
    /// contains one of the `--only` substrings (all, without one), then
    /// the first `--limit` of them.
    pub fn select(&self, plan: &Plan) -> Vec<Digest> {
        let keep = |main: &[u8]| {
            self.only.is_empty()
                || self.only.iter().any(|s| {
                    let s = s.as_bytes();
                    s.is_empty() || main.windows(s.len()).any(|w| w == s)
                })
        };
        let limit = self
            .limit
            .as_ref()
            .map_or(usize::MAX, |(_, n)| usize::try_from(*n).unwrap_or(usize::MAX));
        plan.observers.iter().filter(|o| keep(&o.main.path)).take(limit).map(|o| o.id).collect()
    }
}

/// `a` or `b`, the corpus and the options; `Err` is a usage error.
pub fn parse_args(args: &[String]) -> Result<(char, String, Options), String> {
    let (which, corpus, rest) = match args {
        [w, c, rest @ ..] if (w == "a" || w == "b") && !c.starts_with('-') => {
            (if w == "a" { 'a' } else { 'b' }, c.clone(), rest)
        }
        [w, ..] if w != "a" && w != "b" => return Err(format!("unknown invariant `{w}`")),
        [] => return Err("missing invariant (a or b)".to_owned()),
        _ => return Err("missing corpus".to_owned()),
    };
    let mut o = Options::default();
    let mut it = rest.iter();
    while let Some(arg) = it.next() {
        let mut value = || it.next().cloned().ok_or_else(|| format!("`{arg}` needs a value"));
        match arg.as_str() {
            "--slip" if o.slip.is_none() => o.slip = Some(PathBuf::from(value()?)),
            "--only" => o.only.push(value()?),
            "--limit" if o.limit.is_none() => {
                let v = value()?;
                let n = (!v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()))
                    .then(|| v.parse::<u64>().ok())
                    .flatten()
                    .ok_or_else(|| format!("`--limit {v}` is not a number"))?;
                o.limit = Some((v, n));
            }
            "-j" if o.jobs.is_none() => o.jobs = Some(value()?),
            "--slip" | "--limit" | "-j" => return Err(format!("`{arg}` is given twice")),
            other => return Err(format!("unknown argument `{other}`")),
        }
    }
    Ok((which, corpus, o))
}

/// Why the command failed: a usage error (exit status 2) or any other
/// error (exit status 1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    Usage(String),
    Error(String),
}

/// `cargo xtask invariants …` over the repository `repo`: [`run`] with
/// the process's environment and standard output.
pub fn main(repo: &Path, args: &[String]) -> ExitCode {
    let env = Env {
        corpus_dir: std::env::var_os("SLIP_CORPUS_DIR"),
        target_dir: std::env::var_os("CARGO_TARGET_DIR"),
    };
    let mut stdout = std::io::stdout();
    match run(repo, args, &env, &mut stdout) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(Failure::Usage(e)) => {
            eprintln!("xtask invariants: {e}\n{USAGE}");
            ExitCode::from(2)
        }
        Err(Failure::Error(e)) => {
            eprintln!("xtask invariants: {e}");
            ExitCode::from(1)
        }
    }
}

/// The environment the command reads.
#[derive(Debug, Clone, Default)]
pub struct Env {
    /// `$SLIP_CORPUS_DIR`.
    pub corpus_dir: Option<OsString>,
    /// `$CARGO_TARGET_DIR`.
    pub target_dir: Option<OsString>,
}

/// Runs `args` (after `invariants`), writing the output to `out`.
/// `Ok(false)`: the invariant does not hold.
pub fn run(repo: &Path, args: &[String], env: &Env, out: &mut dyn Write) -> Result<bool, Failure> {
    let (which, name, opts) = parse_args(args).map_err(Failure::Usage)?;
    let e = Failure::Error;
    let corpora = rows::load(repo).map_err(e)?;
    let c = rows::find(&corpora, &name).map_err(e)?;
    let root = corpus::corpus_root(repo, env.corpus_dir.as_deref()).map_err(e)?;
    let layout = Layout::new(&root, &c.name);
    configured(c, &layout).map_err(e)?;
    let slip =
        slip_run::find_slip(repo, opts.slip.as_deref(), env.target_dir.as_deref()).map_err(e)?;
    let ctx = Ctx { slip: &slip, layout: &layout, opts: &opts };
    let (text, holds) = match which {
        'a' => invariant_a(c, &ctx).map_err(e)?,
        _ => invariant_b(c, &ctx).map_err(e)?,
    };
    out.write_all(text.as_bytes())
        .and_then(|()| out.flush())
        .map_err(|err| Failure::Error(format!("cannot write the output: {err}")))?;
    Ok(holds)
}

/// The corpus is configured (§8.3): the configure and generate stamps are
/// current and the database reads.
fn configured(c: &Corpus, layout: &Layout) -> Result<(), String> {
    let s = corpus::status(c, layout);
    if s.configured && s.generated && matches!(s.database, Some(Ok(_))) {
        Ok(())
    } else {
        Err(format!(
            "corpus `{}` is not configured: run `cargo xtask corpus configure {}`",
            c.name, c.name
        ))
    }
}

struct Ctx<'a> {
    slip: &'a Path,
    layout: &'a Layout,
    opts: &'a Options,
}

impl Ctx<'_> {
    fn stores(&self) -> PathBuf {
        self.layout.dir.join("stores")
    }

    /// Runs `slip` with `args`; a status not in `accept` is an error.
    fn slip(&self, args: &[OsString], accept: &[i32]) -> Result<Ran, String> {
        let what = args.first().map(|a| a.to_string_lossy().into_owned()).unwrap_or_default();
        let ran = slip_run::run(self.slip, args)?;
        if accept.contains(&ran.status) {
            Ok(ran)
        } else {
            Err(format!("`slip {what}` exited with status {}", ran.status))
        }
    }

    /// `slip parse` into the fresh store `store` (§9.1): the store is
    /// removed first. Returns the run as loaded, and the `run` line.
    fn parse_into(&self, store: &Path) -> Result<(Loaded, String), String> {
        slip_run::remove_tree(&self.layout.dir, store)?;
        let stores = self.stores();
        std::fs::create_dir_all(&stores)
            .map_err(|e| format!("cannot create {}: {e}", stores.display()))?;
        let mut args: Vec<OsString> = vec![
            "parse".into(),
            "--db".into(),
            self.layout.database().into(),
            "--store".into(),
            store.into(),
        ];
        args.extend(self.opts.slip_args());
        let ran = self.slip(&args, &[0, 1])?;
        let plan = slip_run::line(&ran.stdout, "plan")
            .and_then(Digest::from_hex)
            .ok_or_else(|| "`slip parse` printed no `plan <id>` line".to_owned())?;
        let printed = slip_run::line(&ran.stdout, "run")
            .ok_or_else(|| "`slip parse` printed no `run <path>` line".to_owned())?
            .to_owned();
        let path = latest_map_run(store, &plan)?.ok_or_else(|| {
            format!("{}: `slip parse` left no map run of plan {plan}", store.display())
        })?;
        let real = |p: &Path| std::fs::canonicalize(p).ok();
        if real(Path::new(&printed)).is_none() || real(Path::new(&printed)) != real(&path) {
            return Err(format!(
                "`slip parse` printed the run {printed}, but the store's latest map run is {}",
                path.display()
            ));
        }
        let loaded = perf::load(&path)?;
        Ok((loaded, printed))
    }
}

/// The path of the latest map run record of `plan` in `store`, if any.
fn latest_map_run(store: &Path, plan: &Digest) -> Result<Option<PathBuf>, String> {
    let st = Store::open(store, Access::Shared)
        .map_err(|e| format!("{}: cannot open the store: {e}", store.display()))?;
    let Some(bytes) = st.latest_run(plan, Phase::Map).map_err(|e| e.to_string())? else {
        return Ok(None);
    };
    let r = RunRecord::from_bytes(&bytes)
        .map_err(|e| format!("{}: the latest map run is not valid: {e}", store.display()))?;
    Ok(Some(st.run_path(plan, Phase::Map, r.run.round, r.run.started_unix_ns)))
}

/// Every observer of `run`'s record is in `selected`.
fn within(run: &Loaded, selected: &[Digest]) -> Result<(), String> {
    let set: BTreeSet<&Digest> = selected.iter().collect();
    match run.record.jobs.iter().find(|j| !set.contains(&j.observer)) {
        None => Ok(()),
        Some(j) => Err(format!(
            "{}: the run has a job for observer {}, which the selection does not keep",
            run.path.store.display(),
            j.observer
        )),
    }
}

fn invariant_a(c: &Corpus, ctx: &Ctx) -> Result<(String, bool), String> {
    let stores = ctx.stores();
    let (first, run1) = ctx.parse_into(&stores.join("a1"))?;
    let (second, run2) = ctx.parse_into(&stores.join("a2"))?;
    if first.path.plan != second.path.plan {
        return Err(format!(
            "the two runs have two plans, {} and {}: the corpus changed between them",
            first.path.plan, second.path.plan
        ));
    }
    let selected = ctx.opts.select(&first.plan);
    within(&first, &selected)?;
    within(&second, &selected)?;
    let a = compare_runs(&first.plan, &first, &second, &selected);
    let text = render_a(&c.name, &first.path.plan, &a, [&run1, &run2], ctx.opts.selection());
    Ok((text, a_holds(&a)))
}

/// `slip --version`: `slip <version> (kiln <version>)`.
fn versions(ctx: &Ctx) -> Result<(String, String), String> {
    let ran = ctx.slip(&["--version".into()], &[0])?;
    let line = ran.stdout.lines().next().unwrap_or("");
    line.strip_prefix("slip ")
        .and_then(|r| r.strip_suffix(')'))
        .and_then(|r| r.split_once(" (kiln "))
        .map(|(s, k)| (s.to_owned(), k.to_owned()))
        .ok_or_else(|| format!("`slip --version` printed `{line}`, not `slip <v> (kiln <v>)`"))
}

/// `stores/a1`'s latest map run, if it is of the corpus's plan, made by
/// this `slip`, round 0, complete, and over exactly the selection.
fn reusable(ctx: &Ctx, store: &Path, tools: &(String, String)) -> Result<Option<Loaded>, String> {
    if !store.is_dir() {
        return Ok(None);
    }
    let args = vec![
        "plan".into(),
        "--db".into(),
        ctx.layout.database().into(),
        "--store".into(),
        store.into(),
    ];
    let ran = ctx.slip(&args, &[0, 1])?;
    let plan = slip_run::line(&ran.stdout, "plan")
        .and_then(Digest::from_hex)
        .ok_or_else(|| "`slip plan` printed no `plan <id>` line".to_owned())?;
    let Some(path) = latest_map_run(store, &plan)? else { return Ok(None) };
    let loaded = perf::load(&path)?;
    let run = &loaded.record.run;
    let selected: BTreeSet<Digest> = ctx.opts.select(&loaded.plan).into_iter().collect();
    let observers: BTreeSet<Digest> = loaded.shards.keys().copied().collect();
    let same = (run.tool_slip.as_str(), run.tool_kiln.as_str()) == (&tools.0[..], &tools.1[..])
        && run.round == 0
        && loaded.missing_shards() == 0
        && observers == selected;
    Ok(same.then_some(loaded))
}

fn invariant_b(c: &Corpus, ctx: &Ctx) -> Result<(String, bool), String> {
    let tools = versions(ctx)?;
    let store = ctx.stores().join("a1");
    let map = if let Some(map) = reusable(ctx, &store, &tools)? {
        map
    } else {
        let (map, _) = ctx.parse_into(&store)?;
        within(&map, &ctx.opts.select(&map.plan))?;
        map
    };
    let report = ctx.stores().join("b.json");
    slip_run::remove_tree(&ctx.layout.dir, &report)?;
    let mut args: Vec<OsString> = vec![
        "verify".into(),
        "--plan".into(),
        map.path.plan.to_string().into(),
        "--store".into(),
        store.into(),
        "--round".into(),
        "0".into(),
        "--report".into(),
        report.clone().into(),
    ];
    args.extend(ctx.opts.slip_args());
    ctx.slip(&args, &[0, 1, 4])?;
    let bytes =
        std::fs::read(&report).map_err(|e| format!("{}: cannot read: {e}", report.display()))?;
    let value = json::parse(&bytes).map_err(|e| format!("{}: not JSON: {e}", report.display()))?;
    let b = check_identity(&map, &value).map_err(|e| format!("{}: {e}", report.display()))?;
    let text = render_b(&c.name, &map.path.plan, &b, &report, ctx.opts.selection());
    Ok((text, b.holds()))
}
