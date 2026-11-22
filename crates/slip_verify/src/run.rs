//! The run of the verify phase (`spec/verify.md` §7): the state of each
//! selected observer, the run's verdict, and the report (§7.1) in JSON and
//! text. Work package WP-055; the interface is §7.3.
//!
//! Pure and part of the trusted base: what a person reads as "verified" is
//! decided here. The rule every function keeps is that nothing it is not
//! sure of becomes `verified`: a shard that breaks an invariant of its
//! format, a verdict whose counters are missing or disagree with it, and a
//! shard of another observer or round are all `failed`.
//!
//! [`Report::add`] keeps what the report needs of a shard (its state, its
//! listed findings folded into groups, its used exemptions) and drops the
//! shard.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use slip_model::declared::Declared;
use slip_model::json::Value;
use slip_model::verdict::{Finding, FindingKind, Site, Verdict, VerdictKind, Where};
use slip_model::{Digest, Phase, Shard, Status};

/// The report's format name (§7.1).
const REPORT: &str = "slip.verify.v1";

/// How many finding groups and `not_verified` lines the text shows (§7.1).
const TEXT_MAX: usize = 100;

/// The state of one selected observer (§7).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum State {
    Verified,
    Unchanged,
    Mismatch,
    NotParsed,
    Unverifiable,
    NeverParsed,
    Failed,
}

impl State {
    /// In the order of the table of §7, which is the order of the report.
    pub const ALL: [State; 7] = [
        State::Verified,
        State::Unchanged,
        State::Mismatch,
        State::NotParsed,
        State::Unverifiable,
        State::NeverParsed,
        State::Failed,
    ];

    /// Its name in the report.
    pub fn name(self) -> &'static str {
        match self {
            State::Verified => "verified",
            State::Unchanged => "unchanged",
            State::Mismatch => "mismatch",
            State::NotParsed => "not_parsed",
            State::Unverifiable => "unverifiable",
            State::NeverParsed => "never_parsed",
            State::Failed => "failed",
        }
    }

    /// Its index in [`State::ALL`].
    fn index(self) -> usize {
        match self {
            State::Verified => 0,
            State::Unchanged => 1,
            State::Mismatch => 2,
            State::NotParsed => 3,
            State::Unverifiable => 4,
            State::NeverParsed => 5,
            State::Failed => 6,
        }
    }
}

/// The counters of §7.2 by kind, in the order of `FindingKind`'s numbers.
const BY_KIND: [(FindingKind, &str); 6] = [
    (FindingKind::Changed, "verify.findings.changed"),
    (FindingKind::Removed, "verify.findings.removed"),
    (FindingKind::Added, "verify.findings.added"),
    (FindingKind::Indeterminate, "verify.findings.indeterminate"),
    (FindingKind::DerivedChanged, "verify.findings.derived_changed"),
    (FindingKind::DerivedIndeterminate, "verify.findings.derived_indeterminate"),
];

/// The number of a finding kind in the format, as an index of [`BY_KIND`].
fn kind_index(kind: FindingKind) -> usize {
    match kind {
        FindingKind::Changed => 0,
        FindingKind::Removed => 1,
        FindingKind::Added => 2,
        FindingKind::Indeterminate => 3,
        FindingKind::DerivedChanged => 4,
        FindingKind::DerivedIndeterminate => 5,
    }
}

/// The name of a finding kind (`spec/verify.md` §6).
fn kind_name(kind: FindingKind) -> &'static str {
    match kind {
        FindingKind::Changed => "changed",
        FindingKind::Removed => "removed",
        FindingKind::Added => "added",
        FindingKind::Indeterminate => "indeterminate",
        FindingKind::DerivedChanged => "derived_changed",
        FindingKind::DerivedIndeterminate => "derived_indeterminate",
    }
}

/// The value of the counter `name` of `shard`, if it has one. The counters
/// of a validated shard are strictly ascending by name: one value at most.
fn counter(shard: &Shard, name: &str) -> Option<u64> {
    shard.counters.iter().find(|c| c.name == name).map(|c| c.value)
}

/// The counts of the verdict's findings by kind (§7.2), checked against the
/// verdict: `None` if one of the six is missing, if they do not add up to
/// `total`, if `verify.findings` is present and is not `total`, or if the
/// verdict lists more findings of a kind than its counter says.
fn counts_by_kind(shard: &Shard, verdict: &Verdict) -> Option<[u64; 6]> {
    let mut counts = [0u64; 6];
    for (i, (_, name)) in BY_KIND.iter().enumerate() {
        counts[i] = counter(shard, name)?;
    }
    let sum = counts.iter().try_fold(0u64, |a, &c| a.checked_add(c))?;
    if sum != verdict.total {
        return None;
    }
    if counter(shard, "verify.findings").is_some_and(|n| n != verdict.total) {
        return None;
    }
    let mut listed = [0u64; 6];
    for f in &verdict.findings {
        listed[kind_index(f.kind)] += 1;
    }
    if listed.iter().zip(&counts).any(|(l, c)| l > c) {
        return None;
    }
    Some(counts)
}

/// What [`state`] knows of a shard it accepts with a verdict.
struct Judged<'s> {
    state: State,
    /// For `Verified`, `Mismatch` and `Unverifiable`: the verdict and its
    /// counts by kind.
    verdict: Option<(&'s Verdict, [u64; 6])>,
}

fn judge(verify: Option<&Shard>, map: Option<Status>) -> Judged<'_> {
    let failed = Judged { state: State::Failed, verdict: None };
    let Some(shard) = verify else { return failed };
    // A shard of another phase, or one that breaks an invariant of its
    // format, is nothing this run can read a result from.
    if shard.phase != Phase::Verify || shard.validate().is_err() {
        return failed;
    }
    match shard.outcome.status {
        Status::Failed => failed,
        Status::Diagnosed | Status::Rejected => {
            let state = if map == Some(Status::Ok) { State::NotParsed } else { State::NeverParsed };
            Judged { state, verdict: None }
        }
        Status::Ok => {
            let Some(verdict) = &shard.verdict else { return failed };
            let Some(counts) = counts_by_kind(shard, verdict) else { return failed };
            // A comparison is believed only against the pre shard the store
            // has now: an ok map shard. `unverifiable` compared nothing.
            let compared = map == Some(Status::Ok);
            let state = match verdict.kind {
                VerdictKind::Verified if compared => State::Verified,
                VerdictKind::Mismatch if compared => State::Mismatch,
                VerdictKind::Verified | VerdictKind::Mismatch => return failed,
                VerdictKind::Unverifiable => State::Unverifiable,
            };
            Judged { state, verdict: Some((verdict, counts)) }
        }
    }
}

/// §7: `verify` is this run's shard of the observer (None: there is none, or
/// it is not this run's); `map` the status of its map shard (None: absent).
///
/// A shard that is not of phase `verify`, that `Shard::validate` rejects,
/// or whose verdict lacks the counters of §7.2 by kind or disagrees with
/// them (they must add up to `total`, `verify.findings` must be `total`
/// when present, and no kind may have more findings listed than counted)
/// is `Failed`; so is a `verified` or `mismatch` verdict when `map` is
/// not `Some(Status::Ok)` (the pre shard it compared against is not the
/// one the store has now). `Unchanged` is never returned: it is decided
/// without a shard (§5.5, not in M1).
pub fn state(verify: Option<&Shard>, map: Option<Status>) -> State {
    judge(verify, map).state
}

/// The run's verdict (§7).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RunVerdict {
    Verified,
    NotVerified,
    Incomplete,
}

impl RunVerdict {
    /// Its name in the report.
    fn name(self) -> &'static str {
        match self {
            RunVerdict::Verified => "verified",
            RunVerdict::NotVerified => "not_verified",
            RunVerdict::Incomplete => "incomplete",
        }
    }
}

/// §7: `NotVerified` if some state is `Mismatch` or `NotParsed`; else
/// `Verified` if there is at least one state and every one is `Verified`
/// or `Unchanged`; else `Incomplete` (also for no state at all).
pub fn run_verdict(states: &[State]) -> RunVerdict {
    if states.iter().any(|s| matches!(s, State::Mismatch | State::NotParsed)) {
        RunVerdict::NotVerified
    } else if !states.is_empty()
        && states.iter().all(|s| matches!(s, State::Verified | State::Unchanged))
    {
        RunVerdict::Verified
    } else {
        RunVerdict::Incomplete
    }
}

/// What the report says before any observer (§7.1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Header {
    pub plan: Digest,
    pub round: u64,
    /// The content id of the `DECL` container.
    pub declared: Digest,
    pub rules: Digest,
    /// `(subject, post)` per root map, in the image's order; empty for the
    /// identity.
    pub image: Vec<(Vec<u8>, Vec<u8>)>,
    pub limit: u64,
    pub locate: u64,
    /// The number of observers of the plan.
    pub plan_observers: u64,
    /// The path of the run record.
    pub run: Vec<u8>,
}

/// The listed findings of one `(kind, key)` across observers (§7.1).
#[derive(Clone, Debug)]
struct FindingGroup {
    observers: u64,
    /// The index (in `add` order) of the last observer counted, so that an
    /// observer listing a `(kind, key)` twice (only possible without a key)
    /// is counted once.
    last: u64,
    /// The main file of the first observer that lists it.
    first: Vec<u8>,
    /// The finding the group shows: of the first observer whose finding has
    /// a name, else of the first.
    shown: Finding,
}

/// An entry of `not_verified` (§7.1).
#[derive(Clone, Debug)]
struct NotVerified {
    observer: Digest,
    main: Vec<u8>,
    state: State,
    message: String,
    findings: u64,
    indeterminate: u64,
    listed: u64,
}

/// The report of one run (§7.1), built one observer at a time.
#[derive(Clone, Debug)]
pub struct Report {
    header: Header,
    changed: Vec<Digest>,
    created: Vec<Digest>,
    removed: Vec<Digest>,
    /// How many `add` calls there were.
    selected: u64,
    states: [u64; 7],
    indeterminate_only: u64,
    total: u64,
    listed: u64,
    /// By `(kind's number, key)`.
    groups: BTreeMap<(u8, Option<Digest>), FindingGroup>,
    not_verified: Vec<NotVerified>,
    /// The observers whose verdict has the key in `used`, for every key of
    /// a list of `Declared`.
    used: BTreeMap<Digest, u64>,
    /// The verdict of the states so far, kept as `run_verdict` would give
    /// it without keeping the states.
    any_bad: bool,
    all_good: bool,
}

impl Report {
    pub fn new(header: Header, declared: &Declared) -> Report {
        let used = declared
            .changed()
            .iter()
            .chain(declared.created())
            .chain(declared.removed())
            .map(|&k| (k, 0))
            .collect();
        Report {
            header,
            changed: declared.changed().to_vec(),
            created: declared.created().to_vec(),
            removed: declared.removed().to_vec(),
            selected: 0,
            states: [0; 7],
            indeterminate_only: 0,
            total: 0,
            listed: 0,
            groups: BTreeMap::new(),
            not_verified: Vec::new(),
            used,
            any_bad: false,
            all_good: true,
        }
    }

    /// One call per selected observer, in plan order.
    ///
    /// `verify` is this run's shard of the observer, as for [`state`]; a
    /// shard whose `observer` is not `observer`, whose `round` is not the
    /// header's, or whose verdict's `rules` is not the header's is not this
    /// observer's result in this run: the observer is `Failed`, with an
    /// empty message. Only a `Mismatch` contributes findings, and only a
    /// `Verified` or `Mismatch` verdict contributes used exemptions.
    pub fn add(
        &mut self,
        observer: &Digest,
        main: &[u8],
        verify: Option<&Shard>,
        map: Option<Status>,
    ) {
        let index = self.selected;
        self.selected += 1;
        let rules = self.header.rules;
        let ours = verify.filter(|s| {
            s.observer == *observer
                && s.round == self.header.round
                && s.verdict.as_ref().is_none_or(|v| v.rules == rules)
        });
        let judged = if verify.is_some() && ours.is_none() {
            Judged { state: State::Failed, verdict: None }
        } else {
            judge(ours, map)
        };
        let state = judged.state;
        self.states[state.index()] += 1;
        self.any_bad |= matches!(state, State::Mismatch | State::NotParsed);
        self.all_good &= matches!(state, State::Verified | State::Unchanged);

        let (mut findings, mut indeterminate, mut listed) = (0, 0, 0);
        if let Some((verdict, counts)) = judged.verdict {
            if matches!(state, State::Verified | State::Mismatch) {
                for k in &verdict.used {
                    if let Some(n) = self.used.get_mut(k) {
                        *n += 1;
                    }
                }
            }
            if state == State::Mismatch {
                findings = verdict.total;
                indeterminate = counts[kind_index(FindingKind::Indeterminate)]
                    + counts[kind_index(FindingKind::DerivedIndeterminate)];
                listed = verdict.findings.len() as u64;
                if findings == indeterminate {
                    self.indeterminate_only += 1;
                }
                self.total = self.total.saturating_add(findings);
                self.listed = self.listed.saturating_add(listed);
                for f in &verdict.findings {
                    self.group(index, main, f);
                }
            }
        }
        if !matches!(state, State::Verified | State::Unchanged) {
            let message = match state {
                State::Unverifiable => {
                    judged.verdict.map(|(v, _)| v.reason.clone()).unwrap_or_default()
                }
                State::NotParsed | State::NeverParsed | State::Failed => {
                    ours.map(|s| s.outcome.message.clone()).unwrap_or_default()
                }
                State::Mismatch | State::Verified | State::Unchanged => String::new(),
            };
            self.not_verified.push(NotVerified {
                observer: *observer,
                main: main.to_vec(),
                state,
                message,
                findings,
                indeterminate,
                listed,
            });
        }
    }

    /// Folds one listed finding of observer number `index` into its group.
    fn group(&mut self, index: u64, main: &[u8], f: &Finding) {
        let id = (kind_index(f.kind) as u8, f.key);
        match self.groups.get_mut(&id) {
            None => {
                let g = FindingGroup {
                    observers: 1,
                    last: index,
                    first: main.to_vec(),
                    shown: f.clone(),
                };
                self.groups.insert(id, g);
            }
            Some(g) => {
                if g.last != index {
                    g.observers += 1;
                    g.last = index;
                }
                if g.shown.name.is_empty() && !f.name.is_empty() {
                    g.shown = f.clone();
                }
            }
        }
    }

    /// The run's verdict over the observers added so far (§7).
    pub fn verdict(&self) -> RunVerdict {
        if self.any_bad {
            RunVerdict::NotVerified
        } else if self.selected > 0 && self.all_good {
            RunVerdict::Verified
        } else {
            RunVerdict::Incomplete
        }
    }

    /// The number of observers in each state, in the order of [`State::ALL`].
    pub fn states(&self) -> [u64; 7] {
        self.states
    }

    /// The groups in report order: by `(rank, name, key)` (§7.1).
    fn sorted_groups(&self) -> Vec<&FindingGroup> {
        let mut v: Vec<&FindingGroup> = self.groups.values().collect();
        v.sort_by(|a, b| {
            (a.shown.kind.rank(), &a.shown.name, a.shown.key).cmp(&(
                b.shown.kind.rank(),
                &b.shown.name,
                b.shown.key,
            ))
        });
        v
    }

    /// The exemption entries: `(list, key, observers)` in the order
    /// `changed`, `created`, `removed`, then by key.
    fn exemptions(&self) -> Vec<(&'static str, Digest, u64)> {
        let lists =
            [("changed", &self.changed), ("created", &self.created), ("removed", &self.removed)];
        let mut out = Vec::new();
        for (list, keys) in lists {
            for k in keys {
                out.push((list, *k, self.used.get(k).copied().unwrap_or(0)));
            }
        }
        out
    }

    /// The JSON document of §7.1.
    pub fn to_json(&self) -> Value {
        let h = &self.header;
        let image = h
            .image
            .iter()
            .map(|(s, p)| obj(vec![("subject", bytes(s)), ("post", bytes(p))]))
            .collect();
        let mut observers = vec![("plan", uv(h.plan_observers)), ("selected", uv(self.selected))];
        for s in State::ALL {
            observers.push((s.name(), uv(self.states[s.index()])));
        }
        observers.push(("indeterminate_only", uv(self.indeterminate_only)));
        let groups = self
            .sorted_groups()
            .into_iter()
            .map(|g| {
                let f = &g.shown;
                obj(vec![
                    ("kind", Value::Str(kind_name(f.kind).to_owned())),
                    ("key", opt_digest(f.key.as_ref())),
                    ("name", Value::Str(f.name.clone())),
                    ("observers", uv(g.observers)),
                    ("first", bytes(&g.first)),
                    ("file", bytes(&f.file)),
                    ("pre", opt_digest(f.pre.as_ref())),
                    ("post", opt_digest(f.post.as_ref())),
                    ("where", f.location.as_ref().map_or(Value::Null, where_json)),
                ])
            })
            .collect();
        let not_verified = self
            .not_verified
            .iter()
            .map(|n| {
                obj(vec![
                    ("observer", digest(&n.observer)),
                    ("main", bytes(&n.main)),
                    ("state", Value::Str(n.state.name().to_owned())),
                    ("message", Value::Str(n.message.clone())),
                    ("findings", uv(n.findings)),
                    ("indeterminate", uv(n.indeterminate)),
                    ("listed", uv(n.listed)),
                ])
            })
            .collect();
        let exemptions = self
            .exemptions()
            .into_iter()
            .map(|(list, key, n)| {
                obj(vec![
                    ("list", Value::Str(list.to_owned())),
                    ("key", digest(&key)),
                    ("observers", uv(n)),
                ])
            })
            .collect();
        obj(vec![
            ("report", Value::Str(REPORT.to_owned())),
            ("plan", digest(&h.plan)),
            ("round", uv(h.round)),
            ("declared", digest(&h.declared)),
            ("rules", digest(&h.rules)),
            ("image", Value::Array(image)),
            ("limit", uv(h.limit)),
            ("locate", uv(h.locate)),
            ("verdict", Value::Str(self.verdict().name().to_owned())),
            ("observers", obj(observers)),
            (
                "findings",
                obj(vec![
                    ("total", uv(self.total)),
                    ("listed", uv(self.listed)),
                    ("groups", Value::Array(groups)),
                ]),
            ),
            ("not_verified", Value::Array(not_verified)),
            ("exemptions", Value::Array(exemptions)),
            ("run", bytes(&h.run)),
        ])
    }

    /// §7.1 from `plan` to `exemption`: the caller adds `wall`, `peak_rss`,
    /// `run`, `report` and prints `verdict_line()` last.
    ///
    /// Every path and every string from a shard is written as `escape`
    /// gives it, so none can write a line or a control sequence; a key is
    /// its first 16 hexadecimal digits (`-` for a finding without one); a
    /// line's last word is left out when it is empty, so no line ends in a
    /// blank.
    pub fn to_text(&self) -> String {
        let h = &self.header;
        let mut out = String::new();
        let _ = writeln!(out, "plan {}", h.plan);
        let _ = writeln!(out, "round {}", h.round);
        let _ = writeln!(out, "declared {} rules {}", h.declared, h.rules);
        if h.image.is_empty() {
            out.push_str("image identity\n");
        }
        for (s, p) in &h.image {
            let _ = writeln!(out, "image {} => {}", path(s), path(p));
        }
        let _ = writeln!(out, "selected {} of {}", self.selected, h.plan_observers);
        for s in State::ALL {
            let _ = writeln!(out, "observer {} {}", s.name(), self.states[s.index()]);
        }
        let _ = writeln!(out, "mismatch indeterminate_only {}", self.indeterminate_only);
        let groups = self.sorted_groups();
        for g in groups.iter().take(TEXT_MAX) {
            let f = &g.shown;
            let key = f.key.map_or_else(|| "-".to_owned(), |k| short(&k));
            line(
                &mut out,
                &format!("finding {} {key} observers {}", kind_name(f.kind), g.observers),
                &[&text(&f.name)],
            );
            let _ = writeln!(out, "  first {}", path(&g.first));
            if let Some(w) = &f.location {
                site_line(&mut out, "  pre  ", &w.pre);
                site_line(&mut out, "  post ", &w.post);
            }
        }
        let _ = writeln!(
            out,
            "findings {} total, {} listed, {} group(s)",
            self.total,
            self.listed,
            groups.len()
        );
        for (list, key, n) in self.exemptions() {
            let _ = writeln!(out, "exemption {list} {} observers {n}", short(&key));
        }
        out
    }

    /// The last line of the text (§7.1).
    pub fn verdict_line(&self) -> &'static str {
        match self.verdict() {
            RunVerdict::Verified => "verdict verified",
            RunVerdict::NotVerified => "verdict not verified",
            RunVerdict::Incomplete => "verdict incomplete",
        }
    }

    /// The lines for standard error (§7.1): one per entry of
    /// `not_verified`, at most [`TEXT_MAX`], then `slip: and <n> more (see
    /// the report)`. Each ends in a newline.
    pub fn error_lines(&self) -> String {
        let mut out = String::new();
        for n in self.not_verified.iter().take(TEXT_MAX) {
            let message = if n.state == State::Mismatch {
                format!("{} finding(s), {} indeterminate", n.findings, n.indeterminate)
            } else {
                text(n.message.lines().next().unwrap_or(""))
            };
            let _ = writeln!(out, "slip: {}: {}: {message}", n.state.name(), path(&n.main));
        }
        if self.not_verified.len() > TEXT_MAX {
            let more = self.not_verified.len() - TEXT_MAX;
            let _ = writeln!(out, "slip: and {more} more (see the report)");
        }
        out
    }
}

/// `head` and then each non-empty word of `tail`, blank-separated.
fn line(out: &mut String, head: &str, tail: &[&str]) {
    out.push_str(head);
    for w in tail {
        if !w.is_empty() {
            out.push(' ');
            out.push_str(w);
        }
    }
    out.push('\n');
}

fn site_line(out: &mut String, prefix: &str, s: &Site) {
    let place = if s.file.is_empty() {
        "-".to_owned()
    } else {
        format!("{}:{}:{}", path(&s.file), s.line, s.column)
    };
    line(out, &format!("{prefix}{place}"), &[&text(&s.node), &text(&s.what)]);
}

/// A key's first 16 hexadecimal digits.
fn short(d: &Digest) -> String {
    let mut s = d.to_string();
    s.truncate(16);
    s
}

/// Bytes for the text form (`spec/verify.md` §7.1): each byte that is not
/// UTF-8, each byte below 0x20, 0x7f, the backslash, and the bytes of a C1
/// control character (U+0080 to U+009F) as `\xNN` (lower-case hexadecimal);
/// every other character as it is. The result has no control character,
/// and `\` in it always starts an escape, so it reads back unambiguously.
fn escape(bytes: &[u8]) -> String {
    let mut out = String::new();
    for chunk in bytes.utf8_chunks() {
        for c in chunk.valid().chars() {
            if c.is_control() || c == '\\' {
                let mut buf = [0; 4];
                for b in c.encode_utf8(&mut buf).bytes() {
                    let _ = write!(out, "\\x{b:02x}");
                }
            } else {
                out.push(c);
            }
        }
        for b in chunk.invalid() {
            let _ = write!(out, "\\x{b:02x}");
        }
    }
    out
}

fn path(bytes: &[u8]) -> String {
    escape(bytes)
}

fn text(s: &str) -> String {
    escape(s.as_bytes())
}

// ---------------------------------------------------------------------------
// JSON values as `spec/model.md` §10 renders them

fn obj(members: Vec<(&str, Value)>) -> Value {
    Value::Object(members.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
}

/// A `uv`: a number below 2^53, else its decimal string.
fn uv(v: u64) -> Value {
    if v < 1 << 53 { Value::Int(i128::from(v)) } else { Value::Str(v.to_string()) }
}

fn digest(d: &Digest) -> Value {
    Value::Str(d.to_string())
}

fn opt_digest(d: Option<&Digest>) -> Value {
    d.map_or(Value::Null, digest)
}

/// Bytes: a string if UTF-8, else `{ "hex": … }` in lower case.
fn bytes(b: &[u8]) -> Value {
    if let Ok(s) = std::str::from_utf8(b) {
        return Value::Str(s.to_owned());
    }
    let mut s = String::with_capacity(b.len() * 2);
    for x in b {
        let _ = write!(s, "{x:02x}");
    }
    obj(vec![("hex", Value::Str(s))])
}

fn site_json(s: &Site) -> Value {
    obj(vec![
        ("file", bytes(&s.file)),
        ("line", uv(s.line)),
        ("column", uv(s.column)),
        ("node", Value::Str(s.node.clone())),
        ("what", Value::Str(s.what.clone())),
    ])
}

fn where_json(w: &Where) -> Value {
    obj(vec![("pre", site_json(&w.pre)), ("post", site_json(&w.post))])
}
