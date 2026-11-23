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
//!
//! M2 (WP-070, `spec/verify.md` §9.3 to §9.5, §10.3, §10.5, §10.6) adds
//! the two conditions over all observers: agreement (stale observers and
//! unused rules, from each shard's `rule_use`) and the spelled entities
//! (the fold of each shard's `spellings`, [`crate::spelling`]); and to
//! "this run's shard" what a shard alone decides of it (its `binding`, the
//! presence of `rule_use` and `spellings`). The verdict is [`combine`] of
//! M1's. A run with a tree image and no rule gives M1's report, byte for
//! byte.

use std::cell::OnceCell;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use slip_model::declared::Declared;
use slip_model::json::Value;
use slip_model::keys::KeyKind;
use slip_model::names::NameNote;
use slip_model::verdict::{Finding, FindingKind, Site, Verdict, VerdictKind, Where};
use slip_model::{Binding, Digest, Overlay, Phase, Shard, Status};

use crate::spelling::{Fold, Outcome};

/// The report's format name (§7.1).
const REPORT: &str = "slip.verify.v1";

/// How many finding groups and `not_verified` lines the text shows (§7.1).
const TEXT_MAX: usize = 100;

/// The report member, and the finding kind under `--strict-names`, of a
/// note (§7.1, WP-112).
const NAMES_CHANGED: &str = "names_changed";

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

/// The run's verdict (§7): the one type, defined in `slip_model` because
/// the result (`spec/edits.md` §8.5) stores it and `slip_model` cannot
/// depend on this crate. Its `name()` is the name in the report.
pub use slip_model::result::RunVerdict;

/// A stale observer (§9.2): the rule's index in `Declared.rules`, the
/// observer, and its `direct` count for the rule. The one type, as for
/// [`RunVerdict`]: the result (`spec/edits.md` §8.5) stores it.
pub use slip_model::result::Stale;

/// §9.3: the verdict of the states, with the two conditions over all
/// observers. `NotVerified` if `states` is, or if `stale` (stale pairs) or
/// `obligations_not_passed` is not 0: a stale observer and a failed
/// obligation are facts about the post-image, like a `mismatch`, and make
/// the run not verified also when other observers are `unverifiable` or
/// `failed`. Else `states`.
pub fn combine(states: RunVerdict, stale: usize, obligations_not_passed: usize) -> RunVerdict {
    if states == RunVerdict::NotVerified || stale != 0 || obligations_not_passed != 0 {
        RunVerdict::NotVerified
    } else {
        states
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
    /// The plan position of the last observer counted, so that an observer
    /// listing a `(kind, key)` twice (only possible without a key) is
    /// counted once: one call's findings are folded together.
    last: u64,
    /// The main file of the first observer in plan order that lists it,
    /// and that observer's plan position.
    first: Vec<u8>,
    first_at: u64,
    /// The finding the group shows: of the first observer in plan order
    /// whose finding has a name, else of the first; and its position.
    shown: Finding,
    shown_at: u64,
}

/// A key noted `names_changed` by at least one observer (§7.1, WP-112).
#[derive(Clone, Debug)]
struct NameGroup {
    /// How many observers noted it.
    observers: u64,
    /// The main file of the first observer in plan order that noted it,
    /// and that observer's plan position.
    first: Vec<u8>,
    first_at: u64,
    /// The note shown: the first observer's in plan order.
    name: String,
    file: Vec<u8>,
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

/// A stale pair with the main file of its observer, for the report.
#[derive(Clone, Debug)]
struct StaleAt {
    stale: Stale,
    main: Vec<u8>,
}

/// The overlay of a run of the overlay form (§9.5 `Report::overlay`).
#[derive(Clone, Debug)]
struct OverlayRun {
    id: Digest,
    overlay: Overlay,
    fold: Fold,
}

/// The report of one run (§7.1), built one observer at a time.
#[derive(Clone, Debug)]
pub struct Report {
    header: Header,
    /// Per rule of `Declared.rules`, by index: whether its kind is
    /// `namespace` (§9.2: such a rule is never stale).
    namespace: Vec<bool>,
    /// Per rule: whether some `ok` shard of this run used it (§9.2).
    used_rules: Vec<bool>,
    /// The round of the pre shards (§9.5 `pre_round`).
    pre: u64,
    overlay: Option<Box<OverlayRun>>,
    /// The stale pairs by `(rule, plan position)`.
    stale: BTreeMap<(u64, u64), StaleAt>,
    stale_count: usize,
    /// `stale` flattened, made on demand and dropped by `add`.
    stale_flat: OnceCell<Vec<Stale>>,
    /// The fold's outcomes, made on demand and dropped by `add`.
    outcomes: OnceCell<Vec<Outcome>>,
    /// The observer and main file of every plan position that became the
    /// `first` of an obligation's failure.
    firsts: BTreeMap<usize, (Digest, Vec<u8>)>,
    changed: Vec<Digest>,
    created: Vec<Digest>,
    removed: Vec<Digest>,
    /// How many `add` calls there were.
    selected: u64,
    /// The plan positions and the observers added (§7.3: each at most once).
    positions: BTreeSet<u64>,
    observers: BTreeSet<Digest>,
    states: [u64; 7],
    indeterminate_only: u64,
    total: u64,
    listed: u64,
    /// By `(kind's number, key)`.
    groups: BTreeMap<(u8, Option<Digest>), FindingGroup>,
    /// By plan position.
    not_verified: BTreeMap<u64, NotVerified>,
    /// The observers whose verdict has the key in `used`, for every key of
    /// a list of `Declared`.
    used: BTreeMap<Digest, u64>,
    /// The verdict of the states so far, kept as `run_verdict` would give
    /// it without keeping the states.
    any_bad: bool,
    all_good: bool,
    /// `--strict-names` (§7.1, WP-112): each note is a finding.
    strict_names: bool,
    /// The notes `names_changed` of the `verified` and `mismatch` shards,
    /// by key.
    names: BTreeMap<Digest, NameGroup>,
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
        let namespace: Vec<bool> =
            declared.rules().rules().iter().map(|r| r.kind == KeyKind::Namespace).collect();
        Report {
            header,
            used_rules: vec![false; namespace.len()],
            namespace,
            pre: 0,
            overlay: None,
            stale: BTreeMap::new(),
            stale_count: 0,
            stale_flat: OnceCell::new(),
            outcomes: OnceCell::new(),
            firsts: BTreeMap::new(),
            changed: declared.changed().to_vec(),
            created: declared.created().to_vec(),
            removed: declared.removed().to_vec(),
            selected: 0,
            positions: BTreeSet::new(),
            observers: BTreeSet::new(),
            states: [0; 7],
            indeterminate_only: 0,
            total: 0,
            listed: 0,
            groups: BTreeMap::new(),
            not_verified: BTreeMap::new(),
            used,
            any_bad: false,
            all_good: true,
            strict_names: false,
            names: BTreeMap::new(),
        }
    }

    /// Before the first `add`: `--strict-names` (§7.1, WP-112). Each note
    /// `names_changed` of a `verified` or `mismatch` shard is then a
    /// finding of kind `names_changed`, which no exemption prevents: an
    /// observer with one is `mismatch`.
    ///
    /// # Panics
    ///
    /// If called after an `add`: a defect of the caller.
    pub fn strict_names(&mut self) {
        assert!(self.selected == 0, "Report::strict_names after Report::add");
        self.strict_names = true;
    }

    /// Before the first `add`, for a run whose image is an overlay (§9.5):
    /// `add` then expects `Binding.overlay` to be `id` in every shard, and
    /// folds the overlay's obligations (§10.3). The image of such a run has
    /// no root map: the report's `image` is `[]`.
    ///
    /// # Panics
    ///
    /// If called after an `add`, a second time, or with a header whose
    /// `image` is not empty: each is a defect of the caller.
    pub fn overlay(&mut self, id: Digest, overlay: &Overlay) {
        assert!(self.selected == 0, "Report::overlay after Report::add");
        assert!(self.overlay.is_none(), "Report::overlay called twice");
        assert!(self.header.image.is_empty(), "Report::overlay with a tree image in the header");
        self.overlay =
            Some(Box::new(OverlayRun { id, overlay: overlay.clone(), fold: Fold::new(overlay) }));
    }

    /// Before the first `add`: the round of the pre shards (default 0),
    /// which `add` expects as `Binding.pre` (§9.3).
    ///
    /// # Panics
    ///
    /// If called after an `add`: a defect of the caller.
    pub fn pre_round(&mut self, pre: u64) {
        assert!(self.selected == 0, "Report::pre_round after Report::add");
        self.pre = pre;
    }

    /// Whether the run has rules (`Declared.rules` not empty).
    fn has_rules(&self) -> bool {
        !self.namespace.is_empty()
    }

    /// Whether the run's image is an overlay with at least one obligation.
    fn has_obligations(&self) -> bool {
        self.overlay.as_ref().is_some_and(|o| !o.fold.is_empty())
    }

    /// Whether a shard is this observer's result in this run as far as the
    /// shard alone decides it (§7.3, §9.3, §10.4): its observer, round and
    /// verdict's rules; for a shard a worker wrote (status not `failed`),
    /// its `binding` (required unless the run is M1's, a tree image with no
    /// rule; when present, the run's in all three fields); for an `ok`
    /// shard, `rule_use` present exactly when the run has rules, with the
    /// run's `rules_id` and no rule index past the table, and `spellings`
    /// present exactly when the overlay has obligations. Status and section
    /// placement beyond that are `Shard::validate`'s (`judge`).
    ///
    /// `Err` is the reason the shard is not this run's, in the order of
    /// the checks: it is the observer's message in `not_verified` and on
    /// standard error (§7.1; `spec/landed/verify/WP-121.md` §4).
    fn ours(&self, s: &Shard, observer: &Digest) -> Result<(), String> {
        let h = &self.header;
        if s.observer != *observer {
            return Err("the shard is another observer's".to_owned());
        }
        if s.round != h.round {
            return Err(format!("the shard is of round {}, not the run's {}", s.round, h.round));
        }
        if s.verdict.as_ref().is_some_and(|v| v.rules != h.rules) {
            return Err("the shard's verdict was made under other rules than the run's".to_owned());
        }
        if s.outcome.status != Status::Failed {
            let want = Binding {
                overlay: self.overlay.as_ref().map(|o| o.id),
                declared: h.declared,
                pre: self.pre,
            };
            let required = self.overlay.is_some() || self.has_rules();
            match &s.binding {
                Some(b) if *b != want => {
                    let what = if b.overlay != want.overlay {
                        "its overlay".to_owned()
                    } else if b.declared != want.declared {
                        "its declared changes".to_owned()
                    } else {
                        format!("its pre round {}, not {}", b.pre, want.pre)
                    };
                    return Err(format!("the shard's binding is not this run's: {what}"));
                }
                None if required => return Err("the shard has no binding".to_owned()),
                _ => {}
            }
        }
        if s.outcome.status == Status::Ok {
            match &s.rule_use {
                None if self.has_rules() => return Err("the shard has no rule_use".to_owned()),
                Some(_) if !self.has_rules() => {
                    return Err("the shard has a rule_use and the run has no rule".to_owned());
                }
                Some(u) => {
                    let n = self.namespace.len() as u64;
                    if u.rules != h.rules {
                        return Err(
                            "the shard's rule_use is under other rules than the run's".to_owned()
                        );
                    }
                    if let Some(x) = u.uses.iter().find(|x| x.rule >= n) {
                        return Err(format!(
                            "the shard's rule_use names rule {}; the run has {n}",
                            x.rule
                        ));
                    }
                }
                None => {}
            }
            match (&s.spellings, self.has_obligations()) {
                (None, true) => return Err("the shard has no spellings".to_owned()),
                (Some(_), false) => {
                    return Err("the shard has spellings and the run has no obligation".to_owned());
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// [`Report::add_at`] at the next position: the number of `add` calls
    /// so far. For a caller that adds every selected observer in plan
    /// order and selects them all (M1's `slip verify`).
    ///
    /// # Panics
    ///
    /// As [`Report::add_at`].
    pub fn add(
        &mut self,
        observer: &Digest,
        main: &[u8],
        verify: Option<&Shard>,
        map: Option<Status>,
    ) {
        self.add_at(self.selected, observer, main, verify, map);
    }

    /// One call per selected observer, `position` being its index in the
    /// plan, in any order: nothing the report holds depends on the order of
    /// the calls (the stale pairs, `not_verified`, a group's `first` and
    /// shown finding, and the fold's `first` are by plan position).
    ///
    /// `verify` is this run's shard of the observer, as for [`state`]; a
    /// shard whose `observer` is not `observer`, whose `round` is not the
    /// header's, or whose verdict's `rules` is not the header's is not this
    /// observer's result in this run: the observer is `Failed`, and its
    /// message says why (WP-121; never the foreign shard's own message).
    /// Only a `Mismatch` contributes findings, and only a
    /// `Verified` or `Mismatch` verdict contributes used exemptions.
    ///
    /// M2 (§9.3, §10.4): so is a shard whose `binding` is not the run's
    /// (declared, overlay or its absence, pre round; a shard without one,
    /// unless the run has a tree image and no rule), an `ok` shard without
    /// a `rule_use` of the run's `rules_id` in a run with rules (or with
    /// one in a run without, or naming a rule past the table), an `ok`
    /// shard without `spellings` when the overlay has obligations (or with
    /// one when it has none), and one whose `spellings` the fold refuses;
    /// each with its reason as the message. The `rule_use` and `spellings` of an `ok` shard the state table
    /// accepts with a verdict (`verified`, `mismatch`, `unverifiable`) are
    /// read: a stale pair for every `direct` not 0 of a rule whose kind is
    /// not `namespace`; every rule with a use is used. The fold reads the
    /// `spellings` of a `verified` or `mismatch` shard only; that of an
    /// `unverifiable` one is checked (a refusal makes it `failed`) but not
    /// folded (§10.6).
    ///
    /// # Panics
    ///
    /// If `position` is not less than the header's `plan_observers`, or
    /// was added before, or `observer` was: each is a defect of the caller
    /// (§7.3), which would otherwise let one observer stand for another.
    pub fn add_at(
        &mut self,
        position: u64,
        observer: &Digest,
        main: &[u8],
        verify: Option<&Shard>,
        map: Option<Status>,
    ) {
        assert!(
            position < self.header.plan_observers,
            "Report::add_at: position {position} is not in the plan of {}",
            self.header.plan_observers
        );
        // Both checked before either is kept: a refused call changes
        // nothing.
        assert!(
            !self.positions.contains(&position),
            "Report::add_at: position {position} added twice"
        );
        assert!(!self.observers.contains(observer), "Report::add_at: an observer added twice");
        self.positions.insert(position);
        self.observers.insert(*observer);
        let index = position;
        self.selected += 1;
        self.stale_flat = OnceCell::new();
        self.outcomes = OnceCell::new();
        // Why the shard is not this run's, when it is not: the message of
        // the observer (§7.1), never the foreign shard's own.
        let mut refused = verify.and_then(|s| self.ours(s, observer).err());
        let ours = verify.filter(|_| refused.is_none());
        let mut judged = if refused.is_some() {
            Judged { state: State::Failed, verdict: None }
        } else {
            judge(ours, map)
        };
        // The sections of an ok shard the state table accepts. The fold is
        // the last check: it refuses before it changes anything, and its
        // refusal makes the shard no shard of this run (§10.4).
        let accepted = judged.verdict.is_some();
        if let (Some(s), true) = (ours, accepted)
            && let Err(e) = self.read_sections(index, observer, main, s, judged.state)
        {
            judged = Judged { state: State::Failed, verdict: None };
            refused = Some(e);
        }
        // The notes of a shard that was compared (§4.4): read whatever the
        // option; under it, each is a finding.
        let notes: &[NameNote] = match (ours, judged.state) {
            (Some(s), State::Verified | State::Mismatch) => {
                s.names.as_ref().map_or(&[][..], |n| n.notes.as_slice())
            }
            _ => &[],
        };
        for n in notes {
            self.note(index, main, n);
        }
        let strict = if self.strict_names { notes.len() as u64 } else { 0 };
        let state = if strict > 0 { State::Mismatch } else { judged.state };
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
                findings = verdict.total.saturating_add(strict);
                indeterminate = counts[kind_index(FindingKind::Indeterminate)]
                    + counts[kind_index(FindingKind::DerivedIndeterminate)];
                listed = verdict.findings.len() as u64 + strict;
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
                State::NotParsed | State::NeverParsed | State::Failed => refused
                    .take()
                    .or_else(|| ours.map(|s| s.outcome.message.clone()))
                    .unwrap_or_default(),
                State::Mismatch | State::Verified | State::Unchanged => String::new(),
            };
            self.not_verified.insert(
                index,
                NotVerified {
                    observer: *observer,
                    main: main.to_vec(),
                    state,
                    message,
                    findings,
                    indeterminate,
                    listed,
                },
            );
        }
    }

    /// The `spellings` and `rule_use` of an `ok` shard the state table
    /// accepted as `state`, of the observer at plan position `index`; the
    /// reason if the fold refuses the section (the shard is then not this
    /// run's, and nothing of it is read). An unverifiable shard's post-image is
    /// not the run's (§10.6): its section is checked, not folded.
    fn read_sections(
        &mut self,
        index: u64,
        observer: &Digest,
        main: &[u8],
        s: &Shard,
        state: State,
    ) -> Result<(), String> {
        let position = usize::try_from(index).unwrap_or(usize::MAX);
        let fold = matches!(state, State::Verified | State::Mismatch);
        if let (Some(o), Some(section)) = (&mut self.overlay, &s.spellings)
            && !o.fold.is_empty()
        {
            let inputs = s.inputs.as_deref().unwrap_or(&[]);
            if fold {
                match o.fold.add_first(position, inputs, section) {
                    Ok(true) => {
                        self.firsts.insert(position, (*observer, main.to_vec()));
                    }
                    Ok(false) => {}
                    Err(e) => return Err(refused_spellings(&e)),
                }
            } else if let Err(e) = o.fold.check(inputs, section) {
                return Err(refused_spellings(&e));
            }
        }
        if let Some(u) = &s.rule_use {
            for x in &u.uses {
                let r = usize::try_from(x.rule).unwrap_or(usize::MAX);
                if x.via != 0 || x.direct != 0 {
                    self.used_rules[r] = true;
                }
                if x.direct != 0 && !self.namespace[r] {
                    let pair = Stale { rule: x.rule, observer: *observer, direct: x.direct };
                    let at = StaleAt { stale: pair, main: main.to_vec() };
                    if self.stale.insert((x.rule, index), at).is_none() {
                        self.stale_count += 1;
                    }
                }
            }
        }
        Ok(())
    }

    /// Folds one note of the observer at plan position `index` into the
    /// run's notes (each key once).
    fn note(&mut self, index: u64, main: &[u8], n: &NameNote) {
        match self.names.get_mut(&n.key) {
            None => {
                let g = NameGroup {
                    observers: 1,
                    first: main.to_vec(),
                    first_at: index,
                    name: n.name.clone(),
                    file: n.file.clone(),
                };
                self.names.insert(n.key, g);
            }
            Some(g) => {
                g.observers += 1;
                if index < g.first_at {
                    g.first_at = index;
                    main.clone_into(&mut g.first);
                    n.name.clone_into(&mut g.name);
                    n.file.clone_into(&mut g.file);
                }
            }
        }
    }

    /// The run's notes in the report's order: by name, then key.
    fn sorted_names(&self) -> Vec<(&Digest, &NameGroup)> {
        let mut v: Vec<_> = self.names.iter().collect();
        v.sort_by(|a, b| (&a.1.name, a.0).cmp(&(&b.1.name, b.0)));
        v
    }

    /// How many keys were noted `names_changed` in this run (each once,
    /// whatever the number of observers that noted it).
    pub fn names_changed(&self) -> usize {
        self.names.len()
    }

    /// Folds one listed finding of the observer at plan position `index`
    /// into its group.
    fn group(&mut self, index: u64, main: &[u8], f: &Finding) {
        let id = (kind_index(f.kind) as u8, f.key);
        match self.groups.get_mut(&id) {
            None => {
                let g = FindingGroup {
                    observers: 1,
                    last: index,
                    first: main.to_vec(),
                    first_at: index,
                    shown: f.clone(),
                    shown_at: index,
                };
                self.groups.insert(id, g);
            }
            Some(g) => {
                if g.last != index {
                    g.observers += 1;
                    g.last = index;
                }
                if index < g.first_at {
                    g.first_at = index;
                    main.clone_into(&mut g.first);
                }
                // A named finding over an unnamed one, then the lower
                // position; within one observer, the first listed.
                let (named, shown_named) = (!f.name.is_empty(), !g.shown.name.is_empty());
                if (named && !shown_named) || (named == shown_named && index < g.shown_at) {
                    g.shown = f.clone();
                    g.shown_at = index;
                }
            }
        }
    }

    /// The run's verdict over the observers added so far (§7, §9.3):
    /// [`combine`] of the states' verdict, the stale pairs and the
    /// obligations that did not pass.
    pub fn verdict(&self) -> RunVerdict {
        let states = if self.any_bad {
            RunVerdict::NotVerified
        } else if self.selected > 0 && self.all_good {
            RunVerdict::Verified
        } else {
            RunVerdict::Incomplete
        };
        let not_passed = self.spellings().iter().filter(|o| !o.passed()).count();
        combine(states, self.stale_count, not_passed)
    }

    /// Every stale pair (§9.2), sorted by `(rule, plan position)`.
    pub fn stale(&self) -> &[Stale] {
        self.stale_flat.get_or_init(|| self.stale.values().map(|s| s.stale).collect())
    }

    /// The rules no `ok` shard of this run used (§9.2), ascending: indices
    /// of `Declared.rules`. Empty for a run without rules.
    pub fn unused(&self) -> Vec<u64> {
        (0u64..).zip(&self.used_rules).filter(|(_, u)| !**u).map(|(i, _)| i).collect()
    }

    /// The namespace rules some `ok` shard of this run used (`via` or
    /// `direct` not 0), ascending: whether the observers agree on them is
    /// not checked until WP-111 (§9.2, §9.4). Not empty: the report says
    /// so, and conclude permits no write (`spec/edits.md` §8.5); the
    /// verdict is not changed by it.
    pub fn unchecked(&self) -> Vec<u64> {
        (0u64..)
            .zip(self.namespace.iter().zip(&self.used_rules))
            .filter(|(_, (n, u))| **n && **u)
            .map(|(i, _)| i)
            .collect()
    }

    /// The outcome of every obligation of the overlay (§10.3, §10.6), in
    /// its order; empty for a run without an overlay or obligations.
    pub fn spellings(&self) -> &[Outcome] {
        self.outcomes
            .get_or_init(|| self.overlay.as_ref().map_or_else(Vec::new, |o| o.fold.outcomes()))
    }

    /// The header the report was made with.
    pub(crate) fn header(&self) -> &Header {
        &self.header
    }

    /// The overlay given to [`Report::overlay`], if any.
    pub(crate) fn overlay_given(&self) -> Option<&Overlay> {
        self.overlay.as_ref().map(|o| &o.overlay)
    }

    /// The observer at plan position `position` that is the `first` of an
    /// obligation's failure.
    pub(crate) fn first_at(&self, position: usize) -> Option<&(Digest, Vec<u8>)> {
        self.firsts.get(&position)
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
        let mut groups: Vec<Value> = self
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
        groups.extend(self.strict_names_groups());
        let not_verified = self
            .not_verified
            .values()
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
        let mut members = vec![
            ("report", Value::Str(REPORT.to_owned())),
            ("plan", digest(&h.plan)),
            ("round", uv(h.round)),
            ("declared", digest(&h.declared)),
            ("rules", digest(&h.rules)),
            ("image", Value::Array(image)),
        ];
        if let Some(o) = &self.overlay {
            members.push(("overlay", digest(&o.id)));
        }
        members.extend([
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
        ]);
        if !self.names.is_empty() {
            members.push((NAMES_CHANGED, self.names_json()));
        }
        if self.has_rules() {
            members.push(("agreement", self.agreement_json()));
        }
        if self.has_obligations() {
            members.push(("spellings", self.spellings_json()));
        }
        members.push(("run", bytes(&h.run)));
        obj(members)
    }

    /// Under `--strict-names`, each noted key is a finding group of kind
    /// `names_changed`, after every other kind; without it, none.
    fn strict_names_groups(&self) -> Vec<Value> {
        if !self.strict_names {
            return Vec::new();
        }
        self.sorted_names()
            .into_iter()
            .map(|(k, g)| {
                obj(vec![
                    ("kind", Value::Str(NAMES_CHANGED.to_owned())),
                    ("key", digest(k)),
                    ("name", Value::Str(g.name.clone())),
                    ("observers", uv(g.observers)),
                    ("first", bytes(&g.first)),
                    ("file", bytes(&g.file)),
                    ("pre", Value::Null),
                    ("post", Value::Null),
                    ("where", Value::Null),
                ])
            })
            .collect()
    }

    /// §7.1 `names_changed` (WP-112): the count of keys and the first
    /// [`TEXT_MAX`] in the report's order.
    fn names_json(&self) -> Value {
        let listed = self
            .sorted_names()
            .into_iter()
            .take(TEXT_MAX)
            .map(|(k, g)| {
                obj(vec![
                    ("key", digest(k)),
                    ("name", Value::Str(g.name.clone())),
                    ("file", bytes(&g.file)),
                    ("observers", uv(g.observers)),
                    ("first", bytes(&g.first)),
                ])
            })
            .collect();
        obj(vec![("total", uv(self.names.len() as u64)), ("listed", Value::Array(listed))])
    }

    /// §9.4 `agreement`.
    fn agreement_json(&self) -> Value {
        let stale = self
            .stale
            .values()
            .map(|s| {
                obj(vec![
                    ("rule", uv(s.stale.rule)),
                    ("observer", digest(&s.stale.observer)),
                    ("main", bytes(&s.main)),
                    ("direct", uv(s.stale.direct)),
                ])
            })
            .collect();
        obj(vec![
            ("rules", uv(self.namespace.len() as u64)),
            ("stale", Value::Array(stale)),
            ("unused", Value::Array(self.unused().into_iter().map(uv).collect())),
            ("unchecked", Value::Array(self.unchecked().into_iter().map(uv).collect())),
        ])
    }

    /// The failed obligations of §10.5, in the obligations' order:
    /// `(index, why, observers, first's main file, found, name)`.
    fn failed_spellings(&self) -> Vec<FailedAt<'_>> {
        self.spellings()
            .iter()
            .enumerate()
            .filter_map(|(i, o)| match o {
                Outcome::Passed { .. } => None,
                Outcome::Unseen => Some(FailedAt {
                    index: i,
                    why: "unseen",
                    observers: 0,
                    first: None,
                    found: None,
                    name: "",
                }),
                Outcome::DenotesOther { observers, first, found, name } => Some(FailedAt {
                    index: i,
                    why: "denotes_other",
                    observers: *observers,
                    first: Some(self.first_at(*first).map_or_else(
                        || unreachable!("the report records the first observer of every failure"),
                        |(_, m)| m.as_slice(),
                    )),
                    found: *found,
                    name,
                }),
            })
            .collect()
    }

    /// §10.5 `spellings`.
    fn spellings_json(&self) -> Value {
        let Some(o) = &self.overlay else { unreachable!("spellings without an overlay") };
        let outcomes = self.spellings();
        let passed = outcomes.iter().filter(|o| o.passed()).count() as u64;
        let failed = self
            .failed_spellings()
            .into_iter()
            .map(|f| {
                let ob = &o.overlay.spellings[f.index];
                obj(vec![
                    ("file", bytes(o.fold.path_of(f.index))),
                    ("lo", uv(ob.lo)),
                    ("hi", uv(ob.hi)),
                    ("key", digest(&ob.key)),
                    ("why", Value::Str(f.why.to_owned())),
                    ("observers", uv(f.observers)),
                    ("first", f.first.map_or(Value::Null, bytes)),
                    ("found", opt_digest(f.found.as_ref())),
                    ("name", Value::Str(f.name.to_owned())),
                ])
            })
            .collect();
        obj(vec![
            ("obligations", uv(outcomes.len() as u64)),
            ("passed", uv(passed)),
            ("failed", Value::Array(failed)),
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
        if let Some(o) = &self.overlay {
            let _ = writeln!(out, "image overlay {}", o.id);
        } else if h.image.is_empty() {
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
        let strict_names = if self.strict_names { self.sorted_names() } else { Vec::new() };
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
        for (k, g) in strict_names.iter().take(TEXT_MAX.saturating_sub(groups.len())) {
            line(
                &mut out,
                &format!("finding {NAMES_CHANGED} {} observers {}", short(k), g.observers),
                &[&text(&g.name)],
            );
            let _ = writeln!(out, "  first {}", path(&g.first));
        }
        let _ = writeln!(
            out,
            "findings {} total, {} listed, {} group(s)",
            self.total,
            self.listed,
            groups.len() + strict_names.len()
        );
        for (list, key, n) in self.exemptions() {
            let _ = writeln!(out, "exemption {list} {} observers {n}", short(&key));
        }
        self.names_text(&mut out);
        self.agreement_text(&mut out);
        self.spellings_text(&mut out);
        out
    }

    /// The lines of §7.1 `names_changed`, when the run has a note: one per
    /// key, at most [`TEXT_MAX`], then how many more.
    fn names_text(&self, out: &mut String) {
        let names = self.sorted_names();
        for (_, g) in names.iter().take(TEXT_MAX) {
            let _ = writeln!(
                out,
                "note: name-derived string changed: {} @ {}",
                text(&g.name),
                path(&g.file)
            );
        }
        if names.len() > TEXT_MAX {
            let _ = writeln!(out, "note: {} more (see the report)", names.len() - TEXT_MAX);
        }
    }

    /// The lines of §9.4, when the run has rules.
    fn agreement_text(&self, out: &mut String) {
        if self.has_rules() {
            let unused = self.unused();
            let _ = writeln!(
                out,
                "agreement rules {} stale {} unused {}",
                self.namespace.len(),
                self.stale_count,
                unused.len()
            );
            for s in self.stale.values().take(TEXT_MAX) {
                let _ = writeln!(
                    out,
                    "stale rule {} direct {} {}",
                    s.stale.rule,
                    s.stale.direct,
                    path(&s.main)
                );
            }
            if self.stale_count > TEXT_MAX {
                let _ =
                    writeln!(out, "stale: {} more (see the report)", self.stale_count - TEXT_MAX);
            }
            for r in unused {
                let _ = writeln!(out, "unused rule {r}");
            }
            for r in self.unchecked() {
                let _ = writeln!(
                    out,
                    "not checked: agreement between observers for namespace rule {r}"
                );
            }
        }
    }

    /// The lines of §10.5, when the overlay has obligations.
    fn spellings_text(&self, out: &mut String) {
        if let Some(o) = self.overlay.as_ref().filter(|_| self.has_obligations()) {
            let outcomes = self.spellings();
            let passed = outcomes.iter().filter(|o| o.passed()).count();
            let failed = self.failed_spellings();
            let _ = writeln!(
                out,
                "spelling obligations {} passed {passed} failed {}",
                outcomes.len(),
                failed.len()
            );
            for f in failed.iter().take(TEXT_MAX) {
                let ob = &o.overlay.spellings[f.index];
                line(
                    out,
                    &format!(
                        "spelling {} {}:{}-{} {} observers {}",
                        f.why,
                        path(o.fold.path_of(f.index)),
                        ob.lo,
                        ob.hi,
                        short(&ob.key),
                        f.observers
                    ),
                    &[&text(f.name)],
                );
            }
            if failed.len() > TEXT_MAX {
                let _ =
                    writeln!(out, "spelling: {} more (see the report)", failed.len() - TEXT_MAX);
            }
        }
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
        for n in self.not_verified.values().take(TEXT_MAX) {
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

/// The message of a shard whose `spellings` the fold refuses (WP-121):
/// the fold's reason, which names the section itself (`spellings: …`),
/// after one prefix.
fn refused_spellings(e: &str) -> String {
    let e = e.strip_prefix("spellings: ").unwrap_or(e);
    format!("the shard's spellings are refused: {e}")
}

/// A failed obligation as the report shows it (§10.5).
struct FailedAt<'r> {
    index: usize,
    why: &'static str,
    observers: u64,
    /// The main file of the first observer it failed in; `None` for
    /// `unseen`.
    first: Option<&'r [u8]>,
    found: Option<Digest>,
    name: &'r str,
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
