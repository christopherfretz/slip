//! `spec/edits.md` §8.5: the state of a result, its tally, and what each
//! state permits; over edit sets and run reports written by hand, every
//! combination of the facts the state reads, and random runs. Work
//! package WP-070; decisions in `spec/landed/verify/WP-070.md` §5. Each
//! test names the section it covers.
//!
//! The question this file answers for the trusted base: no result of a
//! run with a finding, a stale observer, a failed guard, or an obligation
//! that did not pass permits a write, and no result permits one that its
//! own fields or its edit set contradict.

#![allow(
    clippy::many_single_char_names,
    clippy::too_many_lines,
    clippy::struct_excessive_bools,
    clippy::items_after_statements,
    reason = "tables of facts written out: one bool per fact, one row per line"
)]

mod common;

use std::panic::{AssertUnwindSafe, catch_unwind};

use common::{
    Rng, Run, SetFacts, d, key, overlay, overlay_for, residue, result_of, rule, section, set, used,
    used_kept,
};
use slip_model::Overlay;
use slip_model::editset::EditSet;
use slip_model::keys::KeyKind;
use slip_model::result::{Category, Residue, RunVerdict, Stale, State, Tally};
use slip_model::verdict::VerdictKind;
use slip_model::{FileRef, Status};
use slip_verify::conclude::{Allow, Concluded, Facts, Scan, conclude, permits, state, tally};
use slip_verify::guards::overlay_inputs;
use slip_verify::run::Report;

/// The report of a run without rules over two observers whose states give
/// `verdict`, through `set`'s overlay (without obligations), and that
/// overlay.
fn report(verdict: RunVerdict, set: &EditSet) -> (Report, Overlay) {
    let o = overlay_for(set);
    let run = Run::new(vec![], Some(o.clone()));
    let mut r = run.report();
    let second = match verdict {
        RunVerdict::Verified => run.verified(0xa1),
        RunVerdict::NotVerified => run.mismatch(0xa1),
        RunVerdict::Incomplete => run.unverifiable(0xa1, "r"),
    };
    for s in [run.verified(0xa0), second] {
        r.add(&s.observer, b"/m.cc", Some(&s), common::MAP_OK);
    }
    assert_eq!(r.verdict(), verdict);
    (r, o)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Res {
    None,
    Mention,
    Dependent,
}

fn residue_of(r: Res) -> Vec<Residue> {
    match r {
        Res::None => vec![],
        Res::Mention => vec![residue(Category::Mention, 0)],
        Res::Dependent => vec![residue(Category::Mention, 0), residue(Category::Dependent, 4)],
    }
}

/// One row of the facts conclude reads.
#[derive(Clone, Copy, Debug)]
struct Row {
    set: SetFacts,
    allow_unparsed: bool,
    residue: Res,
    verdict: Option<RunVerdict>,
}

fn rows() -> Vec<Row> {
    let b = [false, true];
    let mut v = Vec::new();
    for refused in b {
        for refusal in b {
            for failed in b {
                for error in b {
                    for unparsed in b {
                        for allow_unparsed in b {
                            for residue in [Res::None, Res::Mention, Res::Dependent] {
                                for verdict in [
                                    None,
                                    Some(RunVerdict::Verified),
                                    Some(RunVerdict::NotVerified),
                                    Some(RunVerdict::Incomplete),
                                ] {
                                    let set = SetFacts {
                                        refused,
                                        refusal,
                                        failed_runs: u64::from(failed),
                                        error_report: error,
                                        unparsed: u8::from(unparsed),
                                    };
                                    v.push(Row { set, allow_unparsed, residue, verdict });
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    v
}

/// §8.5's list, written out over a row: the first state that applies.
fn expect(r: &Row) -> State {
    let reasons = r.residue == Res::Dependent
        || r.set.refusal
        || r.set.failed_runs != 0
        || r.set.error_report;
    if r.set.refused {
        State::Refused
    } else if r.verdict == Some(RunVerdict::NotVerified) {
        State::NotVerified
    } else if r.verdict != Some(RunVerdict::Verified) || (r.set.unparsed != 0 && !r.allow_unparsed)
    {
        State::Incomplete
    } else if reasons {
        State::VerifiedWithResidue
    } else {
        State::Verified
    }
}

struct Built {
    set: EditSet,
    report: Option<Report>,
    /// The run's overlay, given to conclude.
    overlay: Option<Overlay>,
    residue: Vec<Residue>,
    selected: u64,
}

fn build(r: &Row) -> Built {
    let set = set(r.set);
    let selected = 2 + u64::from(r.set.unparsed);
    let report = r.verdict.map(|v| report(v, &set).0);
    let overlay = Some(overlay_for(&set));
    Built { set, report, overlay, residue: residue_of(r.residue), selected }
}

/// conclude over `b` with its overlay.
fn concluded(b: &Built, allow_unparsed: bool) -> Concluded {
    conclude(&facts(b, allow_unparsed), b.overlay.as_ref())
}

fn facts(b: &Built, allow_unparsed: bool) -> Facts<'_> {
    Facts {
        set: &b.set,
        selected: b.selected,
        report: b.report.as_ref(),
        allow_unparsed,
        scan: Scan::Done(&b.residue),
    }
}

// §8.5: every combination of the facts the state reads: 2^6 × 3 × 4 = 768
// rows, each against the list written out; the tally of each, and the
// result conclude makes of it passes the reader's checks.
#[test]
fn s8_5_state_exhaustive() {
    let all = rows();
    assert_eq!(all.len(), 768);
    let mut by_state = [0usize; 5];
    for r in &all {
        let b = build(r);
        let f = facts(&b, r.allow_unparsed);
        let want = expect(r);
        assert_eq!(state(&f), want, "{r:?}");
        let c = conclude(&f, b.overlay.as_ref());
        assert_eq!(c.state, want, "{r:?}");
        by_state[want as usize] += 1;
        // The tally.
        let t = tally(&f);
        assert_eq!(c.tally, t);
        assert_eq!(t.selected, b.selected);
        assert_eq!(t.unparsed, u64::from(r.set.unparsed));
        assert_eq!(t.failed_runs, r.set.failed_runs);
        assert_eq!(t.error_reports, u64::from(r.set.error_report));
        let states = [
            t.verified,
            t.unchanged,
            t.mismatch,
            t.not_parsed,
            t.unverifiable,
            t.never_parsed,
            t.failed,
        ];
        let want_states = match (r.set.refused, r.verdict) {
            (true, _) | (_, None) => [0; 7],
            (false, Some(RunVerdict::Verified)) => [2, 0, 0, 0, 0, 0, 0],
            (false, Some(RunVerdict::NotVerified)) => [1, 0, 1, 0, 0, 0, 0],
            (false, Some(RunVerdict::Incomplete)) => [1, 0, 0, 0, 1, 0, 0],
        };
        assert_eq!(states, want_states, "{r:?}");
        let has_verdict = !r.set.refused && r.verdict.is_some();
        assert_eq!(c.verdict, if has_verdict { r.verdict } else { None }, "{r:?}");
        let result = result_of(&c, &b.set, false, r.allow_unparsed, &b.residue);
        assert_eq!(result.validate(), Ok(()), "{r:?}");
    }
    // 384 refused; of the 384 accepted, 96 per verdict.
    assert_eq!(by_state.iter().sum::<usize>(), 768);
    assert_eq!(by_state[State::Refused as usize], 384);
    assert_eq!(by_state[State::NotVerified as usize], 96);
}

fn one(set: SetFacts, allow_unparsed: bool, res: Res, verdict: Option<RunVerdict>) -> State {
    let r = Row { set, allow_unparsed, residue: res, verdict };
    let b = build(&r);
    state(&facts(&b, allow_unparsed))
}

const V: Option<RunVerdict> = Some(RunVerdict::Verified);

// §8.5: each row alone, and each with the row after it (the earlier
// wins).
#[test]
fn s8_5_each_row_alone_and_in_pairs() {
    let clean = SetFacts::default();
    // Row 5.
    assert_eq!(one(clean, false, Res::None, V), State::Verified);
    // Row 4, each reason alone; a mention is no reason.
    assert_eq!(one(clean, false, Res::Mention, V), State::Verified);
    assert_eq!(one(clean, false, Res::Dependent, V), State::VerifiedWithResidue);
    assert_eq!(
        one(SetFacts { refusal: true, ..clean }, false, Res::None, V),
        State::VerifiedWithResidue
    );
    assert_eq!(
        one(SetFacts { failed_runs: 1, ..clean }, false, Res::None, V),
        State::VerifiedWithResidue
    );
    assert_eq!(
        one(SetFacts { error_report: true, ..clean }, false, Res::None, V),
        State::VerifiedWithResidue
    );
    // Row 3: each cause alone, and before row 4.
    assert_eq!(one(clean, false, Res::None, Some(RunVerdict::Incomplete)), State::Incomplete);
    assert_eq!(one(clean, false, Res::None, None), State::Incomplete);
    let unparsed = SetFacts { unparsed: 1, ..clean };
    assert_eq!(one(unparsed, false, Res::None, V), State::Incomplete);
    assert_eq!(one(unparsed, true, Res::None, V), State::Verified);
    assert_eq!(one(unparsed, false, Res::Dependent, V), State::Incomplete);
    // Row 2, before row 3 (and its unparsed).
    assert_eq!(one(clean, false, Res::None, Some(RunVerdict::NotVerified)), State::NotVerified);
    assert_eq!(one(unparsed, false, Res::None, Some(RunVerdict::NotVerified)), State::NotVerified);
    // Row 1, before row 2.
    let refused = SetFacts { refused: true, ..clean };
    assert_eq!(one(refused, false, Res::None, Some(RunVerdict::NotVerified)), State::Refused);
    assert_eq!(one(refused, false, Res::None, None), State::Refused);
}

// §8.5, conclude: a report that does not describe the run is no report:
// another plan, another round, states that do not add up to `selected −
// unparsed`, a report over another overlay, or no overlay given. Each
// gives `incomplete`, no verdict, no state counted, and every obligation
// `unseen`.
#[test]
fn s8_5_report_of_another_run_is_none() {
    let b = build(&Row {
        set: SetFacts::default(),
        allow_unparsed: false,
        residue: Res::None,
        verdict: V,
    });
    let f = facts(&b, false);
    assert_eq!(state(&f), State::Verified);
    assert_eq!(concluded(&b, false).state, State::Verified);
    for selected in [0, 1, 3, u64::MAX] {
        let f = Facts { selected, ..f };
        assert_eq!(state(&f), State::Incomplete, "{selected}");
        assert_eq!(tally(&f).verified, 0);
    }
    let mut other = b.set.clone();
    other.plan = d(0x55);
    assert_eq!(state(&Facts { set: &other, ..f }), State::Incomplete);
    let mut other = b.set.clone();
    other.round += 1;
    assert_eq!(state(&Facts { set: &other, ..f }), State::Incomplete);
    // Made with the set's overlay, concluded with none.
    let c = conclude(&f, None);
    assert_eq!((c.state, c.verdict, c.tally.verified), (State::Incomplete, None, 0));
    assert_eq!(c.spellings.obligations, 0);
    assert_eq!(result_of(&c, &b.set, false, false, &[]).validate(), Ok(()));
    // Made with the set's overlay (no obligations), concluded with one
    // that has them.
    let mut o = overlay_for(&b.set);
    o.spellings = overlay().spellings;
    let c = conclude(&f, Some(&o));
    assert_eq!(c.state, State::Incomplete);
    assert_eq!(c.verdict, None);
    assert_eq!(c.tally.verified, 0);
    assert_eq!(c.spellings.obligations, 3);
    assert_eq!(c.spellings.passed, 0);
    assert!(c.spellings.failed.iter().all(|s| s.why == slip_model::result::Why::Unseen));
    assert_eq!(result_of(&c, &b.set, false, false, &[]).validate(), Ok(()));
    // Made with one overlay, concluded with another.
    let run = Run::new(vec![], Some(overlay()));
    let mut r = run.report();
    let s = run.verified(0xa0);
    r.add(&s.observer, b"/m", Some(&s), common::MAP_OK);
    let b = Built {
        set: set(SetFacts::default()),
        report: Some(r),
        overlay: Some(overlay()),
        residue: vec![],
        selected: 1,
    };
    let f = facts(&b, false);
    assert_eq!(conclude(&f, Some(&overlay())).verdict, Some(RunVerdict::NotVerified));
    let mut o2 = overlay();
    o2.spellings.pop();
    assert_eq!(conclude(&f, Some(&o2)).state, State::Incomplete);
    assert_eq!(conclude(&f, None).state, State::Incomplete);
}

// spec/edits.md §8.4, §8.5 (WP-076 fix round, B1): whether the scan ran
// is a value. A whole report that verifies, with a scan that did not run,
// concludes `incomplete` with no verdict, as a run cut short: never
// `verified` from "no residue found" read out of "not scanned". A scan
// that ran and found nothing, or a request with no scan, is `verified`.
#[test]
fn s8_4_a_scan_that_did_not_run_is_incomplete() {
    let b = build(&Row {
        set: SetFacts::default(),
        allow_unparsed: false,
        residue: Res::None,
        verdict: V,
    });
    let f = facts(&b, false);
    for (scan, want) in [
        (Scan::Done(&[]), State::Verified),
        (Scan::NotApplicable, State::Verified),
        (Scan::NotRun, State::Incomplete),
    ] {
        let f = Facts { scan, ..f };
        assert_eq!(state(&f), want, "{scan:?}");
        let c = conclude(&f, b.overlay.as_ref());
        assert_eq!(c.state, want, "{scan:?}");
        let r = result_of(&c, &b.set, false, false, scan.residue());
        assert_eq!(r.validate(), Ok(()), "{scan:?}");
        assert_eq!(permits(&r, &b.set, ALL_ALLOWED), want == State::Verified, "{scan:?}");
        if scan == Scan::NotRun {
            assert_eq!((c.verdict, c.tally.verified), (None, 0));
        }
    }
    // Under every allowance and with `--allow-unparsed`, still nothing.
    let f = Facts { scan: Scan::NotRun, allow_unparsed: true, ..f };
    assert_eq!(state(&f), State::Incomplete);
}

/// conclude over `report` for `set` (two observers, nothing unparsed)
/// with `overlay`: the state alone, conclude's state, and whether the
/// result permits anything.
fn verifies(set: &EditSet, report: &Report, overlay: Option<&Overlay>) -> (State, State, bool) {
    let f = Facts {
        set,
        selected: 2,
        report: Some(report),
        allow_unparsed: false,
        scan: Scan::Done(&[]),
    };
    let c = conclude(&f, overlay);
    let r = result_of(&c, set, false, false, &[]);
    assert_eq!(r.validate(), Ok(()));
    (state(&f), c.state, permits(&r, set, ALL_ALLOWED))
}

/// A report over `run` of two verified observers.
fn two_verified(run: &Run) -> Report {
    let mut r = run.report();
    for obs in [0xa0, 0xa1] {
        let sh = run.verified(obs);
        r.add(&sh.observer, b"/m.cc", Some(&sh), common::MAP_OK);
    }
    assert_eq!(r.verdict(), RunVerdict::Verified);
    r
}

// §8.1, §8.5 (review F1, F2): an accepted set is verified only through a
// report of the overlay form over its own overlay (`Overlay.edits` its
// content id). A tree report compared nothing the set changed; an overlay
// of another set, or another transform's overlay and report over the
// same plan and round, verify nothing of this one.
#[test]
fn s8_5_accepted_set_needs_a_report_over_its_overlay() {
    let s = set(SetFacts::default());
    let (good, o) = report(RunVerdict::Verified, &s);
    assert_eq!(verifies(&s, &good, Some(&o)), (State::Verified, State::Verified, true));
    let none = (State::Incomplete, State::Incomplete, false);
    // Review P3: a tree report (no rules, so its shards need no binding),
    // concluded with no overlay and with the set's.
    let tree = two_verified(&Run::new(vec![], None));
    assert_eq!(verifies(&s, &tree, None), none);
    assert_eq!(verifies(&s, &tree, Some(&o)), none);
    // Review P2: an overlay whose `edits` is not this set's.
    let foreign = Overlay { edits: d(4), ..o.clone() };
    let r = two_verified(&Run::new(vec![], Some(foreign.clone())));
    assert_eq!(verifies(&s, &r, Some(&foreign)), none);
    assert_eq!(verifies(&s, &r, Some(&o)), none);
    // Another transform over the same plan and round: its own set, its
    // overlay, its report, all verified; this set is not.
    let mut theirs = s.clone();
    theirs.transform = d(0x99);
    theirs.validate().unwrap();
    let (their_report, their_overlay) = report(RunVerdict::Verified, &theirs);
    assert_eq!(
        verifies(&theirs, &their_report, Some(&their_overlay)),
        (State::Verified, State::Verified, true)
    );
    assert_eq!(verifies(&s, &their_report, Some(&their_overlay)), none);
    assert_eq!(verifies(&s, &their_report, Some(&o)), none);
}

// §7.3 `Report::add_at` (review F3): a position added twice, an observer
// added twice, and a position past the plan are defects of the caller:
// each panics (the run stops, nothing is written), and the refused call
// changes nothing. Review P1: observer A added twice and B never, which
// would otherwise add up to `selected`.
#[test]
fn s7_3_add_at_refuses_a_caller_defect() {
    let run = Run::new(vec![rule(KeyKind::Function, "f", "g")], Some(overlay()));
    let a = run.ok(0xa0, run.verdict(VerdictKind::Verified), &[], &[used(0, 0, 1)]);
    let b = run.verified(0xa1);
    let mut r = run.report();
    r.add_at(0, &a.observer, b"/m/a", Some(&a), common::MAP_OK);
    let before = slip_model::json::write_compact(&r.to_json()).unwrap();
    let defects: [(&str, u64, &slip_model::Shard); 4] = [
        ("P1: A again at B's position", 1, &a),
        ("position 0 again", 0, &b),
        ("position past the plan", 10, &b),
        ("position u64::MAX", u64::MAX, &b),
    ];
    for (what, position, s) in defects {
        let mut r2 = r.clone();
        let hit = catch_unwind(AssertUnwindSafe(|| {
            r2.add_at(position, &s.observer, b"/m/x", Some(s), common::MAP_OK);
        }));
        assert!(hit.is_err(), "{what}");
        assert_eq!(slip_model::json::write_compact(&r2.to_json()).unwrap(), before, "{what}");
    }
    // M1's `add` is `add_at` at the next position: it refuses a repeat too.
    let mut r2 = r.clone();
    let hit = catch_unwind(AssertUnwindSafe(|| {
        r2.add(&a.observer, b"/m/a", Some(&a), common::MAP_OK);
    }));
    assert!(hit.is_err());
    // The last position of the plan is accepted.
    r.add_at(9, &b.observer, b"/m/b", Some(&b), common::MAP_OK);
    assert_eq!(r.states()[0], 2);
    assert_eq!(r.stale(), [Stale { rule: 0, observer: d(0xa0), direct: 1, kept_via: 0 }]);
}

/// A mismatch over `run` reading overlay file 0, with one finding named
/// `name`, `uses`, and obligation 1 failed with the key `found`.
fn mismatch_at(
    run: &Run,
    obs: u8,
    name: &str,
    uses: &[slip_model::agreement::Use],
    found: u8,
) -> slip_model::Shard {
    let mut v = run.verdict(VerdictKind::Mismatch);
    v.total = 1;
    v.findings = vec![slip_model::verdict::Finding {
        kind: slip_model::verdict::FindingKind::Changed,
        key: Some(key(1)),
        file: b"/s/a.h".to_vec(),
        pre: Some(d(0xa1)),
        post: Some(d(0xb1)),
        name: name.to_owned(),
        location: None,
    }];
    let mut s = run.ok(obs, v, &[0], uses);
    let i = s.inputs.clone().unwrap();
    s.spellings = Some(section(run.overlay.as_ref().unwrap(), &i, &[0], &[(1, found)]));
    s.validate().unwrap();
    s
}

// §7.3 (review F4): nothing a report holds depends on the order of the
// `add_at` calls. Every permutation of four shards gives the same JSON,
// text and error lines, byte for byte, and the same concluded result.
// The shards exercise all that once followed the call order: two stale
// pairs of one rule, a finding group whose name is only in the second
// observer and another name in the fourth (the lower position's is
// shown), `not_verified`, an obligation failed in two observers with
// different `found`.
#[test]
fn s7_3_report_independent_of_call_order() {
    let s = set(SetFacts::default());
    let run = Run::new(vec![rule(KeyKind::Function, "f", "g")], Some(overlay()));
    let o = run.overlay.clone().unwrap();
    let shards = [
        (0, mismatch_at(&run, 0xc0, "", &[used(0, 0, 1)], 3), &b"/m/c0.cc"[..]),
        (1, mismatch_at(&run, 0xc1, "f", &[used(0, 1, 2)], 4), b"/m/c1.cc"),
        (2, run.unverifiable(0xc2, "post-image: why"), b"/m/c2.cc"),
        (3, mismatch_at(&run, 0xc3, "g", &[], 5), b"/m/c3.cc"),
    ];
    let mut perms = Vec::new();
    for a in 0..4usize {
        for b in 0..4 {
            for c in 0..4 {
                if a != b && a != c && b != c {
                    perms.push([a, b, c, 6 - a - b - c]);
                }
            }
        }
    }
    assert_eq!(perms.len(), 24);
    let mut seen = Vec::new();
    for p in perms {
        let mut r = run.report();
        for i in p {
            let (pos, sh, main) = &shards[i];
            r.add_at(*pos, &sh.observer, main, Some(sh), common::MAP_OK);
        }
        let json = slip_model::json::write_compact(&r.to_json()).unwrap();
        let f = Facts {
            set: &s,
            selected: 4,
            report: Some(&r),
            allow_unparsed: false,
            scan: Scan::Done(&[]),
        };
        let c = conclude(&f, Some(&o));
        assert_eq!(c.state, State::NotVerified);
        let result = result_of(&c, &s, false, false, &[]);
        assert_eq!(result.validate(), Ok(()));
        seen.push((json, r.to_text(), r.error_lines(), result.to_bytes()));
    }
    for w in seen.windows(2) {
        assert_eq!(w[0], w[1]);
    }
    let (json, text, errors, _) = &seen[0];
    let c0 = "c0".repeat(32);
    assert!(json.contains(&format!(r#""stale":[{{"rule":0,"observer":"{c0}""#)), "{json}");
    assert!(json.contains(r#""name":"f","observers":3,"first":"/m/c0.cc""#), "{json}");
    let one = text.find("stale rule 0 direct 1 kept_via 0 /m/c0.cc").unwrap();
    let two = text.find("stale rule 0 direct 2 kept_via 0 /m/c1.cc").unwrap();
    assert!(one < two, "{text}");
    assert!(errors.lines().next().unwrap().contains("/m/c0.cc"), "{errors}");
    let k3 = format!("1003{}", "00".repeat(30));
    assert!(json.contains(&format!(r#""first":"/m/c0.cc","found":"{k3}""#)), "{json}");
}

// conclude copies the report: agreement sorted by (rule, observer),
// unused, spellings with the first observer's digest.
#[test]
fn s8_5_conclude_copies_the_report() {
    let run = Run::new(
        vec![rule(KeyKind::Function, "f", "g"), rule(KeyKind::Function, "h", "i")],
        Some(overlay()),
    );
    let o = overlay();
    let v = run.verdict(VerdictKind::Verified);
    let mut r = run.report();
    let mut s = run.ok(0xb2, v.clone(), &[0], &[used(0, 0, 1)]);
    s.spellings = Some(section(&o, s.inputs.as_ref().unwrap(), &[0], &[(1, 3)]));
    r.add(&s.observer, b"/m2", Some(&s), common::MAP_OK);
    let s = run.ok(0xb1, v, &[], &[used(0, 0, 4)]);
    r.add(&s.observer, b"/m1", Some(&s), common::MAP_OK);
    let b = Built {
        set: set(SetFacts::default()),
        report: Some(r),
        overlay: Some(o.clone()),
        residue: vec![],
        selected: 2,
    };
    let c = conclude(&facts(&b, false), Some(&o));
    assert_eq!(c.state, State::NotVerified);
    assert_eq!(
        c.agreement,
        [
            Stale { rule: 0, observer: d(0xb1), direct: 4, kept_via: 0 },
            Stale { rule: 0, observer: d(0xb2), direct: 1, kept_via: 0 }
        ]
    );
    assert_eq!(c.unused, [1]);
    assert_eq!(c.spellings.obligations, 3);
    assert_eq!(c.spellings.passed, 1);
    assert_eq!(c.spellings.failed.len(), 2);
    let f = &c.spellings.failed[0];
    assert_eq!((f.obligation, f.observers, f.first, f.found), (1, 1, Some(d(0xb2)), Some(key(3))));
    let u = &c.spellings.failed[1];
    assert_eq!((u.obligation, u.observers, u.first, u.found), (2, 0, None, None));
    let result = result_of(&c, &b.set, false, false, &[]);
    assert_eq!(result.validate(), Ok(()));
    assert!(!permits(&result, &b.set, ALL_ALLOWED));
}

const ALL_ALLOWED: Allow = Allow { residue: true, errors: true, partial: true };

const STATES: [State; 5] = [
    State::Verified,
    State::VerifiedWithResidue,
    State::NotVerified,
    State::Incomplete,
    State::Refused,
];

// §8.5 `permits`: five states × the eight combinations of `Allow`'s
// residue and errors × each of the 16 sets of reasons (residue, refusal,
// failed run, error report) × partial on and off × allow.partial on and
// off: 5 × 4 × 16 × 2 × 2 = 1280 rows. The result is the one conclude
// makes of a verified run with those reasons, with its state replaced.
// And the exact set of states that ever permits is {verified,
// verified_with_residue}.
#[test]
fn s8_5_permits_exhaustive() {
    let mut rows = 0;
    let mut permitting = Vec::new();
    for bits in 0u8..16 {
        let (res, refusal, failed, error) =
            (bits & 1 != 0, bits & 2 != 0, bits & 4 != 0, bits & 8 != 0);
        let row = Row {
            set: SetFacts {
                refusal,
                failed_runs: u64::from(failed),
                error_report: error,
                ..SetFacts::default()
            },
            allow_unparsed: false,
            residue: if res { Res::Dependent } else { Res::Mention },
            verdict: V,
        };
        let b = build(&row);
        let c = concluded(&b, false);
        let natural = if bits == 0 { State::Verified } else { State::VerifiedWithResidue };
        assert_eq!(c.state, natural);
        for st in STATES {
            for partial in [false, true] {
                for allow_bits in 0u8..8 {
                    let allow = Allow {
                        residue: allow_bits & 1 != 0,
                        errors: allow_bits & 2 != 0,
                        partial: allow_bits & 4 != 0,
                    };
                    let mut result = result_of(&c, &b.set, partial, false, &b.residue);
                    result.state = st;
                    let needs_residue = res || refusal;
                    let needs_errors = failed || error;
                    let want = st == natural
                        && (!partial || allow.partial)
                        && (!needs_residue || allow.residue)
                        && (!needs_errors || allow.errors);
                    assert_eq!(
                        permits(&result, &b.set, allow),
                        want,
                        "{bits:04b} {st:?} {partial} {allow:?}"
                    );
                    if want {
                        permitting.push(st);
                    }
                    rows += 1;
                }
            }
        }
    }
    assert_eq!(rows, 1280);
    permitting.sort_by_key(|s| *s as u8);
    permitting.dedup();
    assert_eq!(permitting, [State::Verified, State::VerifiedWithResidue]);
}

/// A result conclude makes of a verified run, with its set, that permits.
fn permitted() -> (slip_model::result::TransformResult, EditSet) {
    let b = build(&Row {
        set: SetFacts::default(),
        allow_unparsed: false,
        residue: Res::None,
        verdict: V,
    });
    let c = concluded(&b, false);
    let r = result_of(&c, &b.set, false, false, &[]);
    assert!(permits(&r, &b.set, Allow::default()));
    (r, b.set)
}

// §8.5 `permits`: a result whose fields contradict its state, or whose
// set is not its own, permits nothing, whatever is allowed.
#[test]
fn s8_5_permits_refuses_inconsistent_results() {
    let (good, set0) = permitted();
    type Break = fn(&mut slip_model::result::TransformResult, &mut EditSet);
    let breaks: [(&str, Break); 24] = [
        ("another set", |_, s| s.basis = d(0x77)),
        ("another plan", |r, _| r.plan = d(0x77)),
        ("another round", |r, _| r.round += 1),
        ("another transform", |r, _| r.transform = d(0x77)),
        ("the set refused", |_, s| {
            *s = set(SetFacts { refused: true, ..SetFacts::default() });
        }),
        ("an edits id of another set", |r, _| r.edits = d(0x77)),
        ("a verdict not verified", |r, _| r.verdict = Some(RunVerdict::NotVerified)),
        ("a verdict incomplete", |r, _| r.verdict = Some(RunVerdict::Incomplete)),
        ("no verdict", |r, _| {
            r.verdict = None;
            r.tally = Tally { selected: r.tally.selected, ..Tally::default() };
        }),
        ("a stale pair", |r, _| {
            r.agreement = vec![Stale { rule: 0, observer: d(1), direct: 1, kept_via: 0 }];
        }),
        ("a failed spelling", |r, _| {
            r.spellings = slip_model::result::Spellings {
                obligations: 1,
                passed: 0,
                failed: vec![slip_model::result::FailedSpelling {
                    obligation: 0,
                    why: slip_model::result::Why::Unseen,
                    observers: 0,
                    first: None,
                    found: None,
                    name: String::new(),
                }],
            };
        }),
        ("a mismatch", |r, _| {
            r.tally.verified -= 1;
            r.tally.mismatch += 1;
        }),
        ("not parsed", |r, _| {
            r.tally.verified -= 1;
            r.tally.not_parsed += 1;
        }),
        ("unverifiable", |r, _| {
            r.tally.verified -= 1;
            r.tally.unverifiable += 1;
        }),
        ("never parsed", |r, _| {
            r.tally.verified -= 1;
            r.tally.never_parsed += 1;
        }),
        ("failed", |r, _| {
            r.tally.verified -= 1;
            r.tally.failed += 1;
        }),
        ("no observer verified", |r, _| r.tally = Tally::default()),
        ("unparsed without allow_unparsed", |r, s| {
            *s = set(SetFacts { unparsed: 1, ..SetFacts::default() });
            r.edits = slip_model::formats::content_id(&s.to_bytes()).unwrap();
            r.tally.selected += 1;
            r.tally.unparsed = 1;
        }),
        ("a tally's unparsed not the set's", |r, _| {
            r.allow_unparsed = true;
            r.tally.selected += 1;
            r.tally.unparsed = 1;
        }),
        ("a tally's failed_runs not the set's", |r, _| r.tally.failed_runs = 1),
        ("a tally's error_reports not the set's", |r, _| r.tally.error_reports = 1),
        ("verified with a residue", |r, _| r.residue = vec![residue(Category::Dependent, 0)]),
        ("verified with a refusal", |r, s| {
            *s = set(SetFacts { refusal: true, ..SetFacts::default() });
            r.edits = slip_model::formats::content_id(&s.to_bytes()).unwrap();
        }),
        ("a result the reader refuses", |r, _| r.scheme = 2),
    ];
    for (what, f) in breaks {
        let (mut r, mut s) = (good.clone(), set0.clone());
        f(&mut r, &mut s);
        // A reason is a contradiction only of `verified` (below for
        // `verified_with_residue`).
        let states: &[State] = if what.starts_with("verified with") {
            &[State::Verified]
        } else {
            &[State::Verified, State::VerifiedWithResidue]
        };
        for &st in states {
            r.state = st;
            assert!(!permits(&r, &s, ALL_ALLOWED), "{what} as {st:?}");
        }
    }
    // A failed run and an error report under their own allow.
    for sf in [
        SetFacts { failed_runs: 1, ..SetFacts::default() },
        SetFacts { error_report: true, ..SetFacts::default() },
    ] {
        let mut r = good.clone();
        r.tally.failed_runs = sf.failed_runs;
        r.tally.error_reports = u64::from(sf.error_report);
        let s = set(sf);
        r.edits = slip_model::formats::content_id(&s.to_bytes()).unwrap();
        assert!(!permits(&r, &s, ALL_ALLOWED), "verified with {sf:?}");
        r.state = State::VerifiedWithResidue;
        assert!(permits(&r, &s, Allow { errors: true, ..Allow::default() }));
        assert!(!permits(&r, &s, Allow { residue: true, partial: true, ..Allow::default() }));
    }
    // A residue that is only a mention is no reason: verified permits it.
    let (mut r, s) = (good, set0);
    r.residue = vec![residue(Category::Mention, 0)];
    assert!(permits(&r, &s, Allow::default()));
    r.state = State::VerifiedWithResidue;
    assert!(!permits(&r, &s, ALL_ALLOWED));
}

// §8.5: an observer counted `unchanged` counts as verified for permits.
#[test]
fn s8_5_unchanged_counts() {
    let (mut r, s) = permitted();
    r.tally.unchanged = r.tally.verified;
    r.tally.verified = 0;
    assert!(permits(&r, &s, Allow::default()));
}

// ---------------------------------------------------------------------------
// The property: no write from a run with a finding, a stale observer, a
// failed guard, an obligation that did not pass, a report not over the
// set's own overlay, or a caller defect.

/// What a random run had that keeps it from being verified.
#[derive(Debug, Default)]
struct Bad {
    finding: bool,
    stale: bool,
    guard: bool,
    state: bool,
    spelling: bool,
    /// The report and the overlay are another set's.
    foreign: bool,
    /// A tree report (no overlay) for an accepted set.
    tree: bool,
    /// No observer added.
    empty: bool,
}

impl Bad {
    fn any(&self) -> bool {
        self.finding
            || self.stale
            || self.guard
            || self.state
            || self.spelling
            || self.foreign
            || self.tree
            || self.empty
    }
}

/// A random run over `set`: its report (`None`: the caller added an
/// observer twice, which panics, so the run wrote nothing), the number
/// of observers added, what was bad in it, and its overlay.
fn random_run(g: &mut Rng, set: &EditSet) -> (Option<Report>, u64, Bad, Option<Overlay>) {
    let mut rules = Vec::new();
    for (i, name) in ["f", "h", "n"].iter().enumerate() {
        if g.chance(2) {
            let kind = if i == 2 || g.chance(4) { KeyKind::Namespace } else { KeyKind::Function };
            rules.push(rule(kind, name, &format!("{name}2")));
        }
    }
    let mut bad = Bad::default();
    let own = Overlay { edits: overlay_for(set).edits, ..overlay() };
    let mut ov = match g.below(6) {
        0 | 1 => Some(own),
        2 | 3 => Some(Overlay { spellings: vec![], ..own }),
        _ => None,
    };
    if ov.is_none() {
        bad.tree = true;
    } else if g.chance(8) {
        // Another set's overlay (review P2), and the report over it.
        bad.foreign = true;
        if let Some(o) = ov.as_mut() {
            o.edits = d(4);
        }
    }
    let run = Run::new(rules, ov.clone());
    let kinds: Vec<KeyKind> = run.declared.rules().rules().iter().map(|r| r.kind).collect();
    let n_obl = ov.as_ref().map_or(0, |o| o.spellings.len());
    let mut held = vec![false; n_obl];
    let mut failed = vec![false; n_obl];
    let mut r = run.report();
    let n = g.below(6);
    bad.empty = n == 0;
    // Plan positions in a random order (the calls need not follow the
    // plan, §7.3); sometimes one observer is added a second time.
    let mut order: Vec<u64> = (0..n).collect();
    for i in (1..order.len()).rev() {
        order.swap(i, g.below(i as u64 + 1) as usize);
    }
    let duplicate = n > 0 && g.chance(30);
    for &k in &order {
        let obs = 0xa0 + k as u8;
        let main = format!("/m/{k}.cc");
        let pick = g.below(10);
        // 0..=5 a verified verdict, 6 mismatch, 7 a guard failure, 8 not
        // parsed, 9 no shard.
        if pick >= 8 {
            bad.state = true;
            if pick == 8 {
                let s = run.unparsed(obs, Status::Diagnosed, "e");
                r.add_at(k, &s.observer, main.as_bytes(), Some(&s), common::MAP_OK);
            } else {
                r.add_at(k, &d(obs), main.as_bytes(), None, common::MAP_OK);
            }
            continue;
        }
        let mut s = match pick {
            6 => {
                bad.finding = true;
                run.mismatch(obs)
            }
            _ => run.verified(obs),
        };
        // The files read: through the overlay, or (for a guard failure)
        // at another content.
        let mut inputs: Vec<FileRef> = s.inputs.take().unwrap();
        if let Some(o) = &ov {
            for i in 0..o.files.len() {
                if g.chance(2) {
                    inputs.push(common::post_input(o, i));
                }
            }
        }
        if pick == 7 {
            let pre = vec![common::file("/s/x.cpp", 1, 0x99)];
            let mut post = inputs.clone();
            // Another content at /s/x.cpp: O2 (or O1 at an overlay file).
            let i = g.below(post.len() as u64) as usize;
            post[i].content = d(0x01);
            let guard = match &ov {
                Some(o) => overlay_inputs(&pre, &post, o),
                None => overlay_inputs(
                    &pre,
                    &post,
                    &Overlay { edits: d(4), files: vec![], spellings: vec![] },
                ),
            };
            // Every entry is the overlay's or the pre shard's, so another
            // content fails O1 or O2.
            let e = guard.expect_err("a tampered entry fails a guard");
            bad.guard = true;
            let mut u = run.unverifiable(obs, &e.reason());
            u.inputs = None;
            s = u;
        }
        inputs.sort_by(|a, b| a.path.cmp(&b.path));
        let mut uses = Vec::new();
        // WP-111: a namespace rule is stale by `direct` or `kept_via` like
        // any rule by `direct`; `via` alone is a use.
        for (i, kind) in kinds.iter().enumerate() {
            let namespace = *kind == KeyKind::Namespace;
            match g.below(4) {
                0 => {}
                1 => uses.push(used(i as u64, 1 + g.below(3), 0)),
                2 if namespace => {
                    let kept_via = 1 + g.below(3);
                    uses.push(used_kept(i as u64, g.below(2), g.below(2), kept_via));
                    bad.stale = true;
                }
                _ => {
                    let direct = 1 + g.below(3);
                    uses.push(used(i as u64, g.below(2), direct));
                    bad.stale = true;
                }
            }
        }
        if let Some(u) = s.rule_use.as_mut() {
            u.uses = uses;
        }
        if let Some(o) = ov.as_ref().filter(|o| !o.spellings.is_empty()) {
            let mut h = Vec::new();
            let mut f = Vec::new();
            for (i, ob) in o.spellings.iter().enumerate() {
                let path = &o.files[ob.file as usize].path;
                if !inputs.iter().any(|x| &x.path == path) {
                    continue;
                }
                match g.below(4) {
                    0 => {}
                    1 => {
                        f.push((i as u64, 5));
                        // An unverifiable observer's spellings are not
                        // folded (§10.6); it makes the run bad anyway.
                        failed[i] |= pick != 7;
                    }
                    _ => {
                        h.push(i as u64);
                        held[i] |= pick != 7;
                    }
                }
            }
            s.spellings = Some(section(o, &inputs, &h, &f));
        }
        s.inputs = Some(inputs);
        s.validate().expect("a valid random shard");
        r.add_at(k, &s.observer, main.as_bytes(), Some(&s), common::MAP_OK);
    }
    if duplicate {
        // Review P1: an observer added again, at a free position (or at
        // its own): a defect of the caller, which stops the run.
        let k = g.below(n);
        let s = run.verified(0xa0 + k as u8);
        let at = if g.chance(2) { k } else { n };
        let hit = catch_unwind(AssertUnwindSafe(|| {
            r.add_at(at, &s.observer, b"/m/again.cc", Some(&s), common::MAP_OK);
        }));
        assert!(hit.is_err(), "an observer added twice is refused");
        return (None, n, bad, ov);
    }
    bad.spelling = (0..n_obl).any(|i| failed[i] || !held[i]);
    (Some(r), n, bad, ov)
}

// §8.5, §9.3, §10.3: over random runs, a result permits a write (with
// everything allowed) exactly when the run had no finding, no stale
// observer, no failed guard, every observer
// verified and every obligation passed, was made over the set's own
// overlay, and had an observer (and the set is accepted, and `unparsed`
// is 0 or allowed). A run whose caller added an observer twice stops and
// writes nothing. `SLIP_VERIFY_SEED` replays a run.
#[test]
fn s8_5_property_no_write_from_a_bad_run() {
    let mut g = Rng::from_env(0x5eed_0070);
    let (mut bad_runs, mut good_runs, mut stopped) = (0, 0, 0);
    for round in 0..6000 {
        let sf = SetFacts {
            refused: g.chance(10),
            refusal: g.chance(4),
            failed_runs: g.below(2),
            error_report: g.chance(4),
            unparsed: u8::from(g.chance(4)),
        };
        let edit_set = set(sf);
        let (report, n, bad, ov) = random_run(&mut g, &edit_set);
        let Some(report) = report else {
            stopped += 1;
            continue;
        };
        let allow_unparsed = g.chance(2);
        let b = Built {
            set: edit_set,
            report: Some(report),
            overlay: ov,
            residue: if g.chance(3) { vec![residue(Category::Dependent, 0)] } else { vec![] },
            selected: n + u64::from(sf.unparsed),
        };
        let c = conclude(&facts(&b, allow_unparsed), b.overlay.as_ref());
        let partial = g.chance(3);
        let result = result_of(&c, &b.set, partial, allow_unparsed, &b.residue);
        assert_eq!(result.validate(), Ok(()), "round {round}: {bad:?}");
        let can = !bad.any() && !sf.refused && (sf.unparsed == 0 || allow_unparsed);
        assert_eq!(
            permits(&result, &b.set, ALL_ALLOWED),
            can,
            "round {round}: {bad:?} {sf:?} {:?}",
            c.state
        );
        // `unchanged` counts as `verified` (§8.5): the report never gives
        // it (§5.5 is not implemented, `run::state`), so the result's
        // verified observers are moved to it: the answer is the same.
        let mut moved = result.clone();
        moved.tally.unchanged = moved.tally.verified;
        moved.tally.verified = 0;
        assert_eq!(permits(&moved, &b.set, ALL_ALLOWED), can, "round {round}: unchanged");
        if bad.any() {
            bad_runs += 1;
            assert!(
                matches!(c.state, State::NotVerified | State::Incomplete | State::Refused),
                "round {round}"
            );
            for bits in 0u8..8 {
                let allow =
                    Allow { residue: bits & 1 != 0, errors: bits & 2 != 0, partial: bits & 4 != 0 };
                assert!(!permits(&result, &b.set, allow), "round {round}");
            }
        } else {
            good_runs += 1;
        }
    }
    // Every side is exercised.
    assert!(bad_runs > 1000 && good_runs > 100 && stopped > 20, "{bad_runs} {good_runs} {stopped}");
}

// §8.5 row 4: every category of residue but `mention` is a reason, each
// alone; a mention beside it changes nothing.
#[test]
fn s8_5_each_residue_category() {
    let b = build(&Row {
        set: SetFacts::default(),
        allow_unparsed: false,
        residue: Res::None,
        verdict: V,
    });
    for (cat, want) in [
        (Category::Mention, State::Verified),
        (Category::Dependent, State::VerifiedWithResidue),
        (Category::NotCompiled, State::VerifiedWithResidue),
        (Category::NotPlanned, State::VerifiedWithResidue),
    ] {
        let residue = vec![residue(cat, 3)];
        let f = Facts { scan: Scan::Done(&residue), ..facts(&b, false) };
        assert_eq!(state(&f), want, "{cat:?}");
        let c = conclude(&f, b.overlay.as_ref());
        let r = result_of(&c, &b.set, false, false, &residue);
        assert_eq!(permits(&r, &b.set, Allow::default()), want == State::Verified, "{cat:?}");
        assert!(permits(&r, &b.set, Allow { residue: true, ..Allow::default() }), "{cat:?}");
    }
}

// §8.5 tally: `failed_runs` counts the observers of each failure, not the
// records; `error_reports` counts reports of severity `error` only.
#[test]
fn s8_5_tally_counts_observers_and_error_reports_only() {
    let mut s = set(SetFacts { failed_runs: 3, error_report: true, ..SetFacts::default() });
    let mut review = s.reports[0].clone();
    review.report.severity = slip_model::edits::Severity::Review;
    review.report.message = b"r".to_vec();
    s.reports.insert(0, review);
    s.validate().unwrap();
    let (report1, _) = report(RunVerdict::Verified, &s);
    let f = Facts {
        set: &s,
        selected: 2,
        report: Some(&report1),
        allow_unparsed: false,
        scan: Scan::Done(&[]),
    };
    let t = tally(&f);
    assert_eq!((t.failed_runs, t.error_reports), (3, 1));
    assert_eq!(state(&f), State::VerifiedWithResidue);
    // With only the review report, no reason.
    let mut s2 = set(SetFacts::default());
    s2.reports = vec![s.reports[0].clone()];
    s2.validate().unwrap();
    let (report2, o2) = report(RunVerdict::Verified, &s2);
    let f = Facts { set: &s2, report: Some(&report2), ..f };
    assert_eq!(tally(&f).error_reports, 0);
    assert_eq!(state(&f), State::Verified);
    let c = conclude(&f, Some(&o2));
    assert!(permits(&result_of(&c, &s2, false, false, &[]), &s2, Allow::default()));
}

// §8.5 `permits`: a refused set permits nothing, even with a result that
// names it and claims `verified`.
#[test]
fn s8_5_permits_refuses_a_refused_set() {
    let (mut r, _) = permitted();
    let s = set(SetFacts { refused: true, ..SetFacts::default() });
    r.edits = slip_model::formats::content_id(&s.to_bytes()).unwrap();
    for st in [State::Verified, State::VerifiedWithResidue] {
        r.state = st;
        assert!(!permits(&r, &s, ALL_ALLOWED), "{st:?}");
    }
}

// §9.2, §9.3, §8.5 (WP-111, which removed `unchecked`): a namespace rule
// an observer used through `via` only permits the write; one stale in an
// observer (`direct` or `kept_via` not 0) makes the run not verified and
// permits nothing, whatever is allowed. A shard that is not read says
// nothing either way.
#[test]
fn s9_2_namespace_rule_stale_permits_nothing() {
    let s = set(SetFacts::default());
    let o = overlay_for(&s);
    let run = Run::new(
        vec![rule(KeyKind::Namespace, "n", "m"), rule(KeyKind::Function, "f", "g")],
        Some(o.clone()),
    );
    let ns = (0..2).find(|&i| run.declared.rules().rules()[i].kind == KeyKind::Namespace).unwrap();
    let ns = ns as u64;
    let v = run.verdict(VerdictKind::Verified);
    for (uses, stale) in [
        (vec![], false),
        (vec![used(ns, 1, 0)], false),
        (vec![used(ns, 0, 2)], true),
        (vec![used_kept(ns, 3, 0, 1)], true),
        (vec![used_kept(ns, 3, 1, 1)], true),
    ] {
        let mut r = run.report();
        for obs in [0xa0, 0xa1] {
            let u = if obs == 0xa1 { uses.clone() } else { vec![] };
            let sh = run.ok(obs, v.clone(), &[], &u);
            r.add(&sh.observer, b"/m.cc", Some(&sh), common::MAP_OK);
        }
        // An observer whose shard is not read (another round): its use is
        // not read either.
        let mut r2 = r.clone();
        let mut stray = run.ok(0xa2, v.clone(), &[], &[used(ns, 1, 1)]);
        stray.round += 1;
        r2.add(&stray.observer, b"/m2.cc", Some(&stray), common::MAP_OK);
        assert_eq!(r2.stale().len(), usize::from(stale));
        assert_eq!(r.stale().len(), usize::from(stale));
        assert_eq!(
            r.verdict(),
            if stale { RunVerdict::NotVerified } else { RunVerdict::Verified },
            "{uses:?}"
        );
        let json = slip_model::json::write_compact(&r.to_json()).unwrap();
        assert!(!json.contains("unchecked"), "{json}");
        assert!(!r.to_text().contains("not checked"));
        let f = Facts {
            set: &s,
            selected: 2,
            report: Some(&r),
            allow_unparsed: true,
            scan: Scan::Done(&[]),
        };
        let c = conclude(&f, Some(&o));
        assert_eq!(state(&f), c.state);
        let result = result_of(&c, &s, false, true, &[]);
        assert_eq!(result.validate(), Ok(()));
        assert_eq!(c.state, if stale { State::NotVerified } else { State::Verified });
        for bits in 0u8..8 {
            let allow =
                Allow { residue: bits & 1 != 0, errors: bits & 2 != 0, partial: bits & 4 != 0 };
            assert_eq!(permits(&result, &s, allow), !stale, "{allow:?}");
        }
    }
}
