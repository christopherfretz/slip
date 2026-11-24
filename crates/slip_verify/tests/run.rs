//! `spec/verify.md` §7, §7.1, §7.2 and §7.3 over shards written by hand:
//! the state of an observer for every row of the table of §7 against every
//! status of its map shard, the run's verdict, and the report (groups,
//! `not_verified`, exemptions, `indeterminate_only`), with golden JSON and
//! text in `tests/golden/run/`. Each test names the section it covers.
//!
//! The question this file answers for the trusted base: no input makes the
//! report say `verified` when a selected observer is not.

use std::fs;
use std::path::Path;

use slip_model::declared::Declared;
use slip_model::digests::{Digests, SCHEME, tu_digest};
use slip_model::json::{self, Value};
use slip_model::names::{NameNote, Names, NamesEntry};
use slip_model::verdict::{Finding, FindingKind, Site, Verdict, VerdictKind, Where};
use slip_model::{Counter, Digest, Outcome, Phase, Shard, Status};
use slip_verify::run::{Header, Report, RunVerdict, State, run_verdict, state};

// ---------------------------------------------------------------------------
// Helpers

fn d(n: u8) -> Digest {
    Digest([n; 32])
}

fn key(n: u8) -> Digest {
    let mut b = [0u8; 32];
    b[0] = 0x10;
    b[1] = n;
    Digest(b)
}

const ROUND: u64 = 1;

/// The six counters of §7.2 by kind, and `verify.findings`, sorted.
fn counters(by_kind: [u64; 6]) -> Vec<Counter> {
    let names = [
        "verify.findings.changed",
        "verify.findings.removed",
        "verify.findings.added",
        "verify.findings.indeterminate",
        "verify.findings.derived_changed",
        "verify.findings.derived_indeterminate",
    ];
    let mut v: Vec<Counter> = names
        .iter()
        .zip(by_kind)
        .map(|(n, value)| Counter { name: (*n).to_owned(), value })
        .collect();
    v.push(Counter { name: "verify.findings".to_owned(), value: by_kind.iter().sum() });
    v.sort();
    v
}

fn outcome(status: Status, message: &str) -> Outcome {
    let errors = u64::from(status != Status::Ok);
    Outcome { status, errors, warnings: 0, message: message.to_owned() }
}

fn empty_digests() -> Digests {
    Digests { scheme: SCHEME, tu: tu_digest(&[]), groups: vec![] }
}

/// A verify shard of observer `obs` with status `ok` and `verdict`, with
/// the counters its findings' kinds give when `by_kind` is `None`.
fn ok_shard(obs: u8, verdict: Verdict, by_kind: Option<[u64; 6]>) -> Shard {
    let by_kind = by_kind.unwrap_or_else(|| {
        let mut c = [0u64; 6];
        for f in &verdict.findings {
            c[f.kind as usize] += 1;
        }
        c
    });
    let s = Shard {
        observer: d(obs),
        phase: Phase::Verify,
        round: ROUND,
        outcome: outcome(Status::Ok, ""),
        inputs: Some(vec![]),
        counters: counters(by_kind),
        digests: Some(empty_digests()),
        verdict: Some(verdict),
        edits: None,
        binding: None,
        rule_use: None,
        spellings: None,
        names: Some(Names::default()),
        negatives: None,
        found: None,
    };
    s.validate().expect("a valid shard");
    s
}

/// A verify shard whose parse ended with `status` (not `ok`).
fn unparsed_shard(obs: u8, status: Status, message: &str) -> Shard {
    let failed = status == Status::Failed;
    let s = Shard {
        observer: d(obs),
        phase: Phase::Verify,
        round: ROUND,
        outcome: outcome(status, message),
        inputs: if failed { None } else { Some(vec![]) },
        counters: if failed {
            vec![]
        } else {
            vec![Counter { name: "pp.tokens".to_owned(), value: 3 }]
        },
        digests: None,
        verdict: None,
        edits: None,
        binding: None,
        rule_use: None,
        spellings: None,
        names: None,
        negatives: None,
        found: None,
    };
    s.validate().expect("a valid shard");
    s
}

fn verified_verdict(used: &[u8]) -> Verdict {
    Verdict {
        kind: VerdictKind::Verified,
        pre: d(0x70),
        // The header's rules (`header`): a verdict under other rules is not
        // this run's.
        rules: d(0x03),
        reason: String::new(),
        only_pre: 0,
        only_post: 0,
        used: used.iter().map(|&n| key(n)).collect(),
        total: 0,
        findings: vec![],
    }
}

fn mismatch_verdict(total: u64, findings: Vec<Finding>, used: &[u8]) -> Verdict {
    Verdict { kind: VerdictKind::Mismatch, total, findings, ..verified_verdict(used) }
}

fn unverifiable_verdict(reason: &str) -> Verdict {
    Verdict {
        kind: VerdictKind::Unverifiable,
        reason: reason.to_owned(),
        pre: Digest([0; 32]),
        ..verified_verdict(&[])
    }
}

fn finding(kind: FindingKind, k: u8, file: &str, name: &str) -> Finding {
    Finding {
        kind,
        key: Some(key(k)),
        file: file.as_bytes().to_vec(),
        pre: if kind == FindingKind::Added { None } else { Some(d(0xa0_u8.wrapping_add(k))) },
        post: if kind == FindingKind::Removed { None } else { Some(d(0xb0_u8.wrapping_add(k))) },
        name: name.to_owned(),
        location: None,
    }
}

fn verified(obs: u8) -> Shard {
    ok_shard(obs, verified_verdict(&[]), None)
}

fn mismatch(obs: u8, findings: Vec<Finding>) -> Shard {
    let total = findings.len() as u64;
    ok_shard(obs, mismatch_verdict(total, findings, &[]), None)
}

fn unverifiable(obs: u8) -> Shard {
    ok_shard(obs, unverifiable_verdict("pre-image: no map shard"), None)
}

fn header(image: Vec<(Vec<u8>, Vec<u8>)>, plan_observers: u64) -> Header {
    Header {
        plan: d(0x01),
        round: ROUND,
        declared: d(0x02),
        rules: d(0x03),
        image,
        limit: 64,
        locate: 16,
        plan_observers,
        run: b"/store/runs/0001.run".to_vec(),
    }
}

fn declared(changed: &[u8], created: &[u8], removed: &[u8]) -> Declared {
    let keys = |l: &[u8]| l.iter().map(|&n| key(n)).collect::<Vec<_>>();
    Declared::new(vec![], keys(changed), keys(created), keys(removed)).unwrap()
}

const MAP_STATUSES: [Option<Status>; 5] =
    [None, Some(Status::Ok), Some(Status::Diagnosed), Some(Status::Rejected), Some(Status::Failed)];

fn golden(name: &str, actual: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/run").join(name);
    if std::env::var_os("SLIP_BLESS").is_some() {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, actual).unwrap();
        eprintln!("blessed {}", path.display());
        return;
    }
    let expected = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e}; run with SLIP_BLESS=1 to create", path.display()));
    assert_eq!(actual, expected, "{name} differs from its golden; SLIP_BLESS=1 rewrites it");
}

// ---------------------------------------------------------------------------
// §7: the state table, every row against every map status

// §7 rows `verified`, `mismatch`, `unverifiable`: an `ok` shard with that
// verdict. A `verified` or `mismatch` verdict is believed only when the map
// shard is `ok` (else it compared against a pre shard the store no longer
// has): `failed`. `unverifiable` whatever the map shard is.
#[test]
fn s7_state_of_an_ok_shard_is_its_verdict() {
    let mismatch = mismatch(1, vec![finding(FindingKind::Changed, 1, "/a.h", "f")]);
    for map in MAP_STATUSES {
        let ok = map == Some(Status::Ok);
        let (v, m) =
            if ok { (State::Verified, State::Mismatch) } else { (State::Failed, State::Failed) };
        assert_eq!(state(Some(&verified(1)), map), v, "{map:?}");
        assert_eq!(state(Some(&mismatch), map), m, "{map:?}");
        assert_eq!(state(Some(&unverifiable(1)), map), State::Unverifiable, "{map:?}");
    }
}

// §7 rows `not_parsed` and `never_parsed`: a `diagnosed` or `rejected`
// shard is `not_parsed` exactly when the map shard is `ok`.
#[test]
fn s7_state_of_an_unparsed_shard_depends_on_the_map_shard() {
    for status in [Status::Diagnosed, Status::Rejected] {
        let s = unparsed_shard(1, status, "a.cpp:1:1: error: x");
        for map in MAP_STATUSES {
            let want = if map == Some(Status::Ok) { State::NotParsed } else { State::NeverParsed };
            assert_eq!(state(Some(&s), map), want, "{status:?} {map:?}");
        }
    }
}

// §7 row `failed`: a shard of status `failed`, no shard, an `ok` shard
// without a verdict (and, §7.3 as implemented, a shard of another phase).
#[test]
fn s7_state_failed() {
    let failed = unparsed_shard(1, Status::Failed, "killed");
    let mut no_verdict = verified(1);
    no_verdict.verdict = None;
    no_verdict.validate().unwrap();
    let mut map_phase = verified(1);
    map_phase.phase = Phase::Map;
    map_phase.verdict = None;
    for map in MAP_STATUSES {
        assert_eq!(state(Some(&failed), map), State::Failed, "{map:?}");
        assert_eq!(state(None, map), State::Failed, "{map:?}");
        assert_eq!(state(Some(&no_verdict), map), State::Failed, "{map:?}");
        assert_eq!(state(Some(&map_phase), map), State::Failed, "{map:?}");
    }
}

// §7 "anything else" and §7.2: a shard that breaks an invariant of its
// format is `failed`, not read; so is a verdict without the counters of
// §7.2 by kind, or with counters that disagree with it. None of these can
// be `verified`.
#[test]
fn s7_state_of_a_shard_that_cannot_be_believed_is_failed() {
    let ok = |s: &Shard| state(Some(s), Some(Status::Ok));
    // A verified verdict with a finding: rejected by spec/model.md §8.3 rule 3.
    let mut s = verified(1);
    s.verdict.as_mut().unwrap().total = 1;
    assert!(s.validate().is_err());
    assert_eq!(ok(&s), State::Failed);
    // A verdict in a shard without digests (rule 1).
    let mut s = verified(1);
    s.digests = None;
    s.names = None;
    assert_eq!(ok(&s), State::Failed);
    // A diagnosed shard with a verdict.
    let mut s = verified(1);
    s.outcome = outcome(Status::Diagnosed, "e");
    assert_eq!(ok(&s), State::Failed);
    // Counters missing: for each of the six by kind, removed alone.
    for v in [
        verified_verdict(&[]),
        mismatch_verdict(1, vec![finding(FindingKind::Changed, 1, "/a.h", "")], &[]),
        unverifiable_verdict("x"),
    ] {
        let base = ok_shard(1, v, None);
        for i in 0..base.counters.len() {
            let mut s = base.clone();
            let removed = s.counters.remove(i);
            let want = if removed.name == "verify.findings" {
                state(Some(&base), Some(Status::Ok))
            } else {
                State::Failed
            };
            assert_eq!(ok(&s), want, "without {}", removed.name);
        }
        let mut s = base.clone();
        s.counters.clear();
        assert_eq!(ok(&s), State::Failed);
    }
    // Counters that do not add up to total.
    let v = mismatch_verdict(2, vec![finding(FindingKind::Changed, 1, "/a.h", "")], &[]);
    assert_eq!(ok(&ok_shard(1, v.clone(), Some([1, 0, 0, 0, 0, 0]))), State::Failed);
    assert_eq!(ok(&ok_shard(1, v.clone(), Some([1, 0, 0, 0, 1, 0]))), State::Mismatch);
    // `verify.findings` other than total.
    let mut s = ok_shard(1, v.clone(), Some([1, 0, 0, 0, 1, 0]));
    s.counters.iter_mut().find(|c| c.name == "verify.findings").unwrap().value = 3;
    assert_eq!(ok(&s), State::Failed);
    // More findings of a kind listed than counted.
    assert_eq!(ok(&ok_shard(1, v, Some([0, 0, 0, 2, 0, 0]))), State::Failed);
    // A verified verdict whose counters count a finding.
    let mut s = verified(1);
    s.counters.iter_mut().find(|c| c.name == "verify.findings.changed").unwrap().value = 1;
    assert_eq!(ok(&s), State::Failed);
}

// §7 "This run's shard" (§7.3, `Report::add`): a shard of another observer
// or another round is not this observer's result in this run: `failed`.
#[test]
fn s7_a_shard_of_another_observer_or_round_is_failed() {
    let mut r = Report::new(header(vec![], 3), &Declared::none());
    r.add(&d(9), b"/a.cpp", Some(&verified(1)), Some(Status::Ok));
    let mut other_round = verified(2);
    other_round.round = ROUND + 1;
    r.add(&d(2), b"/b.cpp", Some(&other_round), Some(Status::Ok));
    r.add(&d(3), b"/c.cpp", Some(&verified(3)), Some(Status::Ok));
    assert_eq!(r.states(), [1, 0, 0, 0, 0, 0, 2]);
    assert_eq!(r.verdict(), RunVerdict::Incomplete);
}

// §7.3 `Report::add`: a verdict made under other rules than the header's is
// not this run's result: `failed`, for every verdict kind.
#[test]
fn s7_a_verdict_under_other_rules_is_failed() {
    let mut r = Report::new(header(vec![], 4), &Declared::none());
    let mut other = verified(1);
    other.verdict.as_mut().unwrap().rules = d(0x04);
    other.validate().unwrap();
    r.add(&d(1), b"/a.cpp", Some(&other), Some(Status::Ok));
    let mut other = mismatch(2, vec![finding(FindingKind::Changed, 1, "/a.h", "f")]);
    other.verdict.as_mut().unwrap().rules = d(0x04);
    r.add(&d(2), b"/b.cpp", Some(&other), Some(Status::Ok));
    let mut other = unverifiable(3);
    other.verdict.as_mut().unwrap().rules = d(0x04);
    r.add(&d(3), b"/c.cpp", Some(&other), Some(Status::Ok));
    r.add(&d(4), b"/d.cpp", Some(&verified(4)), Some(Status::Ok));
    assert_eq!(r.states(), [1, 0, 0, 0, 0, 0, 3]);
    assert_eq!(r.verdict(), RunVerdict::Incomplete);
    // Nothing of the foreign shards is listed or counted.
    let j = r.to_json();
    assert_eq!(j.get("findings").unwrap().get("total").unwrap().as_u64(), Some(0));
}

// §7.1 `not_verified` (WP-121 item 4): a shard that is not this
// observer's, round's or rules' result gives the reason the report decides
// as its message, never the foreign shard's own; one per cause.
#[test]
fn s7_1_a_foreign_shard_says_why() {
    let mut r = Report::new(header(vec![], 3), &Declared::none());
    r.add(
        &d(9),
        b"/a.cpp",
        Some(&unparsed_shard(1, Status::Diagnosed, "foreign")),
        Some(Status::Ok),
    );
    let mut other_round = unparsed_shard(2, Status::Failed, "foreign");
    other_round.round = ROUND + 1;
    r.add(&d(2), b"/b.cpp", Some(&other_round), Some(Status::Ok));
    let mut other_rules = unverifiable(3);
    other_rules.verdict.as_mut().unwrap().rules = d(0x04);
    r.add(&d(3), b"/c.cpp", Some(&other_rules), Some(Status::Ok));
    let j = r.to_json();
    let nv = j.get("not_verified").unwrap().as_array().unwrap();
    let messages: Vec<&str> = nv
        .iter()
        .map(|e| {
            assert_eq!(e.get("state").unwrap().as_str(), Some("failed"));
            e.get("message").unwrap().as_str().unwrap()
        })
        .collect();
    let round = format!("the shard is of round {}, not the run's {ROUND}", ROUND + 1);
    assert_eq!(
        messages,
        [
            "the shard is another observer's",
            round.as_str(),
            "the shard's verdict was made under other rules than the run's",
        ]
    );
    assert!(!r.error_lines().contains("foreign"), "{}", r.error_lines());
    assert_eq!(
        r.error_lines(),
        format!(
            "slip: failed: /a.cpp: the shard is another observer's\n\
             slip: failed: /b.cpp: {round}\n\
             slip: failed: /c.cpp: the shard's verdict was made under other rules than the run's\n"
        )
    );
}

// ---------------------------------------------------------------------------
// §7: the run's verdict

/// §7 restated: verified iff at least one and all verified or unchanged;
/// not verified iff some mismatch or `not_parsed`; else incomplete.
fn expected(states: &[State]) -> RunVerdict {
    let bad = states.iter().filter(|s| matches!(s, State::Mismatch | State::NotParsed)).count();
    let good = states.iter().filter(|s| matches!(s, State::Verified | State::Unchanged)).count();
    if bad > 0 {
        RunVerdict::NotVerified
    } else if good == states.len() && good > 0 {
        RunVerdict::Verified
    } else {
        RunVerdict::Incomplete
    }
}

// §7: every state alone, every combination of two, and no observer.
#[test]
fn s7_run_verdict() {
    assert_eq!(run_verdict(&[]), RunVerdict::Incomplete);
    for a in State::ALL {
        assert_eq!(run_verdict(&[a]), expected(&[a]), "{a:?}");
        for b in State::ALL {
            assert_eq!(run_verdict(&[a, b]), expected(&[a, b]), "{a:?} {b:?}");
        }
    }
    assert_eq!(run_verdict(&[State::Verified, State::Unchanged]), RunVerdict::Verified);
    assert_eq!(run_verdict(&[State::Unchanged]), RunVerdict::Verified);
    assert_eq!(run_verdict(&[State::Verified, State::Failed]), RunVerdict::Incomplete);
    assert_eq!(run_verdict(&[State::Unverifiable, State::Mismatch]), RunVerdict::NotVerified);
    assert_eq!(run_verdict(&[State::NeverParsed, State::Verified]), RunVerdict::Incomplete);
    assert_eq!(run_verdict(&[State::NotParsed, State::Verified]), RunVerdict::NotVerified);
    // An empty report: nothing was checked.
    let r = Report::new(header(vec![], 10), &Declared::none());
    assert_eq!(r.verdict(), RunVerdict::Incomplete);
    assert_eq!(r.verdict_line(), "verdict incomplete");
}

// §7.3: `State::ALL` is the table's order, with the report's names.
#[test]
fn s7_3_state_names() {
    let names: Vec<&str> = State::ALL.iter().map(|s| s.name()).collect();
    assert_eq!(
        names,
        [
            "verified",
            "unchanged",
            "mismatch",
            "not_parsed",
            "unverifiable",
            "never_parsed",
            "failed"
        ]
    );
}

/// xorshift64, as `tests/groups.rs`.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

const SEED: u64 = 0x5eed_0055_0000_7e57;

// §7, properties over generated sets of states (a fixed seed): the run is
// `verified` only if every state is `verified` or `unchanged` and there is
// at least one; `not_verified` if any is `mismatch` or `not_parsed`.
#[test]
fn s7_run_verdict_properties() {
    let mut rng = Rng(SEED);
    for _ in 0..20_000 {
        let n = rng.below(9) as usize;
        let states: Vec<State> = (0..n).map(|_| State::ALL[rng.below(7) as usize]).collect();
        let v = run_verdict(&states);
        let all_good = states.iter().all(|s| matches!(s, State::Verified | State::Unchanged));
        let any_bad = states.iter().any(|s| matches!(s, State::Mismatch | State::NotParsed));
        assert_eq!(v == RunVerdict::Verified, all_good && n > 0, "{states:?}");
        assert_eq!(v == RunVerdict::NotVerified, any_bad, "{states:?}");
    }
}

// §7, §7.3, properties of the report over generated observers (a fixed
// seed): each observer is added with a shard of one of many forms; the
// state counts add up to the number of `add` calls; the report's verdict
// is `run_verdict` of the states `state` gives; and it is `verified` only
// when every observer was given an `ok`, valid shard of its own with a
// `verified` verdict and its counters.
#[test]
fn s7_report_properties() {
    let mut rng = Rng(SEED ^ 0xa5a5);
    for case in 0..3000 {
        let n = rng.below(7) as u8;
        let mut r = Report::new(header(vec![], u64::from(n)), &declared(&[1], &[], &[2]));
        let mut states = Vec::new();
        let mut truly_verified = true;
        for obs in 0..n {
            let map = MAP_STATUSES[rng.below(5) as usize];
            let (shard, good) = match rng.below(13) {
                0..=2 => (Some(verified(obs)), map == Some(Status::Ok)),
                3 => (
                    Some(mismatch(obs, vec![finding(FindingKind::Indeterminate, 3, "/a.h", "")])),
                    false,
                ),
                4 => (Some(unverifiable(obs)), false),
                5 => (Some(unparsed_shard(obs, Status::Diagnosed, "e")), false),
                6 => (Some(unparsed_shard(obs, Status::Failed, "")), false),
                7 => (None, false),
                8 => {
                    let mut s = verified(obs);
                    s.counters.clear();
                    (Some(s), false)
                }
                9 => (Some(verified(obs.wrapping_add(1))), false),
                10 => {
                    let mut s = verified(obs);
                    s.round += 1;
                    (Some(s), false)
                }
                11 => {
                    let mut s = verified(obs);
                    s.verdict.as_mut().unwrap().rules = d(0x04);
                    (Some(s), false)
                }
                _ => {
                    let mut s = verified(obs);
                    s.verdict.as_mut().unwrap().total = 1;
                    (Some(s), false)
                }
            };
            truly_verified &= good;
            let own = shard.as_ref().filter(|s| {
                s.observer == d(obs)
                    && s.round == ROUND
                    && s.verdict.as_ref().is_none_or(|v| v.rules == d(0x03))
            });
            states.push(if shard.is_some() && own.is_none() {
                State::Failed
            } else {
                state(own, map)
            });
            r.add(&d(obs), format!("/s/{obs}.cpp").as_bytes(), shard.as_ref(), map);
        }
        assert_eq!(r.states().iter().sum::<u64>(), u64::from(n), "case {case}");
        for s in State::ALL {
            let count = states.iter().filter(|&&x| x == s).count() as u64;
            assert_eq!(r.states()[State::ALL.iter().position(|&x| x == s).unwrap()], count);
        }
        assert_eq!(r.verdict(), run_verdict(&states), "case {case}");
        if r.verdict() == RunVerdict::Verified {
            assert!(truly_verified && n > 0, "case {case}: verified with an observer that is not");
        }
        let json = r.to_json();
        assert_eq!(json.get("verdict").unwrap().as_str().unwrap(), r.verdict_name());
    }
}

trait VerdictName {
    fn verdict_name(&self) -> &'static str;
}

impl VerdictName for Report {
    fn verdict_name(&self) -> &'static str {
        match self.verdict() {
            RunVerdict::Verified => "verified",
            RunVerdict::NotVerified => "not_verified",
            RunVerdict::Incomplete => "incomplete",
        }
    }
}

// ---------------------------------------------------------------------------
// §7.1: the report

// §7, §7.1 `indeterminate_only`: from the counters, never from the listed
// findings: a verdict that lists fewer findings than it has.
#[test]
fn s7_1_indeterminate_only_from_the_counters() {
    let ind = |k| finding(FindingKind::Indeterminate, k, "/a.h", "");
    let mut r = Report::new(header(vec![], 4), &Declared::none());
    // 100 findings, two listed, both indeterminate; the counters say one
    // of the others is derived_changed: not indeterminate only.
    let v = mismatch_verdict(100, vec![ind(1), ind(2)], &[]);
    r.add(&d(1), b"/a.cpp", Some(&ok_shard(1, v, Some([0, 0, 0, 99, 1, 0]))), Some(Status::Ok));
    // 100 findings, one listed; every one indeterminate or
    // derived_indeterminate by the counters.
    let v = mismatch_verdict(100, vec![ind(1)], &[]);
    r.add(&d(2), b"/b.cpp", Some(&ok_shard(2, v, Some([0, 0, 0, 60, 0, 40]))), Some(Status::Ok));
    // Listing a changed finding first, counted as such.
    let v = mismatch_verdict(3, vec![finding(FindingKind::Changed, 4, "/a.h", "")], &[]);
    r.add(&d(3), b"/c.cpp", Some(&ok_shard(3, v, Some([1, 0, 0, 2, 0, 0]))), Some(Status::Ok));
    // A mismatch with no listed finding (limit 0), all indeterminate.
    let v = mismatch_verdict(5, vec![], &[]);
    r.add(&d(4), b"/d.cpp", Some(&ok_shard(4, v, Some([0, 0, 0, 5, 0, 0]))), Some(Status::Ok));
    let j = r.to_json();
    let obs = j.get("observers").unwrap();
    assert_eq!(obs.get("mismatch").unwrap().as_u64(), Some(4));
    assert_eq!(obs.get("indeterminate_only").unwrap().as_u64(), Some(2));
    let nv = j.get("not_verified").unwrap().as_array().unwrap();
    let triple = |i: usize| {
        let e = &nv[i];
        ["findings", "indeterminate", "listed"].map(|k| e.get(k).unwrap().as_u64().unwrap())
    };
    assert_eq!(triple(0), [100, 99, 2]);
    assert_eq!(triple(1), [100, 100, 1]);
    assert_eq!(triple(2), [3, 2, 1]);
    assert_eq!(triple(3), [5, 5, 0]);
    let f = j.get("findings").unwrap();
    assert_eq!(f.get("total").unwrap().as_u64(), Some(208));
    assert_eq!(f.get("listed").unwrap().as_u64(), Some(4));
    assert_eq!(r.verdict(), RunVerdict::NotVerified);
}

// §7.1 groups: one per (kind, key) across observers; `observers` counts
// those that list it; `first` is the first of them in plan order; `file`,
// `pre`, `post`, `name`, `where` from the first whose finding has a name;
// sorted by (rank, name, key), the rank of §6.
#[test]
#[allow(clippy::many_single_char_names, reason = "the short helpers of one table")]
fn s7_1_groups_across_observers_and_their_order() {
    use FindingKind::*;
    let mut r = Report::new(header(vec![], 3), &Declared::none());
    let named_at = |kind, k, file: &str, name: &str| Finding {
        location: Some(Where {
            pre: Site {
                file: file.as_bytes().to_vec(),
                line: 3,
                column: 7,
                node: "CallExpr".into(),
                what: "refers to a::f(int)".into(),
            },
            post: Site {
                file: file.as_bytes().to_vec(),
                line: 3,
                column: 7,
                node: "CallExpr".into(),
                what: "refers to b::f(long)".into(),
            },
        }),
        ..finding(kind, k, file, name)
    };
    // Observer 1: key 5 changed, unnamed; key 6 indeterminate; key 7 derived_changed.
    r.add(
        &d(1),
        b"/s/one.cpp",
        Some(&mismatch(
            1,
            vec![
                finding(Changed, 5, "/s/x.h", ""),
                finding(DerivedChanged, 7, "/s/y.h", "S<int>"),
                finding(Indeterminate, 6, "/s/x.h", "g"),
            ],
        )),
        Some(Status::Ok),
    );
    // Observer 2 verified: lists nothing.
    r.add(&d(2), b"/s/two.cpp", Some(&verified(2)), Some(Status::Ok));
    // Observer 3: key 5 changed and named here; key 8 changed named "a".
    r.add(
        &d(3),
        b"/s/three.cpp",
        Some(&mismatch(
            3,
            vec![named_at(Changed, 5, "/s/z.h", "ns::f(int)"), finding(Changed, 8, "/s/z.h", "a")],
        )),
        Some(Status::Ok),
    );
    let j = r.to_json();
    let groups = j.get("findings").unwrap().get("groups").unwrap().as_array().unwrap();
    let row = |g: &Value| {
        (
            g.get("kind").unwrap().as_str().unwrap().to_owned(),
            g.get("name").unwrap().as_str().unwrap().to_owned(),
            g.get("observers").unwrap().as_u64().unwrap(),
            g.get("first").unwrap().as_str().unwrap().to_owned(),
            g.get("file").unwrap().as_str().unwrap().to_owned(),
        )
    };
    let rows: Vec<_> = groups.iter().map(row).collect();
    let s = |x: &str| x.to_owned();
    assert_eq!(
        rows,
        vec![
            // rank 0, by name: "a" before "ns::f(int)".
            (s("changed"), s("a"), 1, s("/s/three.cpp"), s("/s/z.h")),
            (s("changed"), s("ns::f(int)"), 2, s("/s/one.cpp"), s("/s/z.h")),
            // rank 3 before rank 4.
            (s("derived_changed"), s("S<int>"), 1, s("/s/one.cpp"), s("/s/y.h")),
            (s("indeterminate"), s("g"), 1, s("/s/one.cpp"), s("/s/x.h")),
        ]
    );
    // The named finding's fields, its `where` included.
    let g = &groups[1];
    assert_eq!(g.get("key").unwrap().as_str().unwrap(), key(5).to_string());
    assert_eq!(g.get("pre").unwrap().as_str().unwrap(), d(0xa5).to_string());
    let w = g.get("where").unwrap();
    assert_eq!(w.get("pre").unwrap().get("line").unwrap().as_u64(), Some(3));
    assert_eq!(groups[0].get("where"), Some(&Value::Null));
    // Ties of rank and name are broken by key.
    let mut r = Report::new(header(vec![], 1), &Declared::none());
    r.add(
        &d(1),
        b"/m.cpp",
        Some(&mismatch(
            1,
            vec![finding(Removed, 9, "/a.h", "x"), finding(Removed, 2, "/b.h", "x")],
        )),
        Some(Status::Ok),
    );
    let j = r.to_json();
    let keys: Vec<String> = j
        .get("findings")
        .unwrap()
        .get("groups")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g.get("key").unwrap().as_str().unwrap().to_owned())
        .collect();
    assert_eq!(keys, vec![key(2).to_string(), key(9).to_string()]);
}

// §7.1 `not_verified`: every observer that is neither verified nor
// unchanged, in plan order, with its state and message: the reason of an
// unverifiable verdict, the shard's message, or empty for a mismatch or
// a missing shard.
#[test]
fn s7_1_not_verified() {
    let mut r = Report::new(header(vec![], 7), &Declared::none());
    r.add(&d(1), b"/1.cpp", Some(&verified(1)), Some(Status::Ok));
    r.add(&d(2), b"/2.cpp", Some(&unverifiable(2)), Some(Status::Ok));
    r.add(
        &d(3),
        b"/3.cpp",
        Some(&unparsed_shard(3, Status::Diagnosed, "3.cpp:1:1: error: x\nmore")),
        Some(Status::Ok),
    );
    r.add(&d(4), b"/4.cpp", Some(&unparsed_shard(4, Status::Rejected, "bad flag")), None);
    r.add(&d(5), b"/5.cpp", Some(&unparsed_shard(5, Status::Failed, "killed")), Some(Status::Ok));
    r.add(&d(6), b"/6.cpp", None, Some(Status::Ok));
    r.add(
        &d(7),
        b"/7.cpp",
        Some(&mismatch(7, vec![finding(FindingKind::Added, 1, "/a.h", "")])),
        Some(Status::Ok),
    );
    let j = r.to_json();
    let nv: Vec<(String, String, String)> = j
        .get("not_verified")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e.get("main").unwrap().as_str().unwrap().to_owned(),
                e.get("state").unwrap().as_str().unwrap().to_owned(),
                e.get("message").unwrap().as_str().unwrap().to_owned(),
            )
        })
        .collect();
    let t = |a: &str, b: &str, c: &str| (a.to_owned(), b.to_owned(), c.to_owned());
    assert_eq!(
        nv,
        vec![
            t("/2.cpp", "unverifiable", "pre-image: no map shard"),
            t("/3.cpp", "not_parsed", "3.cpp:1:1: error: x\nmore"),
            t("/4.cpp", "never_parsed", "bad flag"),
            t("/5.cpp", "failed", "killed"),
            t("/6.cpp", "failed", ""),
            t("/7.cpp", "mismatch", ""),
        ]
    );
    assert_eq!(r.states(), [1, 0, 1, 1, 1, 1, 2]);
    assert_eq!(r.verdict(), RunVerdict::NotVerified);
    assert_eq!(
        r.error_lines(),
        "slip: unverifiable: /2.cpp: pre-image: no map shard\n\
         slip: not_parsed: /3.cpp: 3.cpp:1:1: error: x\n\
         slip: never_parsed: /4.cpp: bad flag\n\
         slip: failed: /5.cpp: killed\n\
         slip: failed: /6.cpp: \n\
         slip: mismatch: /7.cpp: 1 finding(s), 0 indeterminate\n"
    );
}

// §7.1: at most 100 lines on standard error, then `and <n> more`.
#[test]
fn s7_1_error_lines_are_cut_at_100() {
    let mut r = Report::new(header(vec![], 130), &Declared::none());
    for i in 0..130u8 {
        r.add(&d(i), b"/x.cpp", None, None);
    }
    let lines = r.error_lines();
    assert_eq!(lines.lines().count(), 101);
    assert_eq!(lines.lines().last(), Some("slip: and 30 more (see the report)"));
}

// §7.1 exemptions: every key of every list, in the order changed,
// created, removed and then by key, with the observers whose verdict has
// it in `used`: some, none, and one key in two lists (the same number in
// both). An exemption of a shard that is not believed is not counted.
#[test]
#[allow(clippy::many_single_char_names, reason = "the short helpers of one table")]
fn s7_1_exemptions() {
    let decl = declared(&[1, 2], &[1, 3], &[4]);
    let mut r = Report::new(header(vec![], 4), &decl);
    r.add(&d(1), b"/1.cpp", Some(&ok_shard(1, verified_verdict(&[1, 3]), None)), Some(Status::Ok));
    let v = mismatch_verdict(1, vec![finding(FindingKind::Changed, 9, "/a.h", "")], &[1]);
    r.add(&d(2), b"/2.cpp", Some(&ok_shard(2, v, None)), Some(Status::Ok));
    // Not believed: counters removed, so `failed`; its `used` is not counted.
    let mut s = ok_shard(3, verified_verdict(&[1, 2, 3, 4]), None);
    s.counters.clear();
    r.add(&d(3), b"/3.cpp", Some(&s), Some(Status::Ok));
    // A key no list has is ignored.
    r.add(&d(4), b"/4.cpp", Some(&ok_shard(4, verified_verdict(&[7]), None)), Some(Status::Ok));
    let j = r.to_json();
    let ex: Vec<(String, String, u64)> = j
        .get("exemptions")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e.get("list").unwrap().as_str().unwrap().to_owned(),
                e.get("key").unwrap().as_str().unwrap().to_owned(),
                e.get("observers").unwrap().as_u64().unwrap(),
            )
        })
        .collect();
    let e = |l: &str, k: u8, n| (l.to_owned(), key(k).to_string(), n);
    assert_eq!(
        ex,
        vec![
            e("changed", 1, 2),
            e("changed", 2, 0),
            e("created", 1, 2),
            e("created", 3, 1),
            e("removed", 4, 0),
        ]
    );
}

// ---------------------------------------------------------------------------
// §7.1 golden JSON and text

/// A report over five observers of a plan of six, with a two-root image
/// (one root not UTF-8), every kind of finding group, a group seen by two
/// observers, a `where`, exemptions used and unused.
fn sample() -> Report {
    use FindingKind::*;
    let image = vec![
        (b"/src/app".to_vec(), b"/tmp/post/app".to_vec()),
        (b"/src/\xfflib".to_vec(), b"/tmp/post/lib".to_vec()),
    ];
    let mut r = Report::new(header(image, 6), &declared(&[0x21], &[0x22], &[0x21, 0x23]));
    let call = |what: &str| Site {
        file: b"/src/app/main.cpp".to_vec(),
        line: 12,
        column: 5,
        node: "CallExpr".to_owned(),
        what: what.to_owned(),
    };
    let changed = Finding {
        location: Some(Where {
            pre: call("refers to lib::f(long)"),
            post: call("refers to app::f(int)"),
        }),
        ..finding(Changed, 1, "/src/app/main.cpp", "app::run()")
    };
    let added = Finding {
        location: Some(Where {
            pre: Site::default(),
            post: Site { what: String::new(), ..call("") },
        }),
        ..finding(Added, 2, "/src/app/new.h", "app::fresh")
    };
    let v = mismatch_verdict(
        4,
        vec![changed.clone(), added, finding(DerivedChanged, 3, "/src/app/s.h", "app::S<int>")],
        &[0x21],
    );
    r.add(
        &d(1),
        b"/src/app/main.cpp",
        Some(&ok_shard(1, v, Some([1, 0, 1, 1, 1, 0]))),
        Some(Status::Ok),
    );
    let v = mismatch_verdict(
        2,
        vec![
            Finding { name: String::new(), location: None, ..changed },
            finding(Indeterminate, 4, "/src/app/b.h", "app::g()"),
        ],
        &[],
    );
    r.add(
        &d(2),
        b"/src/app/other.cpp",
        Some(&ok_shard(2, v, Some([1, 0, 0, 1, 0, 0]))),
        Some(Status::Ok),
    );
    r.add(
        &d(3),
        b"/src/app/ok.cpp",
        Some(&ok_shard(3, verified_verdict(&[0x21, 0x22]), None)),
        Some(Status::Ok),
    );
    r.add(
        &d(4),
        b"/src/app/bad.cpp",
        Some(&unparsed_shard(4, Status::Diagnosed, "bad.cpp:3:1: error: expected ';'")),
        Some(Status::Ok),
    );
    r.add(&d(5), b"/src/app/odd.cpp", Some(&unverifiable(5)), Some(Status::Ok));
    r
}

#[test]
fn s7_1_golden_json_and_text() {
    let r = sample();
    let text = json::write_pretty(&r.to_json()).unwrap();
    golden("report.json", &text);
    // The document reads back as JSON.
    assert_eq!(json::parse(text.as_bytes()).unwrap(), r.to_json());
    let mut text = r.to_text();
    text.push_str(r.verdict_line());
    text.push('\n');
    golden("report.txt", &text);
    golden("report.err", &r.error_lines());
    assert_eq!(r.verdict_line(), "verdict not verified");
}

// §7.1 text: the identity image; a run with no observer; a key without
// findings; the cut at 100 groups.
#[test]
fn s7_1_text_identity_and_cut() {
    let r = Report::new(header(vec![], 0), &Declared::none());
    let t = r.to_text();
    assert!(t.contains("\nimage identity\n"), "{t}");
    assert!(t.contains("\nselected 0 of 0\n"), "{t}");
    assert!(t.ends_with("findings 0 total, 0 listed, 0 group(s)\n"), "{t}");
    let mut r = Report::new(header(vec![], 1), &Declared::none());
    let findings: Vec<Finding> =
        (0..120).map(|k| finding(FindingKind::Removed, k, "/a.h", "")).collect();
    r.add(&d(1), b"/a.cpp", Some(&mismatch(1, findings)), Some(Status::Ok));
    let t = r.to_text();
    assert_eq!(t.lines().filter(|l| l.starts_with("finding ")).count(), 100);
    assert!(t.contains("findings 120 total, 120 listed, 120 group(s)\n"), "{t}");
}

// §7.1 text: no path or string from a shard can write a line or a control
// sequence: bytes below 0x20, 0x7f, C1 controls, the backslash and bytes
// that are not UTF-8 are `\xNN`; JSON keeps the strings as they are.
#[test]
fn s7_1_text_escapes_paths_and_strings() {
    let nasty = "/x\nverdict verified\r\x1b[2J\\y\u{9b}";
    let escaped = "/x\\x0averdict verified\\x0d\\x1b[2J\\x5cy\\xc2\\x9b";
    let image = vec![(nasty.as_bytes().to_vec(), b"/p\xff\n".to_vec())];
    let mut r = Report::new(header(image, 3), &Declared::none());
    let site = Site {
        file: nasty.as_bytes().to_vec(),
        line: 1,
        column: 2,
        node: nasty.into(),
        what: nasty.into(),
    };
    let f = Finding {
        location: Some(Where { pre: site.clone(), post: site }),
        ..finding(FindingKind::Changed, 1, nasty, nasty)
    };
    r.add(&d(1), nasty.as_bytes(), Some(&mismatch(1, vec![f])), Some(Status::Ok));
    r.add(
        &d(2),
        nasty.as_bytes(),
        Some(&unparsed_shard(2, Status::Diagnosed, nasty)),
        Some(Status::Ok),
    );
    let mut u = unverifiable(3);
    u.verdict.as_mut().unwrap().reason = nasty.to_owned();
    r.add(&d(3), nasty.as_bytes(), Some(&u), Some(Status::Ok));
    let mut text = r.to_text();
    text.push_str(r.verdict_line());
    text.push('\n');
    let errors = r.error_lines();
    for t in [&text, &errors] {
        assert!(!t.lines().any(|l| l == "verdict verified"), "{t}");
        assert!(!t.contains('\r') && !t.contains('\x1b') && !t.contains('\u{9b}'), "{t}");
    }
    assert!(text.contains(&format!("image {escaped} => /p\\xff\\x0a\n")), "{text}");
    assert!(text.contains(&format!("observers 1 {escaped}\n")), "{text}");
    assert!(text.contains(&format!("  first {escaped}\n")), "{text}");
    assert!(text.contains(&format!("  pre  {escaped}:1:2 {escaped} {escaped}\n")), "{text}");
    assert_eq!(text.lines().last(), Some("verdict not verified"));
    // The first line of a message (it ends at the newline), escaped.
    assert_eq!(
        errors,
        "slip: mismatch: ".to_owned()
            + escaped
            + ": 1 finding(s), 0 indeterminate\n"
            + "slip: not_parsed: "
            + escaped
            + ": /x\n"
            + "slip: unverifiable: "
            + escaped
            + ": /x\n"
    );
    // JSON: the strings as they are.
    let j = r.to_json();
    let nv = j.get("not_verified").unwrap().as_array().unwrap();
    assert_eq!(nv[1].get("message").unwrap().as_str(), Some(nasty));
    assert_eq!(nv[0].get("main").unwrap().as_str(), Some(nasty));
}

// ---------------------------------------------------------------------------
// §7.1 `names_changed` and `--strict-names` (WP-112)

/// `s` with notes on `keys` (each with a names digest), named `n<k>` in
/// `/h<k>.h`.
fn with_notes(mut s: Shard, keys: &[u8]) -> Shard {
    let mut keys = keys.to_vec();
    keys.sort_unstable();
    s.names = Some(Names {
        units: keys.iter().map(|&k| NamesEntry { key: key(k), names: d(k) }).collect(),
        notes: keys
            .iter()
            .map(|&k| NameNote {
                key: key(k),
                file: format!("/h{k}.h").into_bytes(),
                name: format!("n{k:03}"),
            })
            .collect(),
    });
    s.validate().expect("a valid shard");
    s
}

/// The index of `s` in `State::ALL`, the order of `Report::states`.
fn at(s: State) -> usize {
    State::ALL.iter().position(|&x| x == s).unwrap()
}

fn names_member(r: &Report) -> Option<Value> {
    r.to_json().get("names_changed").cloned()
}

// §7.1: without the option a note changes no state, no verdict, no count
// and no finding; the report lists it once per key however many observers
// noted it (ten here), by the first observer in plan order.
#[test]
fn s7_1_names_changed_counted_once_over_observers() {
    let mut r = Report::new(header(vec![], 10), &Declared::none());
    for o in 0..10u8 {
        let main = format!("/m{o}.cpp");
        r.add(&d(o), main.as_bytes(), Some(&with_notes(verified(o), &[1])), Some(Status::Ok));
    }
    assert_eq!(r.verdict(), RunVerdict::Verified);
    assert_eq!(r.states()[at(State::Verified)], 10);
    assert_eq!(r.names_changed(), 1);
    let m = names_member(&r).expect("the member");
    assert_eq!(m.get("total").unwrap().as_u64(), Some(1));
    let l = m.get("listed").unwrap().as_array().unwrap();
    assert_eq!(l.len(), 1);
    assert_eq!(l[0].get("observers").unwrap().as_u64(), Some(10));
    assert_eq!(l[0].get("first").unwrap().as_str(), Some("/m0.cpp"));
    let t = r.to_text();
    assert_eq!(t.matches("note: name-derived string changed: n001 @ /h1.h\n").count(), 1, "{t}");
    assert!(t.contains("findings 0 total, 0 listed, 0 group(s)\n"), "{t}");
    assert_eq!(r.verdict_line(), "verdict verified");
}

// §7.1: a note in a `mismatch` shard is read too, and adds nothing to its
// findings; one in an `unverifiable` shard is impossible (the section's
// rule) and an unparsed shard has none.
#[test]
fn s7_1_names_changed_beside_findings() {
    let mut r = Report::new(header(vec![], 2), &Declared::none());
    let m = with_notes(mismatch(1, vec![finding(FindingKind::Changed, 1, "/a.h", "f")]), &[1, 2]);
    r.add(&d(1), b"/a.cpp", Some(&m), Some(Status::Ok));
    r.add(&d(2), b"/b.cpp", Some(&unverifiable(2)), Some(Status::Ok));
    assert_eq!(r.names_changed(), 2);
    let j = r.to_json();
    assert_eq!(j.get("findings").unwrap().get("total").unwrap().as_u64(), Some(1));
}

// §7.1: the first 100 keys in the report's order (by name, then key), in
// JSON and in text, and how many more.
#[test]
fn s7_1_names_changed_cut_at_100() {
    let mut r = Report::new(header(vec![], 1), &Declared::none());
    let keys: Vec<u8> = (0..120).rev().collect();
    r.add(&d(1), b"/a.cpp", Some(&with_notes(verified(1), &keys)), Some(Status::Ok));
    assert_eq!(r.names_changed(), 120);
    let m = names_member(&r).unwrap();
    assert_eq!(m.get("total").unwrap().as_u64(), Some(120));
    let l = m.get("listed").unwrap().as_array().unwrap();
    assert_eq!(l.len(), 100);
    let names: Vec<&str> = l.iter().map(|x| x.get("name").unwrap().as_str().unwrap()).collect();
    let want: Vec<String> = (0..100).map(|k| format!("n{k:03}")).collect();
    assert_eq!(names, want);
    let t = r.to_text();
    assert_eq!(
        t.lines().filter(|l| l.starts_with("note: name-derived string changed: ")).count(),
        100
    );
    assert!(t.contains("note: name-derived string changed: n000 @ /h0.h\n"), "{t}");
    assert!(!t.contains("n100 @"), "{t}");
    assert!(t.contains("note: 20 more (see the report)\n"), "{t}");
}

// §7.1: a run without a note has no member and no line: the goldens of
// `s7_1_golden_json_and_text` are unchanged (that test is the check), and
// a run whose shards hold empty names sections says nothing new either.
#[test]
fn s7_1_names_changed_absent_without_a_note() {
    let r = sample();
    assert!(names_member(&r).is_none());
    assert!(!r.to_text().contains("note:"));
    let mut r = Report::new(header(vec![], 1), &Declared::none());
    r.add(&d(1), b"/a.cpp", Some(&with_notes(verified(1), &[])), Some(Status::Ok));
    assert!(names_member(&r).is_none());
    assert_eq!(r.names_changed(), 0);
}

// §7.1 `--strict-names`: each note is a finding of kind `names_changed`
// which no exemption prevents: a `verified` observer with a note is
// `mismatch`, the run `not_verified`; the key is one group over observers.
#[test]
fn s7_1_strict_names_makes_notes_findings() {
    // The noted key is in `changed`: still a finding.
    let mut r = Report::new(header(vec![], 3), &declared(&[1], &[], &[]));
    r.strict_names();
    let shard = ok_shard(1, verified_verdict(&[1]), None);
    r.add(&d(1), b"/a.cpp", Some(&with_notes(shard, &[1])), Some(Status::Ok));
    r.add(&d(2), b"/b.cpp", Some(&with_notes(verified(2), &[1])), Some(Status::Ok));
    r.add(&d(3), b"/c.cpp", Some(&verified(3)), Some(Status::Ok));
    assert_eq!(r.verdict(), RunVerdict::NotVerified);
    assert_eq!(r.states()[at(State::Mismatch)], 2);
    assert_eq!(r.states()[at(State::Verified)], 1);
    let json = r.to_json();
    let findings = json.get("findings").unwrap();
    assert_eq!(findings.get("total").unwrap().as_u64(), Some(2));
    let groups = findings.get("groups").unwrap().as_array().unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].get("kind").unwrap().as_str(), Some("names_changed"));
    assert_eq!(groups[0].get("observers").unwrap().as_u64(), Some(2));
    // The exemption is still counted as used by the shard that used it.
    let used = json.get("exemptions").unwrap().as_array().unwrap();
    assert_eq!(used[0].get("observers").unwrap().as_u64(), Some(1));
    let text = r.to_text();
    assert!(
        text.contains(&format!(
            "finding names_changed {} observers 2 n001\n",
            &key(1).to_string()[..16]
        )),
        "{text}"
    );
    assert!(text.contains("findings 2 total, 2 listed, 1 group(s)\n"), "{text}");
    assert!(text.contains("note: name-derived string changed: n001 @ /h1.h\n"), "{text}");
    assert_eq!(r.verdict_line(), "verdict not verified");
    // Without a note, the option changes nothing.
    let mut r = Report::new(header(vec![], 1), &Declared::none());
    r.strict_names();
    r.add(&d(1), b"/a.cpp", Some(&verified(1)), Some(Status::Ok));
    assert_eq!(r.verdict(), RunVerdict::Verified);
}

// §4.4, model §8.7 row 1: a shard with `digests` and no `names` breaks
// its format, so `add` never reads a result from it: a `verified` or
// `mismatch` shard without the section is `failed`, not a shard without
// notes, with the option or without it (the same holds of any shard with
// `digests`, `unverifiable` included).
#[test]
fn s4_4_compared_shard_without_names_is_failed() {
    for strict in [false, true] {
        let mut r = Report::new(header(vec![], 3), &Declared::none());
        if strict {
            r.strict_names();
        }
        let mut v = verified(1);
        v.names = None;
        r.add(&d(1), b"/a.cpp", Some(&v), Some(Status::Ok));
        let mut m = mismatch(2, vec![finding(FindingKind::Changed, 1, "/a.h", "f")]);
        m.names = None;
        r.add(&d(2), b"/b.cpp", Some(&m), Some(Status::Ok));
        let mut u = unverifiable(3);
        u.names = None;
        r.add(&d(3), b"/c.cpp", Some(&u), Some(Status::Ok));
        assert_eq!(r.states()[at(State::Failed)], 3, "strict {strict}");
        assert_eq!(r.names_changed(), 0, "strict {strict}");
        assert_eq!(r.verdict(), RunVerdict::Incomplete, "strict {strict}");
    }
}

// §7.1: the option is set before the first `add`.
#[test]
#[should_panic(expected = "Report::strict_names after Report::add")]
fn s7_1_strict_names_after_add_panics() {
    let mut r = Report::new(header(vec![], 1), &Declared::none());
    r.add(&d(1), b"/a.cpp", Some(&verified(1)), Some(Status::Ok));
    r.strict_names();
}
