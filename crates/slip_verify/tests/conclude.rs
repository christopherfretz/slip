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

use common::{Rng, Run, SetFacts, d, key, overlay, residue, result_of, rule, section, set, used};
use slip_model::editset::EditSet;
use slip_model::keys::KeyKind;
use slip_model::result::{Category, Residue, RunVerdict, Stale, State, Tally};
use slip_model::verdict::VerdictKind;
use slip_model::{FileRef, Status};
use slip_verify::conclude::{Allow, Facts, conclude, permits, state, tally};
use slip_verify::guards::overlay_inputs;
use slip_verify::run::Report;

/// The report of a run without rules or overlay over two observers whose
/// states give `verdict`.
fn report(verdict: RunVerdict) -> Report {
    let run = Run::new(vec![], None);
    let mut r = run.report();
    let second = match verdict {
        RunVerdict::Verified => run.verified(0xa1),
        RunVerdict::NotVerified => run.mismatch(0xa1),
        RunVerdict::Incomplete => run.unverifiable(0xa1, "r"),
    };
    for s in [run.verified(0xa0), second] {
        r.add(&s.observer, b"/m.cc", Some(&s), Some(Status::Ok));
    }
    assert_eq!(r.verdict(), verdict);
    r
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
    residue: Vec<Residue>,
    selected: u64,
}

fn build(r: &Row) -> Built {
    let set = set(r.set);
    let selected = 2 + u64::from(r.set.unparsed);
    Built { set, report: r.verdict.map(report), residue: residue_of(r.residue), selected }
}

fn facts(b: &Built, allow_unparsed: bool) -> Facts<'_> {
    Facts {
        set: &b.set,
        selected: b.selected,
        report: b.report.as_ref(),
        allow_unparsed,
        residue: &b.residue,
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
        let c = conclude(&f, None);
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
// unparsed`, or a report over another overlay. Each gives `incomplete`,
// no verdict, no state counted, and every obligation `unseen`.
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
    // Made without an overlay, concluded with one.
    let o = overlay();
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
    r.add(&s.observer, b"/m", Some(&s), Some(Status::Ok));
    let b = Built { set: set(SetFacts::default()), report: Some(r), residue: vec![], selected: 1 };
    let f = facts(&b, false);
    assert_eq!(conclude(&f, Some(&overlay())).verdict, Some(RunVerdict::NotVerified));
    let mut o2 = overlay();
    o2.spellings.pop();
    assert_eq!(conclude(&f, Some(&o2)).state, State::Incomplete);
    assert_eq!(conclude(&f, None).state, State::Incomplete);
}

// conclude: an observer added twice gives a stale pair twice; the report
// is then not usable (the result's agreement would not be strictly
// ascending).
#[test]
fn s8_5_duplicate_stale_pair_is_no_report() {
    let run = Run::new(vec![rule(KeyKind::Function, "f", "g")], None);
    let s = run.ok(0xa0, run.verdict(VerdictKind::Verified), &[], &[used(0, 0, 1)]);
    let mut r = run.report();
    r.add(&s.observer, b"/m", Some(&s), Some(Status::Ok));
    let b1 = Built {
        set: set(SetFacts::default()),
        report: Some(r.clone()),
        residue: vec![],
        selected: 1,
    };
    let c = conclude(&facts(&b1, false), None);
    assert_eq!(c.state, State::NotVerified);
    assert_eq!(c.agreement, [Stale { rule: 0, observer: d(0xa0), direct: 1 }]);
    r.add(&s.observer, b"/m", Some(&s), Some(Status::Ok));
    let b2 = Built { set: set(SetFacts::default()), report: Some(r), residue: vec![], selected: 2 };
    let c = conclude(&facts(&b2, false), None);
    assert_eq!((c.state, c.verdict), (State::Incomplete, None));
    assert_eq!(result_of(&c, &b2.set, false, false, &[]).validate(), Ok(()));
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
    r.add(&s.observer, b"/m2", Some(&s), Some(Status::Ok));
    let s = run.ok(0xb1, v, &[], &[used(0, 0, 4)]);
    r.add(&s.observer, b"/m1", Some(&s), Some(Status::Ok));
    let b = Built { set: set(SetFacts::default()), report: Some(r), residue: vec![], selected: 2 };
    let c = conclude(&facts(&b, false), Some(&o));
    assert_eq!(c.state, State::NotVerified);
    assert_eq!(
        c.agreement,
        [
            Stale { rule: 0, observer: d(0xb1), direct: 4 },
            Stale { rule: 0, observer: d(0xb2), direct: 1 }
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
        let c = conclude(&facts(&b, false), None);
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
    let c = conclude(&facts(&b, false), None);
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
    let breaks: [(&str, Break); 21] = [
        ("another set", |_, s| s.basis = d(0x77)),
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
        ("a stale pair", |r, _| r.agreement = vec![Stale { rule: 0, observer: d(1), direct: 1 }]),
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
// failed guard, or an obligation that did not pass.

/// What a random run had that keeps it from being verified.
#[derive(Debug, Default)]
struct Bad {
    finding: bool,
    stale: bool,
    guard: bool,
    state: bool,
    spelling: bool,
}

impl Bad {
    fn any(&self) -> bool {
        self.finding || self.stale || self.guard || self.state || self.spelling
    }
}

fn random_run(g: &mut Rng) -> (Report, u64, Bad, Option<slip_model::Overlay>) {
    let mut rules = Vec::new();
    for (i, name) in ["f", "h", "n"].iter().enumerate() {
        if g.chance(2) {
            let kind = if i == 2 || g.chance(4) { KeyKind::Namespace } else { KeyKind::Function };
            rules.push(rule(kind, name, &format!("{name}2")));
        }
    }
    let ov = g.chance(3).then(overlay).or_else(|| {
        (g.chance(2)).then(|| {
            let mut o = overlay();
            o.spellings.clear();
            o
        })
    });
    let run = Run::new(rules, ov.clone());
    let kinds: Vec<KeyKind> = run.declared.rules().rules().iter().map(|r| r.kind).collect();
    let n_obl = ov.as_ref().map_or(0, |o| o.spellings.len());
    let mut held = vec![false; n_obl];
    let mut failed = vec![false; n_obl];
    let mut bad = Bad::default();
    let mut r = run.report();
    let n = 1 + g.below(5);
    for k in 0..n {
        let obs = 0xa0 + k as u8;
        let main = format!("/m/{k}.cc");
        let pick = g.below(10);
        // 0..=5 a verified verdict, 6 mismatch, 7 a guard failure, 8 not
        // parsed, 9 no shard.
        if pick >= 8 {
            bad.state = true;
            if pick == 8 {
                let s = run.unparsed(obs, Status::Diagnosed, "e");
                r.add(&s.observer, main.as_bytes(), Some(&s), Some(Status::Ok));
            } else {
                r.add(&d(obs), main.as_bytes(), None, Some(Status::Ok));
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
                    &slip_model::Overlay { edits: d(4), files: vec![], spellings: vec![] },
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
        for (i, kind) in kinds.iter().enumerate() {
            match g.below(3) {
                0 => {}
                1 => uses.push(used(i as u64, 1 + g.below(3), 0)),
                _ => {
                    let direct = 1 + g.below(3);
                    uses.push(used(i as u64, g.below(2), direct));
                    if *kind != KeyKind::Namespace {
                        bad.stale = true;
                    }
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
                        failed[i] = true;
                    }
                    _ => {
                        h.push(i as u64);
                        held[i] = true;
                    }
                }
            }
            s.spellings = Some(section(o, &inputs, &h, &f));
        }
        s.inputs = Some(inputs);
        s.validate().expect("a valid random shard");
        r.add(&s.observer, main.as_bytes(), Some(&s), Some(Status::Ok));
    }
    bad.spelling = (0..n_obl).any(|i| failed[i] || !held[i]);
    (r, n, bad, ov)
}

// §8.5, §9.3, §10.3: over random runs, a result permits a write (with
// everything allowed) exactly when the run had no finding, no stale
// observer, no failed guard, every observer verified and every
// obligation passed (and the set is accepted, and `unparsed` is 0 or
// allowed). `SLIP_VERIFY_SEED` replays a run.
#[test]
fn s8_5_property_no_write_from_a_bad_run() {
    let mut g = Rng::from_env(0x5eed_0070);
    let (mut bad_runs, mut good_runs) = (0, 0);
    for round in 0..3000 {
        let (report, n, bad, ov) = random_run(&mut g);
        let sf = SetFacts {
            refused: g.chance(10),
            refusal: g.chance(4),
            failed_runs: g.below(2),
            error_report: g.chance(4),
            unparsed: u8::from(g.chance(4)),
        };
        let allow_unparsed = g.chance(2);
        let b = Built {
            set: set(sf),
            report: Some(report),
            residue: if g.chance(3) { vec![residue(Category::Dependent, 0)] } else { vec![] },
            selected: n + u64::from(sf.unparsed),
        };
        let c = conclude(&facts(&b, allow_unparsed), ov.as_ref());
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
    // Both sides are exercised.
    assert!(bad_runs > 1000 && good_runs > 100, "{bad_runs} bad, {good_runs} good");
}
