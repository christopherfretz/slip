//! The state of a transform's result and what each state permits
//! (`spec/edits.md` §8.5): the conclude step of the verify loop (§8.1).
//! Work package WP-070.
//!
//! Pure and part of the trusted base: [`permits`] is the one bit that
//! decides whether `--write` or `--patch` touches the subject. Every
//! function here is total and fails closed:
//!
//! - [`state`] decides nothing the run report did not: `not_verified` is
//!   the report's verdict, never a judgement of its own, and `verified`
//!   needs the report's `verified`. A report that does not describe the
//!   run of these facts (another plan or round, states that do not add up
//!   to the observers the verify phase selected) is read as no report:
//!   `incomplete`. So is a report of an accepted set that was not made
//!   over the set's own overlay (a tree report compared nothing the set
//!   changed, §8.1), and one with an unchecked namespace rule
//!   (`Report::unchecked`, until WP-111).
//! - [`permits`] reads its answer from the result again, and checks that
//!   the edit set it is given is the result's (by content id): a result
//!   whose fields contradict its state, or a set that is not its own,
//!   permits nothing.

use slip_model::edits::Severity;
use slip_model::editset::{EditSet, SetState};
use slip_model::result::{
    Category, FailedSpelling, Residue, RunVerdict, Spellings, Stale, State, Tally, TransformResult,
    Why,
};
use slip_model::{Overlay, formats};

use crate::run::Report;
use crate::spelling::Outcome;

/// What conclude knows of a run (`spec/edits.md` §8.5).
#[derive(Clone, Copy, Debug)]
pub struct Facts<'a> {
    /// The run's edit set: its state, its refusals, its failures and
    /// reports, its unparsed observers.
    pub set: &'a EditSet,
    /// The observers the run selected (`unparsed` included).
    pub selected: u64,
    /// The verify phase's report; `None`: the phase did not run or did not
    /// finish.
    pub report: Option<&'a Report>,
    /// The run was given `--allow-unparsed`.
    pub allow_unparsed: bool,
    /// The residue of the scan (§8.4).
    pub residue: &'a [Residue],
}

/// `unparsed` as §8.2 counts it: the set's observers without an `ok` map
/// shard.
fn unparsed(set: &EditSet) -> u64 {
    set.unparsed.len() as u64
}

/// The `ok` shards with a failure: one record per joined failure, each
/// counting its observers. A shard has at most one failure (§4.6), so the
/// sum counts each shard once.
fn failed_runs(set: &EditSet) -> u64 {
    set.failures.iter().fold(0u64, |a, f| a.saturating_add(f.observers))
}

/// The joined reports of severity `error` (§6).
fn error_reports(set: &EditSet) -> u64 {
    set.reports.iter().filter(|r| r.report.severity == Severity::Error).count() as u64
}

/// The report, if it describes the run of these facts: the set is
/// accepted (a refused set has no verify phase, §8.1); the report is of
/// the overlay form, over an overlay of this set (`Overlay.edits` is the
/// set's content id: an accepted set is always verified through its
/// overlay, §8.1, and a tree report compared nothing the set changed); it
/// is of the set's plan and round; and its states add up to the observers
/// the verify phase selected, `selected − unparsed` (§8.2, §8.5).
fn report_of<'a>(facts: &Facts<'a>) -> Option<&'a Report> {
    if facts.set.state == SetState::Refused {
        return None;
    }
    let r = facts.report?;
    let o = r.overlay_given()?;
    if formats::content_id(&facts.set.to_bytes()).ok() != Some(o.edits) {
        return None;
    }
    let h = r.header();
    if h.plan != facts.set.plan || h.round != facts.set.round {
        return None;
    }
    let parsed = facts.selected.checked_sub(unparsed(facts.set))?;
    let sum = r.states().iter().try_fold(0u64, |a, &n| a.checked_add(n))?;
    (sum == parsed).then_some(r)
}

/// [`report_of`], and the report was made over `overlay` (which is then
/// present). Its stale pairs are one per `(rule, observer)`:
/// `Report::add_at` refuses an observer added twice.
fn usable<'a>(facts: &Facts<'a>, overlay: Option<&Overlay>) -> Option<&'a Report> {
    let r = report_of(facts)?;
    (overlay.is_some() && r.overlay_given() == overlay).then_some(r)
}

fn tally_with(facts: &Facts<'_>, report: Option<&Report>) -> Tally {
    let s = report.map_or([0; 7], Report::states);
    Tally {
        selected: facts.selected,
        unparsed: unparsed(facts.set),
        failed_runs: failed_runs(facts.set),
        error_reports: error_reports(facts.set),
        verified: s[0],
        unchanged: s[1],
        mismatch: s[2],
        not_parsed: s[3],
        unverifiable: s[4],
        never_parsed: s[5],
        failed: s[6],
    }
}

/// §8.5's tally: the seven states from the report (`Report::states`),
/// `unparsed` as §8.2 counts it, `failed_runs` and `error_reports` from
/// the edit set. For a refused set, and for a run without a report that
/// describes it ([`state`]), 0 in every field but `selected`, `unparsed`,
/// `failed_runs` and `error_reports`.
pub fn tally(facts: &Facts<'_>) -> Tally {
    tally_with(facts, report_of(facts))
}

fn state_with(facts: &Facts<'_>, report: Option<&Report>) -> State {
    let set = facts.set;
    // Row 1.
    if set.state == SetState::Refused {
        return State::Refused;
    }
    // Rows 2 and 3: the report's verdict, or none.
    let Some(report) = report else { return State::Incomplete };
    match report.verdict() {
        RunVerdict::NotVerified => return State::NotVerified,
        RunVerdict::Incomplete => return State::Incomplete,
        RunVerdict::Verified => {}
    }
    if unparsed(set) != 0 && !facts.allow_unparsed {
        return State::Incomplete;
    }
    // Until WP-111: a namespace rule an observer used, whose agreement
    // between observers was not checked, permits no write (§8.5).
    if !report.unchecked().is_empty() {
        return State::Incomplete;
    }
    // Row 4.
    let residue = facts.residue.iter().any(|r| r.category != Category::Mention);
    if residue || !set.refusals.is_empty() || failed_runs(set) != 0 || error_reports(set) != 0 {
        return State::VerifiedWithResidue;
    }
    // Row 5.
    State::Verified
}

/// The first state of §8.5 that applies:
///
/// 1. `Refused`: the edit set is refused.
/// 2. `NotVerified`: the report's verdict (`Report::verdict`) is
///    `NotVerified`.
/// 3. `Incomplete`: it is `Incomplete`; or there is no report, or none
///    that describes this run (another plan or round, not over an overlay
///    of this set, or states that do not add up to `selected −
///    unparsed`); or `unparsed` is not 0 and the run was not given
///    `--allow-unparsed`; or (until WP-111) the report has an unchecked
///    namespace rule.
/// 4. `VerifiedWithResidue`: the residue has an entry that is not a
///    mention; or the set has a refusal; or `failed_runs` or
///    `error_reports` is not 0.
/// 5. `Verified`.
pub fn state(facts: &Facts<'_>) -> State {
    state_with(facts, report_of(facts))
}

/// What conclude decides of a result (§8.5): the caller fills the ids,
/// `partial`, `allow_unparsed`, `residue` and `exempt`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Concluded {
    pub state: State,
    /// The report's verdict; `None` without a report that describes the
    /// run.
    pub verdict: Option<RunVerdict>,
    pub tally: Tally,
    /// The stale pairs, sorted by `(rule, observer)`.
    pub agreement: Vec<Stale>,
    pub unused: Vec<u64>,
    /// `Report::unchecked`: the namespace rules an observer used (until
    /// WP-111).
    pub unchecked: Vec<u64>,
    pub spellings: Spellings,
}

/// The report's stale pairs in the result's order, `(rule, observer)`.
fn agreement(r: &Report) -> Vec<Stale> {
    let mut v = r.stale().to_vec();
    v.sort_by_key(|s| (s.rule, s.observer));
    v
}

/// The spellings of a result (`spec/verify.md` §10.3) from the fold's
/// outcomes.
fn spellings(r: &Report) -> Spellings {
    let outcomes = r.spellings();
    let mut passed = 0u64;
    let mut failed = Vec::new();
    for (i, o) in (0u64..).zip(outcomes) {
        match o {
            Outcome::Passed { .. } => passed += 1,
            Outcome::Unseen => failed.push(unseen(i)),
            Outcome::DenotesOther { observers, first, found, name } => {
                let Some((observer, _)) = r.first_at(*first) else {
                    unreachable!("the report records the first observer of every failure")
                };
                failed.push(FailedSpelling {
                    obligation: i,
                    why: Why::DenotesOther,
                    observers: *observers,
                    first: Some(*observer),
                    found: *found,
                    name: name.clone(),
                });
            }
        }
    }
    Spellings { obligations: outcomes.len() as u64, passed, failed }
}

fn unseen(i: u64) -> FailedSpelling {
    FailedSpelling {
        obligation: i,
        why: Why::Unseen,
        observers: 0,
        first: None,
        found: None,
        name: String::new(),
    }
}

/// The `state`, `verdict`, `tally`, `agreement`, `unused`, `unchecked` and
/// `spellings` of a result: copied from the report and decided by
/// [`state`].
/// `overlay` is the run's overlay (`None` for a refused set, or when apply
/// made none).
///
/// A report that does not describe the run (as for [`state`]), or that was
/// not made over `overlay` (for an accepted set `overlay` must be given),
/// is read as no report: `Incomplete`, no
/// verdict, no state counted, no stale pair or unused rule, and every
/// obligation of `overlay` `unseen` (none checked it). For a refused set:
/// `Refused`, and the spellings of no obligation.
pub fn conclude(facts: &Facts<'_>, overlay: Option<&Overlay>) -> Concluded {
    let report = usable(facts, overlay);
    let state = state_with(facts, report);
    let tally = tally_with(facts, report);
    if let Some(r) = report {
        return Concluded {
            state,
            verdict: Some(r.verdict()),
            tally,
            agreement: agreement(r),
            unused: r.unused(),
            unchecked: r.unchecked(),
            spellings: spellings(r),
        };
    }
    let refused = facts.set.state == SetState::Refused;
    let spellings = match overlay {
        Some(o) if !refused => {
            let n = o.spellings.len() as u64;
            Spellings { obligations: n, passed: 0, failed: (0..n).map(unseen).collect() }
        }
        _ => Spellings::default(),
    };
    Concluded {
        state,
        verdict: None,
        tally,
        agreement: Vec::new(),
        unused: Vec::new(),
        unchecked: Vec::new(),
        spellings,
    }
}

/// What the person running the command permits beyond `verified`
/// (`spec/edits.md` §8.5): `--allow-residue`, `--allow-errors`,
/// `--allow-partial`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Allow {
    pub residue: bool,
    pub errors: bool,
    pub partial: bool,
}

/// Whether `--write` or `--patch` may act on a result (§8.5). `set` is the
/// result's edit set: which reasons a `verified_with_residue` has is read
/// from it and from the result.
///
/// - `Verified`: yes. `VerifiedWithResidue`: when every reason present is
///   allowed: a residue entry that is not a mention, or a refusal in the
///   set, needs `allow.residue`; `failed_runs` or `error_reports` not 0
///   needs `allow.errors`. Every other state: no. A `partial` result: only
///   with `allow.partial`.
///
/// And no, whatever the state, unless the result is consistent with it:
/// the result and the set validate; the set is accepted and is the
/// result's (`result.edits` is its content id, and the result's plan,
/// round and transform are the set's); the verdict is `verified`, with no
/// stale pair, no failed obligation and no unchecked rule; no observer is in a state
/// other than `verified` or `unchanged`, and one is; `unparsed` is 0
/// unless the run was given `--allow-unparsed`; the tally's `unparsed`,
/// `failed_runs` and `error_reports` are the set's; and a `Verified`
/// result has no reason, a `VerifiedWithResidue` at least one.
pub fn permits(result: &TransformResult, set: &EditSet, allow: Allow) -> bool {
    if !matches!(result.state, State::Verified | State::VerifiedWithResidue) {
        return false;
    }
    if result.partial && !allow.partial {
        return false;
    }
    if result.validate().is_err() || set.validate().is_err() || set.state != SetState::Accepted {
        return false;
    }
    if formats::content_id(&set.to_bytes()).ok() != Some(result.edits)
        || result.plan != set.plan
        || result.round != set.round
        || result.transform != set.transform
    {
        return false;
    }
    let t = &result.tally;
    let verified = result.verdict == Some(RunVerdict::Verified)
        && result.agreement.is_empty()
        && result.spellings.failed.is_empty()
        && result.unchecked.is_empty()
        && t.mismatch == 0
        && t.not_parsed == 0
        && t.unverifiable == 0
        && t.never_parsed == 0
        && t.failed == 0
        && (t.verified != 0 || t.unchanged != 0)
        && (t.unparsed == 0 || result.allow_unparsed)
        && t.unparsed == unparsed(set)
        && t.failed_runs == failed_runs(set)
        && t.error_reports == error_reports(set);
    if !verified {
        return false;
    }
    let residue =
        result.residue.iter().any(|r| r.category != Category::Mention) || !set.refusals.is_empty();
    let errors = t.failed_runs != 0 || t.error_reports != 0;
    match result.state {
        State::Verified => !residue && !errors,
        State::VerifiedWithResidue => {
            (residue || errors) && (!residue || allow.residue) && (!errors || allow.errors)
        }
        State::NotVerified | State::Incomplete | State::Refused => false,
    }
}
