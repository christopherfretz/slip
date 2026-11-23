//! `spec/verify.md` §10.3 and §10.4, the run's half of §10.6: the fold of
//! this run's `spellings` sections into one outcome per obligation, and
//! the fold through `Report::add` (§10.5). Work package WP-070; decisions
//! in `spec/landed/verify/WP-070.md`. Each test names the section it
//! covers.
//!
//! The question this file answers for the trusted base: an obligation
//! passes only when some observer of this run held it and none failed it,
//! and a section the fold cannot place changes nothing.

#![allow(clippy::many_single_char_names, reason = "the overlay, the fold, its inputs and sections")]

mod common;

use common::{Run, file, key, overlay, post_input, section};
use slip_model::spellings::{Failed, Span, Spellings};
use slip_model::verdict::VerdictKind;
use slip_model::{FileRef, Status};
use slip_verify::run::{RunVerdict, State};
use slip_verify::spelling::{Fold, Outcome};

/// The `inputs` of a shard that read overlay files `read` and a main file.
fn inputs(read: &[usize]) -> Vec<FileRef> {
    let o = overlay();
    let mut v: Vec<FileRef> = read.iter().map(|&i| post_input(&o, i)).collect();
    v.push(file("/s/x.cpp", 1, 0x99));
    v.sort_by(|a, b| a.path.cmp(&b.path));
    v
}

fn fold() -> Fold {
    Fold::new(&overlay())
}

fn other(observers: u64, first: usize, n: u8) -> Outcome {
    Outcome::DenotesOther { observers, first, found: Some(key(n)), name: format!("other{n}") }
}

// §10.3 row 1: fails in some observer: `denotes_other`, with the number
// of such observers, the first in plan order and what it found; a
// `held` elsewhere does not save it.
#[test]
fn s10_3_fails_in_some_observer() {
    let o = overlay();
    let mut f = fold();
    let i = inputs(&[0]);
    f.add(0, &i, &section(&o, &i, &[0, 1], &[])).unwrap();
    f.add(1, &i, &section(&o, &i, &[0], &[(1, 5)])).unwrap();
    f.add(2, &i, &section(&o, &i, &[], &[(1, 6)])).unwrap();
    assert_eq!(f.finish(), [Outcome::Passed { not_seen: 1 }, other(2, 1, 5), Outcome::Unseen]);
}

// §10.3 row 2: holds in at least one observer and fails in none:
// `passed`, the observers that read the file and did not see it
// counted. An observer that did not read the file is not counted.
#[test]
fn s10_3_holds_in_one_passes_with_not_seen() {
    let o = overlay();
    let mut f = fold();
    let both = inputs(&[0, 1]);
    f.add(0, &both, &section(&o, &both, &[2], &[])).unwrap();
    for p in 1..10 {
        f.add(p, &both, &section(&o, &both, &[], &[])).unwrap();
    }
    let none = inputs(&[]);
    f.add(10, &none, &section(&o, &none, &[], &[])).unwrap();
    assert_eq!(f.finish()[2], Outcome::Passed { not_seen: 9 });
}

// §10.3 row 3: held in none and failed in none: `unseen`, whether no
// observer read the file, or every one that did saw nothing, or there is
// no observer at all.
#[test]
fn s10_3_unseen() {
    assert_eq!(fold().finish(), [Outcome::Unseen, Outcome::Unseen, Outcome::Unseen]);
    let o = overlay();
    let mut f = fold();
    let i = inputs(&[0, 1]);
    f.add(0, &i, &section(&o, &i, &[], &[])).unwrap();
    let none = inputs(&[]);
    f.add(1, &none, &section(&o, &none, &[], &[])).unwrap();
    assert_eq!(f.finish(), [Outcome::Unseen, Outcome::Unseen, Outcome::Unseen]);
    assert!(!Outcome::Unseen.passed());
    assert!(!other(1, 0, 1).passed());
    assert!(Outcome::Passed { not_seen: 0 }.passed());
}

// §10.3: the first in plan order is kept whatever the order of the
// calls, with what that observer found; the count is of every observer.
#[test]
fn s10_3_first_in_plan_order_whatever_the_call_order() {
    let o = overlay();
    let i = inputs(&[0]);
    let orders: [[usize; 3]; 6] =
        [[3, 5, 7], [3, 7, 5], [5, 3, 7], [5, 7, 3], [7, 3, 5], [7, 5, 3]];
    for order in orders {
        let mut f = fold();
        for p in order {
            f.add(p, &i, &section(&o, &i, &[], &[(0, p as u8)])).unwrap();
        }
        assert_eq!(f.finish()[0], other(3, 3, 3), "{order:?}");
    }
    // A failure without `found` (the origin had no key).
    let mut f = fold();
    let mut s = section(&o, &i, &[], &[(0, 1)]);
    s.failed[0].found = None;
    f.add(0, &i, &s).unwrap();
    assert_eq!(
        f.finish()[0],
        Outcome::DenotesOther { observers: 1, first: 0, found: None, name: "other1".to_owned() }
    );
}

/// Every way a section can be refused, with the reason's start.
fn refusals() -> Vec<(&'static str, Vec<FileRef>, Spellings)> {
    let o = overlay();
    let a = inputs(&[0]);
    let ab = inputs(&[0, 1]);
    let sp = |seen, held: Vec<Span>, failed: Vec<Failed>| Spellings { seen, held, failed };
    let span = |first, count| Span { first, count };
    let fl = |index| Failed { index, found: Some(key(1)), name: String::new() };
    vec![
        ("an empty span", a.clone(), sp(2, vec![span(0, 0)], vec![])),
        ("spans that touch", a.clone(), sp(2, vec![span(0, 1), span(1, 1)], vec![])),
        ("spans descending", ab.clone(), sp(3, vec![span(2, 1), span(0, 1)], vec![])),
        ("spans overlapping", ab.clone(), sp(3, vec![span(0, 2), span(1, 1)], vec![])),
        ("a span that overflows", a.clone(), sp(2, vec![span(1, u64::MAX)], vec![])),
        ("a held index past the overlay", ab.clone(), sp(3, vec![span(2, 2)], vec![])),
        ("a held index far past", ab.clone(), sp(3, vec![span(9, 1)], vec![])),
        ("failed not ascending", a.clone(), sp(2, vec![], vec![fl(1), fl(0)])),
        ("failed twice", a.clone(), sp(2, vec![], vec![fl(0), fl(0)])),
        ("a failed index past the overlay", ab.clone(), sp(3, vec![], vec![fl(3)])),
        ("held and failed", a.clone(), sp(2, vec![span(0, 2)], vec![fl(1)])),
        ("held of a file not read", a.clone(), sp(2, vec![span(2, 1)], vec![])),
        ("failed of a file not read", a.clone(), sp(2, vec![], vec![fl(2)])),
        ("held of no file read", inputs(&[]), sp(0, vec![span(0, 1)], vec![])),
        ("seen too small", a.clone(), sp(1, vec![span(0, 1)], vec![])),
        ("seen too large", a.clone(), sp(3, vec![], vec![])),
        ("seen too large, nothing read", inputs(&[]), sp(1, vec![], vec![])),
        ("more indices than seen", ab.clone(), sp(2, vec![span(0, 3)], vec![])),
        ("seen of the overlay, not of the inputs", a, section(&o, &ab, &[], &[])),
    ]
}

// §10.4, §10.6: each refusal is an `Err`, and the fold is as it was: the
// same outcomes, and the same outcomes after a further good section.
#[test]
fn s10_4_refusals_leave_the_fold_unchanged() {
    let o = overlay();
    let i = inputs(&[0, 1]);
    let good = section(&o, &i, &[0], &[(2, 4)]);
    for (what, inp, s) in refusals() {
        let mut f = fold();
        f.add(0, &i, &good).unwrap();
        let before = f.clone().finish();
        let e = f.add(1, &inp, &s).expect_err(what);
        assert!(e.starts_with("spellings: "), "{what}: {e}");
        assert_eq!(f.clone().finish(), before, "{what}");
        f.add(2, &i, &good).unwrap();
        let mut g = fold();
        g.add(0, &i, &good).unwrap();
        g.add(2, &i, &good).unwrap();
        assert_eq!(f.finish(), g.finish(), "{what}");
    }
}

// §10.4: the reasons of the refusals the interface names (an obligation
// the overlay does not have, a file the shard did not read).
#[test]
fn s10_4_refusal_reasons() {
    let a = inputs(&[0]);
    let s = Spellings {
        seen: 2,
        held: vec![],
        failed: vec![Failed { index: 7, found: None, name: String::new() }],
    };
    assert_eq!(
        fold().add(0, &a, &s).unwrap_err(),
        "spellings: obligation 7 is not one of the overlay's 3"
    );
    let s = Spellings { seen: 2, held: vec![Span { first: 2, count: 1 }], failed: vec![] };
    assert_eq!(
        fold().add(0, &a, &s).unwrap_err(),
        "spellings: obligation 2 is of /s/b.h, which the shard did not read"
    );
    let s = Spellings { seen: 1, held: vec![], failed: vec![] };
    assert_eq!(
        fold().add(0, &a, &s).unwrap_err(),
        "spellings: seen is 1, but the files the shard read have 2 obligations"
    );
}

// §10.4: an index the section does not name, of a file read, is not seen;
// `inputs` in any order, listing a file twice, or naming files the overlay
// does not have, give the same marks.
#[test]
fn s10_4_inputs_order_and_duplicates() {
    let o = overlay();
    let mut i = inputs(&[0, 1]);
    let s = section(&o, &i, &[1], &[]);
    let mut f = fold();
    f.add(0, &i, &s).unwrap();
    i.reverse();
    i.push(post_input(&o, 0));
    i.push(file("/s/zz.h", 3, 3));
    let mut g = fold();
    g.add(0, &i, &s).unwrap();
    assert_eq!(f.finish(), g.finish());
}

// §10.3: an overlay without obligations folds to nothing; `len` and
// `is_empty` say so.
#[test]
fn s10_6_empty_overlay() {
    let mut o = overlay();
    o.spellings.clear();
    let mut f = Fold::new(&o);
    assert!(f.is_empty());
    assert_eq!(f.len(), 0);
    f.add(0, &inputs(&[0, 1, 2]), &Spellings { seen: 0, held: vec![], failed: vec![] }).unwrap();
    assert!(f.finish().is_empty());
    assert_eq!(fold().len(), 3);
}

// §10.6: the fold is total over an overlay that breaks its invariants
// (an obligation of a file it does not have): that obligation can be
// named by no section and stays `unseen`.
#[test]
fn s10_6_total_over_an_invalid_overlay() {
    let mut o = overlay();
    o.spellings[2].file = 9;
    let mut f = Fold::new(&o);
    let i = inputs(&[0, 1]);
    // Only file 0's two obligations are seen now.
    let s = Spellings { seen: 2, held: vec![Span { first: 0, count: 2 }], failed: vec![] };
    f.add(0, &i, &s).unwrap();
    let bad = Spellings { seen: 2, held: vec![Span { first: 2, count: 1 }], failed: vec![] };
    assert!(f.add(1, &i, &bad).is_err());
    assert_eq!(
        f.finish(),
        [Outcome::Passed { not_seen: 0 }, Outcome::Passed { not_seen: 0 }, Outcome::Unseen]
    );
}

// §10.3: deterministic: the same sections in the same order give the
// same outcomes, and so does any order of the same calls (the outcome
// depends on the positions, not the calls' order).
#[test]
fn s10_3_deterministic_and_order_free() {
    let o = overlay();
    let calls: Vec<(usize, Vec<FileRef>, Spellings)> = vec![
        (0, inputs(&[0]), section(&o, &inputs(&[0]), &[0], &[(1, 1)])),
        (1, inputs(&[0, 1]), section(&o, &inputs(&[0, 1]), &[1, 2], &[])),
        (2, inputs(&[1]), section(&o, &inputs(&[1]), &[], &[(2, 2)])),
        (3, inputs(&[0, 1]), section(&o, &inputs(&[0, 1]), &[], &[(1, 3)])),
    ];
    let run = |order: &[usize]| {
        let mut f = fold();
        for &k in order {
            let (p, i, s) = &calls[k];
            f.add(*p, i, s).unwrap();
        }
        f.finish()
    };
    let want = run(&[0, 1, 2, 3]);
    assert_eq!(want, [Outcome::Passed { not_seen: 2 }, other(2, 0, 1), other(1, 2, 2)]);
    assert_eq!(run(&[0, 1, 2, 3]), want);
    for order in [[3, 2, 1, 0], [1, 3, 0, 2], [2, 0, 3, 1]] {
        assert_eq!(run(&order), want, "{order:?}");
    }
}

// ---------------------------------------------------------------------------
// Through `Report::add` (§9.3, §10.5)

// §10.4: a section the fold refuses makes the shard not this run's: the
// observer is `failed`, its `rule_use` is not read, and the fold is as it
// was.
#[test]
fn s10_4_fold_refusal_through_report_add() {
    let run = Run::new(
        vec![common::rule(slip_model::keys::KeyKind::Function, "f", "g")],
        Some(overlay()),
    );
    let o = overlay();
    let mut r = run.report();
    let v = run.verdict(VerdictKind::Verified);
    let mut s = run.ok(0xa0, v.clone(), &[0], &[common::used(0, 0, 1)]);
    let i = s.inputs.clone().unwrap();
    // Obligation 2 is of /s/b.h, which this shard did not read.
    s.spellings =
        Some(Spellings { seen: 2, held: vec![Span { first: 2, count: 1 }], failed: vec![] });
    s.validate().unwrap();
    r.add(&s.observer, b"/m/a0.cc", Some(&s), Some(Status::Ok));
    assert_eq!(r.states()[6], 1, "failed");
    assert!(r.stale().is_empty());
    assert_eq!(r.unused(), [0]);
    assert_eq!(r.spellings(), [Outcome::Unseen, Outcome::Unseen, Outcome::Unseen]);
    let mut s = run.ok(0xa1, v, &[0], &[]);
    s.spellings = Some(section(&o, &i, &[0, 1], &[]));
    r.add(&s.observer, b"/m/a1.cc", Some(&s), Some(Status::Ok));
    assert_eq!(r.states()[0], 1, "verified");
    assert_eq!(
        r.spellings(),
        [Outcome::Passed { not_seen: 0 }, Outcome::Passed { not_seen: 0 }, Outcome::Unseen]
    );
    assert_eq!(r.verdict(), RunVerdict::NotVerified);
}

// §10.3, §10.5, §10.6: only this run's `verified` and `mismatch` shards
// are folded: a verified verdict whose map shard is not `ok` (failed) and
// an unparsed shard fold nothing; an `unverifiable` one (a guard failed:
// its post-image is not the overlay's) folds nothing either, but its
// section is checked, and one the fold refuses makes it `failed`.
// Positions are plan positions, so `first` names the observer of the
// lowest position that failed it.
#[test]
fn s10_3_which_shards_are_folded() {
    let run = Run::new(vec![], Some(overlay()));
    let o = overlay();
    let i = inputs(&[0]);
    let mut r = run.report();
    let mut s = run.ok(0xa0, run.verdict(VerdictKind::Verified), &[0], &[]);
    s.spellings = Some(section(&o, &i, &[0, 1], &[]));
    r.add(&s.observer, b"/m/a0.cc", Some(&s), Some(Status::Failed));
    let u = run.unparsed(0xa1, Status::Diagnosed, "e");
    r.add(&u.observer, b"/m/a1.cc", Some(&u), Some(Status::Ok));
    assert_eq!(r.spellings(), [Outcome::Unseen, Outcome::Unseen, Outcome::Unseen]);
    let at = |st: State| State::ALL.iter().position(|&x| x == st).unwrap();
    // Held and failed by an unverifiable observer: not counted.
    let mut s = run.unverifiable(0xa2, "post-image: why");
    s.inputs = Some(i.clone());
    s.spellings = Some(section(&o, &i, &[0], &[(1, 9)]));
    s.validate().unwrap();
    r.add(&s.observer, b"/m/a2.cc", Some(&s), Some(Status::Ok));
    assert_eq!(r.spellings(), [Outcome::Unseen, Outcome::Unseen, Outcome::Unseen]);
    assert_eq!(r.states()[at(State::Unverifiable)], 1);
    // Checked all the same: obligation 2 is of /s/b.h, which it did not
    // read.
    let mut s = run.unverifiable(0xa3, "post-image: why");
    s.inputs = Some(i.clone());
    s.spellings =
        Some(Spellings { seen: 2, held: vec![Span { first: 2, count: 1 }], failed: vec![] });
    s.validate().unwrap();
    r.add(&s.observer, b"/m/a3.cc", Some(&s), Some(Status::Ok));
    assert_eq!(r.states()[at(State::Unverifiable)], 1);
    assert_eq!(r.states()[at(State::Failed)], 2, "a0 (its map shard) and a3");
    // A mismatch folds.
    let mut s = run.mismatch(0xa4);
    s.inputs = Some(i.clone());
    s.spellings = Some(section(&o, &i, &[0], &[(1, 9)]));
    s.validate().unwrap();
    r.add(&s.observer, b"/m/a4.cc", Some(&s), Some(Status::Ok));
    assert_eq!(r.spellings(), [Outcome::Passed { not_seen: 0 }, other(1, 4, 9), Outcome::Unseen]);
    let j = slip_model::json::write_compact(&r.to_json()).unwrap();
    assert!(j.contains(r#""first":"/m/a4.cc""#), "{j}");
    assert!(j.contains(r#""obligations":3,"passed":1"#), "{j}");
}

// §9.3, §10.3: every obligation passed and every observer verified: the
// run is verified; one obligation unseen: not verified, also when every
// state is `verified`.
#[test]
fn s10_3_verdict_needs_every_obligation_passed() {
    let run = Run::new(vec![], Some(overlay()));
    let o = overlay();
    let v = run.verdict(VerdictKind::Verified);
    let all = inputs(&[0, 1]);
    let mut r = run.report();
    let mut s = run.ok(0xa0, v.clone(), &[0, 1], &[]);
    s.spellings = Some(section(&o, &all, &[0, 1], &[]));
    r.add(&s.observer, b"/m/a0.cc", Some(&s), Some(Status::Ok));
    assert_eq!(r.verdict(), RunVerdict::NotVerified, "obligation 2 unseen");
    let mut s = run.ok(0xa1, v, &[1], &[]);
    s.spellings = Some(section(&o, &inputs(&[1]), &[2], &[]));
    r.add(&s.observer, b"/m/a1.cc", Some(&s), Some(Status::Ok));
    assert_eq!(r.verdict(), RunVerdict::Verified);
    assert!(r.spellings().iter().all(Outcome::passed));
    let t = r.to_text();
    assert!(t.contains("spelling obligations 3 passed 3 failed 0\n"), "{t}");
    assert!(!t.contains("spelling denotes_other") && !t.contains("spelling unseen"));
}
