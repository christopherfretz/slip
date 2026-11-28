//! `spec/verify.md` §9.2 to §9.5 (stale observers, unused rules, "this
//! run's shard", `combine`, the report's additions) and §5.3 (the overlay's
//! guards O1 and O2), over sections written by hand. Work package WP-070;
//! the decisions pinned here are `spec/landed/verify/WP-070.md`. Each test
//! names the section it covers.
//!
//! The question this file answers for the trusted base: no stale
//! observer, no shard of another run and no guard that should fail lets
//! the run be `verified`.

#![allow(
    clippy::many_single_char_names,
    clippy::match_same_arms,
    reason = "short names of one table; the arms of §5.3's table written out one per row"
)]

mod common;

use common::{
    DECLARED, OVERLAY, ROUND, Run, d, file, golden, key, overlay, post_input, rule, section, used,
    used_kept,
};
use slip_model::agreement::Binding;
use slip_model::json;
use slip_model::keys::KeyKind;
use slip_model::overlay::{Content, Obligation, OverlayFile};
use slip_model::verdict::VerdictKind;
use slip_model::{FileRef, Overlay, Status};
use slip_verify::guards::{OverlayGuard, overlay_inputs};
use std::collections::BTreeMap;

use slip_verify::run::{Report, RunVerdict, Stale, State, combine};

fn two_rules() -> Run {
    Run::new(vec![rule(KeyKind::Function, "f", "g"), rule(KeyKind::Function, "h", "i")], None)
}

/// The number of observers in state `st`.
fn count(r: &Report, st: State) -> u64 {
    r.states()[State::ALL.iter().position(|&x| x == st).unwrap()]
}

fn stale(rule: u64, obs: u8, direct: u64) -> Stale {
    Stale { rule, observer: d(obs), direct, kept_via: 0 }
}

/// `add` with the observer's own digest, main file `/m/<obs>.cc` and an
/// `ok` map shard.
fn add(r: &mut Report, s: &slip_model::Shard) {
    let obs = s.observer;
    let main = format!("/m/{:02x}.cc", obs.0[0]);
    r.add(&obs, main.as_bytes(), Some(s), common::MAP_OK);
}

// ---------------------------------------------------------------------------
// §9.2: stale observers and unused rules

// §9.2: two observers stale for one rule; each is a pair, in plan order,
// and the run is not verified although every state is `verified`.
#[test]
fn s9_2_two_observers_stale_for_one_rule() {
    let run = two_rules();
    let mut r = run.report();
    let v = run.verdict(VerdictKind::Verified);
    add(&mut r, &run.ok(0xa1, v.clone(), &[], &[used(0, 0, 2)]));
    add(&mut r, &run.ok(0xa0, v.clone(), &[], &[used(0, 1, 1), used(1, 3, 0)]));
    assert_eq!(count(&r, State::Verified), 2);
    assert_eq!(r.stale(), [stale(0, 0xa1, 2), stale(0, 0xa0, 1)]);
    assert_eq!(r.unused(), Vec::<u64>::new());
    assert_eq!(r.verdict(), RunVerdict::NotVerified);
}

// §9.2: one observer stale for two rules; `stale` is by rule first.
#[test]
fn s9_2_one_observer_stale_for_two_rules() {
    let run = two_rules();
    let mut r = run.report();
    let v = run.verdict(VerdictKind::Verified);
    add(&mut r, &run.ok(0xa0, v.clone(), &[], &[]));
    add(&mut r, &run.ok(0xa1, v.clone(), &[], &[used(0, 0, 1), used(1, 0, 4)]));
    add(&mut r, &run.ok(0xa2, v, &[], &[used(0, 5, 0)]));
    assert_eq!(r.stale(), [stale(0, 0xa1, 1), stale(1, 0xa1, 4)]);
    assert_eq!(r.verdict(), RunVerdict::NotVerified);
}

// §9.2 "Stale for a namespace rule" (WP-111): a namespace rule is stale
// in an observer whose `direct` or `kept_via` is not 0, each alone; `via`
// alone (the namespace moved, its members with it) is a use, not stale.
// The pair carries both counts.
#[test]
fn s9_2_namespace_rule_stale_by_either_count() {
    let run = Run::new(vec![rule(KeyKind::Namespace, "n", "m")], None);
    let v = run.verdict(VerdictKind::Verified);
    for (u, stale_pair) in [
        (used_kept(0, 1, 0, 0), None),
        (used_kept(0, 0, 7, 0), Some((7, 0))),
        (used_kept(0, 1, 0, 2), Some((0, 2))),
        (used_kept(0, 0, 0, 3), Some((0, 3))),
        (used_kept(0, 4, 1, 5), Some((1, 5))),
    ] {
        let mut r = run.report();
        add(&mut r, &run.ok(0xa0, v.clone(), &[], &[]));
        add(&mut r, &run.ok(0xa1, v.clone(), &[], &[u]));
        assert!(r.unused().is_empty(), "{u:?}");
        match stale_pair {
            None => {
                assert!(r.stale().is_empty(), "{u:?}");
                assert_eq!(r.verdict(), RunVerdict::Verified, "{u:?}");
            }
            Some((direct, kept_via)) => {
                let want = Stale { rule: 0, observer: d(0xa1), direct, kept_via };
                assert_eq!(r.stale(), [want], "{u:?}");
                assert_eq!(r.verdict(), RunVerdict::NotVerified, "{u:?}");
                let text = r.to_text();
                let line = format!("stale rule 0 direct {direct} kept_via {kept_via} /m/a1.cc\n");
                assert!(text.contains(&line), "{text}");
                let json = json::write_compact(&r.to_json()).unwrap();
                let member = format!(r#""direct":{direct},"kept_via":{kept_via}}}"#);
                assert!(json.contains(&member), "{json}");
            }
        }
    }
}

// Fail closed (WP-111): a `kept_via` not 0 for a rule that is not of kind
// `namespace` (the walk never writes one) is stale too.
#[test]
fn s9_2_kept_via_of_another_kind_is_stale() {
    let run = two_rules();
    let mut r = run.report();
    let v = run.verdict(VerdictKind::Verified);
    add(&mut r, &run.ok(0xa0, v, &[], &[used_kept(0, 1, 0, 1)]));
    assert_eq!(r.stale(), [Stale { rule: 0, observer: d(0xa0), direct: 0, kept_via: 1 }]);
    assert_eq!(r.verdict(), RunVerdict::NotVerified);
}

/// A case of WP-111's attack list: each observer's `Use` of the rule and
/// its `kept` keys seen; the stale pairs `(observer, direct, kept_via)`.
struct AttackCase {
    name: &'static str,
    fixtures: &'static str,
    a0: (Option<slip_model::agreement::Use>, &'static [u64]),
    a1: (Option<slip_model::agreement::Use>, &'static [u64]),
    stale: &'static [(u8, u64, u64)],
    right: bool,
}

// WP-111 item 5, the attack list: each case is two observers whose
// `rule_use` is what the walk counts for the case's fixtures under
// `crates/slip_digest/tests/rules/stale/` (named per case; the counts are
// pinned there), and the run's stale pairs and verdict. Every observer is
// `verified`. `kept` is the indices of `Declared.kept` the shard says
// some entity has. `right` says whether the run's verdict is the right
// one for the program; the one case where it is not is WP-116's.
#[test]
fn s9_2_attack_list_two_observers() {
    use slip_model::verdict::VerdictKind;
    let u = |via, direct, kept_via| Some(used_kept(0, via, direct, kept_via));
    let cases = [
        AttackCase {
            name: "a function in two owned headers, one edited",
            fixtures: "two-headers-edited, two-headers-unedited",
            a0: (u(1, 0, 0), &[]),
            a1: (u(1, 1, 0), &[]),
            stale: &[(0xa1, 1, 0)],
            right: true,
        },
        AttackCase {
            name: "an owned overload that stays, kept",
            fixtures: "overload-kept (both observers)",
            a0: (u(1, 0, 0), &[0]),
            a1: (u(1, 0, 0), &[0]),
            stale: &[],
            right: true,
        },
        AttackCase {
            name: "an owned overload that stays, not kept",
            fixtures: "overload-not-kept, overload-kept",
            a0: (u(1, 1, 0), &[]),
            a1: (u(1, 0, 0), &[]),
            stale: &[(0xa0, 1, 0)],
            right: true,
        },
        AttackCase {
            name: "kept, and moved in one observer (the same key reached through the rule there and directly here)",
            fixtures: "overload-kept, overload-kept-moved",
            a0: (u(1, 0, 0), &[0]),
            a1: (u(1, 0, 1), &[0]),
            stale: &[(0xa1, 0, 1)],
            right: true,
        },
        AttackCase {
            name: "a compatibility using in kept and created",
            fixtures: "compat-using (both observers)",
            a0: (u(1, 0, 0), &[0]),
            a1: (u(1, 0, 0), &[0]),
            stale: &[],
            right: true,
        },
        AttackCase {
            name: "an owned explicit specialisation of a dependency template",
            fixtures: "explicit-spec-of-dependency (both observers)",
            a0: (u(1, 0, 0), &[]),
            a1: (u(1, 0, 0), &[]),
            stale: &[],
            right: true,
        },
        AttackCase {
            name: "owned in one observer, a dependency entity in the other",
            fixtures: "owned-or-dependency-dep, owned-or-dependency-owned",
            a0: (u(1, 0, 0), &[]),
            a1: (u(1, 0, 0), &[]),
            stale: &[],
            // Verified, and the program declares legacy::f in a dependency
            // and acme::f in owned code: closed by docs/wp/WP-116.md.
            right: false,
        },
        AttackCase {
            name: "a namespace only one observer sees",
            // The first observer's files declare no `legacy`: no rules
            // fixture can say so (`rules.txt` resolves in the pre-image).
            fixtures: "(no fixture), two-headers-edited",
            a0: (None, &[]),
            a1: (u(1, 0, 0), &[]),
            stale: &[],
            right: true,
        },
    ];
    let mut run = Run::new(vec![rule(KeyKind::Namespace, "legacy", "acme")], None);
    run.declared = run.declared.clone().with_kept(vec![key(5)]).unwrap();
    let v = run.verdict(VerdictKind::Verified);
    let mut rights = 0;
    for c in &cases {
        let mut r = run.report();
        for (obs, (u, kept)) in [(0xa0, &c.a0), (0xa1, &c.a1)] {
            let mut s = run.ok(obs, v.clone(), &[], u.as_slice());
            s.rule_use.as_mut().unwrap().kept = kept.to_vec();
            add(&mut r, &s);
        }
        let want: Vec<Stale> = c
            .stale
            .iter()
            .map(|&(o, direct, kept_via)| Stale { rule: 0, observer: d(o), direct, kept_via })
            .collect();
        assert_eq!(r.stale(), want, "{} ({})", c.name, c.fixtures);
        let verdict = if want.is_empty() { RunVerdict::Verified } else { RunVerdict::NotVerified };
        assert_eq!(r.verdict(), verdict, "{}", c.name);
        // The rule is used in every case: some observer has a count.
        assert!(r.unused().is_empty(), "{}", c.name);
        // `kept` key 0 is seen exactly where some shard names it.
        let seen = !c.a0.1.is_empty() || !c.a1.1.is_empty();
        assert_eq!(r.unseen_kept().is_empty(), seen, "{}", c.name);
        rights += usize::from(c.right);
    }
    assert_eq!(rights, cases.len() - 1, "one case is wrong, and it is WP-116's");
}

// §9.2: a rule used by `via` in one observer only is used, not stale;
// the other rule, used by none, is unused and changes no verdict.
#[test]
fn s9_2_rule_used_by_via_in_one_observer() {
    let run = two_rules();
    let mut r = run.report();
    let v = run.verdict(VerdictKind::Verified);
    add(&mut r, &run.ok(0xa0, v.clone(), &[], &[]));
    add(&mut r, &run.ok(0xa1, v, &[], &[used(1, 2, 0)]));
    assert!(r.stale().is_empty());
    assert_eq!(r.unused(), [0]);
    assert_eq!(r.verdict(), RunVerdict::Verified);
}

// §9.3 (WP-070, WP-114): stale pairs and uses are read from the
// `rule_use` of this run's `verified` and `mismatch` shards only: not from
// an `unverifiable` one (checked, not folded), not from a verdict the
// state table refused (its map shard failed). The mismatch's pair is read.
#[test]
fn s9_3_stale_read_from_verified_and_mismatch_shards_only() {
    let run = two_rules();
    let mut r = run.report();
    let mut u = run.unverifiable(0xa0, "post-image: x");
    u.rule_use.as_mut().unwrap().uses = vec![used(0, 0, 1)];
    add(&mut r, &u);
    let mut v = run.verified(0xa1);
    v.rule_use.as_mut().unwrap().uses = vec![used(1, 0, 1)];
    r.add(&v.observer, b"/m/a1.cc", Some(&v), Some(common::map_of(Status::Failed)));
    assert_eq!(count(&r, State::Failed), 1);
    assert!(r.stale().is_empty());
    assert_eq!(r.unused(), [0, 1]);
    assert_eq!(r.verdict(), RunVerdict::Incomplete);
    let mut m = run.mismatch(0xa2);
    m.rule_use.as_mut().unwrap().uses = vec![used(1, 0, 3)];
    add(&mut r, &m);
    assert_eq!(r.stale(), [stale(1, 0xa2, 3)]);
    assert_eq!(r.unused(), [0]);
    assert_eq!(r.verdict(), RunVerdict::NotVerified);
}

// §9.3 "Which `rule_use` is folded" (WP-114 item 5, the ruling "check
// only"): the `rule_use` of an `unverifiable` shard is not folded, whatever
// made it unverifiable (here G3, a post-image that changed during the job).
// A stale pair in it is no finding of the run: the run is `incomplete`
// (status 4, `slip verify`'s `status::INCOMPLETE`), not `not verified`
// (1), and no `stale` line is written; a use in it does not make its rule
// used, so a rule used only there is listed `unused`, and the text says
// over how many observers the fold ran; a `kept` index in it is not seen.
#[test]
fn s9_3_unverifiable_rule_use_is_checked_not_folded() {
    let mut run = two_rules();
    run.declared = run.declared.clone().with_kept(vec![key(0x50)]).unwrap();
    let mut r = run.report();
    let mut u = run.unverifiable(0xa0, "post-image: changed during the job");
    let ru = u.rule_use.as_mut().unwrap();
    ru.uses = vec![used(0, 0, 2), used(1, 1, 0)];
    ru.kept = vec![0];
    add(&mut r, &u);
    assert_eq!(count(&r, State::Unverifiable), 1);
    assert!(r.stale().is_empty());
    assert_eq!(r.unused(), [0, 1]);
    assert_eq!(r.unseen_kept(), [key(0x50)]);
    assert_eq!(r.verdict(), RunVerdict::Incomplete);
    // With a verified observer that uses rule 0: rule 1, used only in the
    // unverifiable shard, is listed unused, with the sentence.
    add(&mut r, &run.ok(0xa1, run.verdict(VerdictKind::Verified), &[], &[used(0, 1, 0)]));
    assert_eq!(r.unused(), [1]);
    assert_eq!(r.verdict(), RunVerdict::Incomplete);
    let t = r.to_text();
    assert!(
        t.contains(
            "agreement rules 2 stale 0 unused 1\nunused rule 1\n\
             agreement read 1 of 2 observers: rule use of 1 unverifiable not read\n"
        ),
        "{t}"
    );
    assert!(!t.contains("stale rule"), "{t}");
    let json = json::write_compact(&r.to_json()).unwrap();
    assert!(json.contains(r#""agreement":{"rules":2,"stale":[],"unused":[1]}"#), "{json}");
    // No unverifiable observer: no sentence.
    let mut r = run.report();
    add(&mut r, &run.ok(0xa1, run.verdict(VerdictKind::Verified), &[], &[used(0, 1, 0)]));
    assert!(!r.to_text().contains("agreement read"), "{}", r.to_text());
}

// §9.3 (WP-114): the `rule_use` of an `unverifiable` shard is still
// checked: under another rule table, with a rule past the table or a
// `kept` index past `Declared.kept`, or absent in a run with rules, the
// shard is not this run's and the observer is `failed`.
#[test]
fn s9_3_unverifiable_rule_use_still_checked() {
    let run = two_rules();
    let good = run.unverifiable(0xa0, "post-image: changed during the job");
    assert_eq!(state_of(&run, &good), State::Unverifiable);
    let mut wrong = Vec::new();
    let mut s = good.clone();
    s.rule_use.as_mut().unwrap().rules = d(0x99);
    wrong.push(("another rules_id", s));
    let mut s = good.clone();
    s.rule_use.as_mut().unwrap().uses = vec![used(2, 1, 0)];
    wrong.push(("a rule past the table", s));
    let mut s = good.clone();
    s.rule_use.as_mut().unwrap().kept = vec![0];
    wrong.push(("a kept index past Declared.kept", s));
    let mut s = good.clone();
    s.rule_use = None;
    wrong.push(("no rule_use", s));
    for (what, s) in wrong {
        let mut r = run.report();
        add(&mut r, &s);
        assert_eq!(count(&r, State::Failed), 1, "{what}");
        let t = r.to_text();
        assert!(!t.contains("agreement read 0 of 1"), "{what}: {t}");
    }
}

// §9.2: without rules, no agreement at all.
#[test]
fn s9_2_no_rules_no_stale_no_unused() {
    let run = Run::new(vec![], None);
    let mut r = run.report();
    add(&mut r, &run.verified(0xa0));
    assert!(r.stale().is_empty() && r.unused().is_empty());
    assert_eq!(r.verdict(), RunVerdict::Verified);
}

// ---------------------------------------------------------------------------
// §9.3: this run's shard

/// The state the run gives `s` as its only observer, with an `ok` map.
fn state_of(run: &Run, s: &slip_model::Shard) -> State {
    let mut r = run.report();
    add(&mut r, s);
    State::ALL.into_iter().find(|&st| count(&r, st) == 1).unwrap()
}

// §9.3: a shard a worker wrote must carry the run's binding: without
// one, or bound to another overlay, another pre round or other declared
// changes, it is not this run's and the observer is `failed`. For `ok`,
// `diagnosed` and `rejected` shards alike.
#[test]
fn s9_3_binding_must_be_the_runs() {
    let runs = [two_rules(), Run::new(vec![], Some(overlay()))];
    for run in &runs {
        let shards = [
            run.verified(0xa0),
            run.unparsed(0xa0, Status::Diagnosed, "e"),
            run.unparsed(0xa0, Status::Rejected, "e"),
        ];
        for good in &shards {
            let want = state_of(run, good);
            assert_ne!(want, State::Failed);
            let b = run.binding();
            let other_overlay = Binding { overlay: Some(d(0xee)), ..b };
            let no_overlay = Binding { overlay: None, ..b };
            let wrong = [
                None,
                Some(if b.overlay.is_some() { no_overlay } else { other_overlay }),
                Some(Binding { pre: ROUND + 1, ..b }),
                Some(Binding { pre: 0, ..b }),
                Some(Binding { declared: d(DECLARED + 1), ..b }),
            ];
            for w in wrong {
                let mut s = good.clone();
                s.binding = w;
                assert_eq!(state_of(run, &s), State::Failed, "{w:?} in {:?}", good.outcome.status);
            }
        }
    }
}

// §9.3 (WP-114): the exception of a run with a tree image and no rule is
// gone. A shard a worker wrote without a binding is not this run's there
// either: `failed`, with the reason, for an `ok`, a `diagnosed` and a
// `rejected` shard;
// one with a binding must have the run's.
#[test]
fn s9_3_tree_run_without_rules_needs_the_binding() {
    let run = Run::new(vec![], None);
    for good in [
        run.verified(0xa0),
        run.unparsed(0xa0, Status::Diagnosed, "e"),
        run.unparsed(0xa0, Status::Rejected, "e"),
    ] {
        assert_ne!(state_of(&run, &good), State::Failed);
        let mut s = good.clone();
        s.binding = None;
        assert_eq!(state_of(&run, &s), State::Failed, "{:?}", good.outcome.status);
        let mut r = run.report();
        add(&mut r, &s);
        assert_eq!(r.error_lines(), "slip: failed: /m/a0.cc: the shard has no binding\n");
        s.binding = Some(Binding { pre: 9, ..run.binding() });
        assert_eq!(state_of(&run, &s), State::Failed);
    }
    // A tree run with rules needs it (the contradiction with WP-074 item 3
    // is read the refusing way, spec/landed/verify/WP-070.md).
    let run = two_rules();
    let mut s = run.verified(0xa0);
    s.binding = None;
    assert_eq!(state_of(&run, &s), State::Failed);
}

// §9.3: a shard of status `failed` is not a worker's: it has no binding
// and none is asked of it.
#[test]
fn s9_3_failed_shard_needs_no_binding() {
    let run = two_rules();
    let s = run.unparsed(0xa0, Status::Failed, "lost");
    assert!(s.binding.is_none());
    let mut r = run.report();
    add(&mut r, &s);
    assert_eq!(count(&r, State::Failed), 1);
    let j = json::write_compact(&r.to_json()).unwrap();
    assert!(j.contains(r#""message":"lost""#), "{j}");
}

// §9.3: an `ok` shard of a run with rules must have a `rule_use` of the
// run's `rules_id`, naming no rule past the table; one of a run without
// rules must have none.
#[test]
fn s9_3_rule_use_must_be_the_runs() {
    let run = two_rules();
    let good = run.ok(0xa0, run.verdict(VerdictKind::Verified), &[], &[used(1, 1, 0)]);
    assert_eq!(state_of(&run, &good), State::Verified);
    let mut s = good.clone();
    s.rule_use = None;
    assert_eq!(state_of(&run, &s), State::Failed);
    let mut s = good.clone();
    s.rule_use.as_mut().unwrap().rules = d(0x33);
    assert_eq!(state_of(&run, &s), State::Failed);
    let mut s = good.clone();
    s.rule_use.as_mut().unwrap().uses = vec![used(2, 1, 0)];
    assert_eq!(state_of(&run, &s), State::Failed);
    // An unparsed shard has no rule_use and needs none.
    assert_eq!(state_of(&run, &run.unparsed(0xa0, Status::Diagnosed, "e")), State::NotParsed);

    let none = Run::new(vec![], None);
    let mut s = none.verified(0xa0);
    s.rule_use = good.rule_use.clone();
    assert_eq!(state_of(&none, &s), State::Failed);
}

// §9.3, §10.4: an `ok` shard has `spellings` exactly when the overlay has
// obligations.
#[test]
fn s9_3_spellings_present_exactly_with_obligations() {
    let run = Run::new(vec![], Some(overlay()));
    let good = run.verified(0xa0);
    assert!(good.spellings.is_some());
    assert_eq!(state_of(&run, &good), State::Verified);
    let mut s = good.clone();
    s.spellings = None;
    assert_eq!(state_of(&run, &s), State::Failed);

    let mut o = overlay();
    o.spellings.clear();
    let run = Run::new(vec![], Some(o));
    let good = run.verified(0xa0);
    assert!(good.spellings.is_none());
    assert_eq!(state_of(&run, &good), State::Verified);
    let mut s = good;
    s.spellings = Some(slip_model::spellings::Spellings { seen: 0, held: vec![], failed: vec![] });
    assert_eq!(state_of(&run, &s), State::Failed);
}

// §9.3: an observer whose shard is not this run's is `failed`, and its
// sections are not read: a stale pair in it does not count, a rule only
// it used is unused.
#[test]
fn s9_3_sections_of_another_runs_shard_are_not_read() {
    let run = two_rules();
    let mut s = run.ok(0xa0, run.verdict(VerdictKind::Verified), &[], &[used(0, 0, 3)]);
    s.binding = Some(Binding { pre: 0, ..run.binding() });
    let mut r = run.report();
    add(&mut r, &s);
    assert!(r.stale().is_empty());
    assert_eq!(r.unused(), [0, 1]);
    assert_eq!(r.verdict(), RunVerdict::Incomplete);
}

// ---------------------------------------------------------------------------
// §9.3: combine, every verdict against zero and non-zero counts

#[test]
fn s9_3_combine_exhaustive() {
    let verdicts = [RunVerdict::Verified, RunVerdict::NotVerified, RunVerdict::Incomplete];
    let counts = [0usize, 1, 2, usize::MAX];
    let mut rows = 0;
    for v in verdicts {
        for s in counts {
            for o in counts {
                // The table of §9.3 written out, not derived from combine.
                let expect = match (v, s, o) {
                    (RunVerdict::NotVerified, _, _) => RunVerdict::NotVerified,
                    (_, 1.., _) | (_, _, 1..) => RunVerdict::NotVerified,
                    (RunVerdict::Verified, 0, 0) => RunVerdict::Verified,
                    (RunVerdict::Incomplete, 0, 0) => RunVerdict::Incomplete,
                };
                assert_eq!(combine(v, s, o), expect, "{v:?} {s} {o}");
                rows += 1;
            }
        }
    }
    assert_eq!(rows, 48);
}

// §9.3, §9.5: `Report::verdict` is `combine` of the states' verdict: a
// stale observer makes `not verified` also when another is `failed` or
// `unverifiable` (which alone gives `incomplete`).
#[test]
fn s9_3_stale_overrides_incomplete() {
    let run = two_rules();
    let mut r = run.report();
    add(&mut r, &run.unverifiable(0xa0, "r"));
    assert_eq!(r.verdict(), RunVerdict::Incomplete);
    add(&mut r, &run.ok(0xa1, run.verdict(VerdictKind::Verified), &[], &[used(0, 0, 1)]));
    assert_eq!(r.verdict(), RunVerdict::NotVerified);
    let mut r = run.report();
    assert_eq!(r.verdict(), RunVerdict::Incomplete, "no observer");
    r.add(&d(0xa2), b"/m", None, common::MAP_OK);
    assert_eq!(r.verdict(), RunVerdict::Incomplete);
}

// ---------------------------------------------------------------------------
// §9.4, §10.5: the report's additions

/// A run with rules and an overlay: two stale pairs, an unused rule, one
/// obligation passed, one `denotes_other` in two observers, one `unseen`;
/// observers in plan order a0, a1, a2.
fn m2_report() -> Report {
    let run = m2_run();
    let (f, n) = (ix(&run, "f"), ix(&run, "n"));
    let v = run.verdict(VerdictKind::Verified);
    let o = overlay();
    let mut r = run.report();
    // The namespace rule: stale in a0 by `kept_via` (WP-111), used and not
    // stale in a1 (`via` only); kept key 0 seen by a0, key 1 by none.
    let mut s = run.ok(0xa0, v.clone(), &[0], &sorted(vec![used(f, 1, 2), used_kept(n, 0, 0, 1)]));
    s.spellings = Some(section(&o, s.inputs.as_ref().unwrap(), &[0], &[(1, 0x77)]));
    s.rule_use.as_mut().unwrap().kept = vec![0];
    add(&mut r, &s);
    let mut s = run.ok(0xa1, v.clone(), &[0], &sorted(vec![used(f, 0, 1), used(n, 2, 0)]));
    s.spellings = Some(section(&o, s.inputs.as_ref().unwrap(), &[], &[(1, 0x78)]));
    add(&mut r, &s);
    add(&mut r, &run.ok(0xa2, v, &[], &[]));
    r
}

fn m2_run() -> Run {
    let mut run = Run::new(
        vec![
            rule(KeyKind::Function, "f", "g"),
            rule(KeyKind::Function, "h", "i"),
            rule(KeyKind::Namespace, "n", "m"),
        ],
        Some(overlay()),
    );
    run.declared = run.declared.clone().with_kept(vec![key(5), key(6)]).unwrap();
    run
}

/// The index in the run's (sorted) rule table of the rule renaming `from`.
fn ix(run: &Run, from: &str) -> u64 {
    let want = slip_model::keys::Name::Identifier(from.as_bytes().to_vec());
    run.declared.rules().rules().iter().position(|r| r.name == want).unwrap() as u64
}

fn sorted(mut v: Vec<slip_model::agreement::Use>) -> Vec<slip_model::agreement::Use> {
    v.sort_by_key(|u| u.rule);
    v
}

#[test]
fn s9_4_s10_5_report_golden() {
    let r = m2_report();
    assert_eq!(r.verdict(), RunVerdict::NotVerified);
    let run = m2_run();
    let (f, n) = (ix(&run, "f"), ix(&run, "n"));
    let by_kept = Stale { rule: n, observer: d(0xa0), direct: 0, kept_via: 1 };
    let mut want = vec![stale(f, 0xa0, 2), stale(f, 0xa1, 1), by_kept];
    want.sort_by_key(|s| s.rule);
    assert_eq!(r.stale(), want);
    assert_eq!(r.unused(), [ix(&run, "h")]);
    assert_eq!(r.unseen_kept(), [key(6)]);
    golden("agreement", "report.json", &json::write_pretty(&r.to_json()).unwrap());
    let text = format!("{}{}\n", r.to_text(), r.verdict_line());
    golden("agreement", "report.txt", &text);
    // Nothing is added to standard error (§9.4): every observer is
    // `verified`.
    assert_eq!(r.error_lines(), "");
}

// §9.4: the members, in their place. `overlay` after `image` (which is
// `[]`); `agreement` and `spellings` after `exemptions`, before `run`.
#[test]
fn s9_4_member_order() {
    let j = m2_report().to_json();
    let json::Value::Object(m) = j else { panic!("not an object") };
    let names: Vec<&str> = m.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(
        names,
        [
            "report",
            "plan",
            "round",
            "declared",
            "rules",
            "image",
            "overlay",
            "limit",
            "locate",
            "verdict",
            "observers",
            "findings",
            "not_verified",
            "exemptions",
            "agreement",
            "spellings",
            "run"
        ]
    );
    let image = &m.iter().find(|(k, _)| k == "image").unwrap().1;
    assert_eq!(*image, json::Value::Array(vec![]));
}

// §9.4: each member only when it applies: rules without an overlay give
// `agreement` only; an overlay without obligations and without rules
// gives `overlay` only, and the text says `image overlay`.
#[test]
fn s9_4_members_only_when_they_apply() {
    let names = |r: &Report| {
        let json::Value::Object(m) = r.to_json() else { panic!() };
        m.into_iter().map(|(k, _)| k).collect::<Vec<_>>()
    };
    let run = two_rules();
    let mut r = run.report();
    add(&mut r, &run.verified(0xa0));
    let n = names(&r);
    assert!(n.contains(&"agreement".to_owned()));
    assert!(!n.contains(&"overlay".to_owned()) && !n.contains(&"spellings".to_owned()));
    assert!(r.to_text().contains("image identity\n"));
    assert!(
        r.to_text().contains("agreement rules 2 stale 0 unused 2\nunused rule 0\nunused rule 1\n")
    );

    let mut o = overlay();
    o.spellings.clear();
    let run = Run::new(vec![], Some(o));
    let mut r = run.report();
    add(&mut r, &run.verified(0xa0));
    let n = names(&r);
    assert!(n.contains(&"overlay".to_owned()));
    assert!(!n.contains(&"agreement".to_owned()) && !n.contains(&"spellings".to_owned()));
    let t = r.to_text();
    assert!(t.contains(&format!("image overlay {}\n", d(OVERLAY))), "{t}");
    assert!(!t.contains("image identity"));
    assert!(!t.contains("agreement") && !t.contains("spelling"));
}

// §9.4: at most 100 `stale` lines, then the count of the others; the
// JSON holds every pair.
#[test]
fn s9_4_stale_lines_at_most_100() {
    let mut run = Run::new(vec![rule(KeyKind::Function, "f", "g")], None);
    run.plan_observers = 200;
    let v = run.verdict(VerdictKind::Verified);
    for (n, more) in [(100u8, None), (101, Some(1)), (103, Some(3))] {
        let mut r = run.report();
        for i in 0..n {
            add(&mut r, &run.ok(i, v.clone(), &[], &[used(0, 0, 1)]));
        }
        let t = r.to_text();
        assert_eq!(t.lines().filter(|l| l.starts_with("stale rule ")).count(), 100);
        let tail = more.map(|m| format!("stale: {m} more (see the report)\n"));
        assert_eq!(t.contains("stale: "), tail.is_some());
        if let Some(tail) = tail {
            assert!(t.contains(&tail), "{t}");
        }
        assert_eq!(r.stale().len(), usize::from(n));
        let j = json::write_compact(&r.to_json()).unwrap();
        assert_eq!(j.matches(r#""direct":1"#).count(), usize::from(n));
    }
}

// §10.5: at most 100 `spelling` lines, then the count of the others.
#[test]
fn s10_5_spelling_lines_at_most_100() {
    for (n, more) in [(100u64, None), (101, Some(1)), (150, Some(50))] {
        let o = Overlay {
            spellings: (0..n)
                .map(|i| Obligation { file: 0, lo: i, hi: i + 1, key: key(1) })
                .collect(),
            ..overlay()
        };
        let run = Run::new(vec![], Some(o));
        let mut r = run.report();
        add(&mut r, &run.verified(0xa0));
        let t = r.to_text();
        assert!(t.contains(&format!("spelling obligations {n} passed 0 failed {n}\n")), "{t}");
        assert_eq!(t.lines().filter(|l| l.starts_with("spelling unseen ")).count(), 100);
        match more {
            None => assert!(!t.contains("spelling: ")),
            Some(m) => assert!(t.contains(&format!("spelling: {m} more (see the report)\n"))),
        }
    }
}

// §9.5: `overlay` and `pre_round` before the first `add`; `overlay` once,
// and not with a tree image. Each is a defect of the caller: a panic.
#[test]
#[should_panic(expected = "Report::overlay after Report::add")]
fn s9_5_overlay_after_add_panics() {
    let run = Run::new(vec![], None);
    let mut r = run.report();
    add(&mut r, &run.verified(0xa0));
    r.overlay(d(OVERLAY), &overlay());
}

#[test]
#[should_panic(expected = "Report::overlay called twice")]
fn s9_5_overlay_twice_panics() {
    let run = Run::new(vec![], Some(overlay()));
    let mut r = run.report();
    r.overlay(d(OVERLAY), &overlay());
}

#[test]
#[should_panic(expected = "Report::overlay with a tree image in the header")]
fn s9_5_overlay_with_tree_image_panics() {
    let run = Run::new(vec![], None);
    let mut h = run.header();
    h.image = vec![(b"/s".to_vec(), b"/p".to_vec())];
    let mut r = Report::new(h, &run.declared);
    r.overlay(d(OVERLAY), &overlay());
}

#[test]
#[should_panic(expected = "Report::pre_round after Report::add")]
fn s9_5_pre_round_after_add_panics() {
    let run = Run::new(vec![], None);
    let mut r = run.report();
    r.add(&d(0xa0), b"/m", None, None);
    r.pre_round(1);
}

// §9.5: the pre round defaults to 0; a shard bound to round 0 is then the
// run's.
#[test]
fn s9_5_pre_round_defaults_to_zero() {
    let run = two_rules();
    let mut s = run.verified(0xa0);
    s.binding = Some(Binding { pre: 0, ..run.binding() });
    let mut r = Report::new(run.header(), &run.declared);
    add(&mut r, &s);
    assert_eq!(count(&r, State::Verified), 1);
}

// ---------------------------------------------------------------------------
// §5.3: the overlay's guards O1 and O2

/// The three contents a path can have: `A`, `B` (A's size, another
/// digest) and `C` (A's digest, another size).
const CONTENTS: [(u64, u8); 3] = [(1, 0xa), (1, 0xb), (2, 0xa)];

fn entry(path: &[u8], c: (u64, u8)) -> FileRef {
    FileRef { path: path.to_vec(), size: c.0, content: d(c.1) }
}

/// What one path contributes: the overlay names it with a `post` content
/// (or not), the post-image read it with a content (or not), the pre shard
/// listed it with a content (or not). 4 × 4 × 4 = 64 cases.
#[derive(Clone, Copy, Debug)]
struct Case {
    overlay: Option<(u64, u8)>,
    post: Option<(u64, u8)>,
    pre: Option<(u64, u8)>,
}

fn cases() -> Vec<Case> {
    let opts: Vec<Option<(u64, u8)>> =
        std::iter::once(None).chain(CONTENTS.iter().copied().map(Some)).collect();
    let mut v = Vec::new();
    for &overlay in &opts {
        for &post in &opts {
            for &pre in &opts {
                v.push(Case { overlay, post, pre });
            }
        }
    }
    v
}

/// §5.3 for one path, written out: O1 if the overlay names it and the
/// post-image read another content; O2 if the overlay does not name it
/// and the post-image read a content other than the pre shard's.
fn expect_one(c: Case) -> Option<char> {
    match (c.overlay, c.post, c.pre) {
        (_, None, _) => None,
        (Some(o), Some(p), _) if o != p => Some('1'),
        (Some(_), Some(_), _) => None,
        (None, Some(p), Some(q)) if p != q => Some('2'),
        (None, Some(_), _) => None,
    }
}

fn lists(cases: &[(&[u8], Case)]) -> (Vec<FileRef>, Vec<FileRef>, Overlay) {
    let mut pre = Vec::new();
    let mut post = Vec::new();
    let mut files = Vec::new();
    for (p, c) in cases {
        if let Some(x) = c.pre {
            pre.push(entry(p, x));
        }
        if let Some(x) = c.post {
            post.push(entry(p, x));
        }
        if let Some(x) = c.overlay {
            // `pre` of the overlay entry is never read by the guards.
            files.push(OverlayFile {
                path: p.to_vec(),
                pre: Content { size: 9, content: d(0x9) },
                post: Content { size: x.0, content: d(x.1) },
            });
        }
    }
    (pre, post, Overlay { edits: d(0x04), files, spellings: vec![] })
}

// §5.3: every case of one path, 64 rows.
#[test]
fn s5_3_guards_one_path_exhaustive() {
    let mut rows = 0;
    for c in cases() {
        let (pre, post, o) = lists(&[(b"/s/p.h", c)]);
        let got = overlay_inputs(&pre, &post, &o);
        let want = match expect_one(c) {
            None => Ok(()),
            Some('1') => Err(OverlayGuard::O1(b"/s/p.h".to_vec())),
            Some(_) => Err(OverlayGuard::O2(b"/s/p.h".to_vec())),
        };
        assert_eq!(got, want, "{c:?}");
        rows += 1;
    }
    assert_eq!(rows, 64);
}

// §5.3: every pair of cases over two paths, each list given in both
// orders: 64 × 64 × 2 = 8192 rows. O1 over every path before O2; each in
// path order.
#[test]
fn s5_3_guards_two_paths_exhaustive() {
    let all = cases();
    let mut rows = 0;
    let (p, q): (&[u8], &[u8]) = (b"/s/p.h", b"/s/q.h");
    for &a in &all {
        for &b in &all {
            let (ea, eb) = (expect_one(a), expect_one(b));
            let want = if ea == Some('1') {
                Err(OverlayGuard::O1(p.to_vec()))
            } else if eb == Some('1') {
                Err(OverlayGuard::O1(q.to_vec()))
            } else if ea == Some('2') {
                Err(OverlayGuard::O2(p.to_vec()))
            } else if eb == Some('2') {
                Err(OverlayGuard::O2(q.to_vec()))
            } else {
                Ok(())
            };
            for reversed in [false, true] {
                let order = if reversed { [(q, b), (p, a)] } else { [(p, a), (q, b)] };
                let (pre, post, mut o) = lists(&order);
                // The overlay as the store gives it is sorted; the guards
                // must not depend on it either way.
                if !reversed {
                    o.files.sort_by(|x, y| x.path.cmp(&y.path));
                }
                assert_eq!(overlay_inputs(&pre, &post, &o), want, "{a:?} {b:?} {reversed}");
                rows += 1;
            }
        }
    }
    assert_eq!(rows, 8192);
}

// §5.3: a path the pre shard lists twice must agree with both entries;
// one the post-image lists twice is checked once per entry.
#[test]
fn s5_3_guards_duplicate_entries() {
    let o = Overlay { edits: d(0x04), files: vec![], spellings: vec![] };
    let p = b"/s/p.h";
    let a = entry(p, CONTENTS[0]);
    let b = entry(p, CONTENTS[1]);
    assert_eq!(overlay_inputs(&[a.clone(), a.clone()], std::slice::from_ref(&a), &o), Ok(()));
    for pre in [[a.clone(), b.clone()], [b.clone(), a.clone()]] {
        assert_eq!(
            overlay_inputs(&pre, std::slice::from_ref(&a), &o),
            Err(OverlayGuard::O2(p.to_vec()))
        );
    }
    for post in [[a.clone(), b.clone()], [b.clone(), a.clone()]] {
        assert_eq!(
            overlay_inputs(std::slice::from_ref(&a), &post, &o),
            Err(OverlayGuard::O2(p.to_vec()))
        );
    }
    let named = Overlay {
        files: vec![OverlayFile {
            path: p.to_vec(),
            pre: Content { size: 1, content: d(0xa) },
            post: Content { size: 1, content: d(0xa) },
        }],
        ..o
    };
    assert_eq!(overlay_inputs(&[], &[a.clone(), b], &named), Err(OverlayGuard::O1(p.to_vec())));
    assert_eq!(overlay_inputs(&[], &[a.clone(), a], &named), Ok(()));
}

// §5.3: the overlay's own examples. An overlay file the post-image did
// not read is not an error; a file in only one list is not one; an
// overlay entry with `post == pre` is checked like any other.
#[test]
fn s5_3_guards_named_cases() {
    let o = overlay();
    let all: Vec<FileRef> = (0..3).map(|i| post_input(&o, i)).collect();
    assert_eq!(overlay_inputs(&[], &[], &o), Ok(()));
    assert_eq!(overlay_inputs(&[], &all, &o), Ok(()));
    assert_eq!(overlay_inputs(&[file("/s/only-pre.h", 1, 1)], &[], &o), Ok(()));
    assert_eq!(overlay_inputs(&[], &[file("/s/only-post.h", 1, 1)], &o), Ok(()));
    // /s/c.h has post == pre; read at another content it fails O1.
    let mut c = post_input(&o, 2);
    c.content = d(0x01);
    assert_eq!(overlay_inputs(&[], &[c], &o), Err(OverlayGuard::O1(b"/s/c.h".to_vec())));
    // The overlay's pre content of a file the post-image read is O1's
    // failure: the post-image was not read through the overlay.
    let f = &o.files[0];
    let pre_read = FileRef { path: f.path.clone(), size: f.pre.size, content: f.pre.content };
    assert_eq!(
        overlay_inputs(std::slice::from_ref(&pre_read), std::slice::from_ref(&pre_read), &o),
        Err(OverlayGuard::O1(b"/s/a.h".to_vec()))
    );
}

// §5.3: the reasons, with a path that is not UTF-8.
#[test]
fn s5_3_guard_reasons() {
    assert_eq!(
        OverlayGuard::O1(b"/s/a.h".to_vec()).reason(),
        "post-image: /s/a.h was not read through the overlay"
    );
    assert_eq!(
        OverlayGuard::O2(b"/s/\xffb\xfe.h".to_vec()).reason(),
        "pre-image: /s/\\xffb\\xfe.h changed since the map phase"
    );
    assert_eq!(
        OverlayGuard::O2("/s/é.h".as_bytes().to_vec()).reason(),
        "pre-image: /s/é.h changed since the map phase"
    );
}

// §9.4: a run with a tree image and no rule writes exactly the M1
// document, with M1's members only, its shards carrying the run's binding
// (WP-114; before it, M1's shards without one gave the same report). (M1's
// own goldens, `tests/golden/run/`, are checked by `tests/run.rs`,
// unchanged.) Without the binding every observer is `failed` (§9.3).
#[test]
fn s9_4_m1_run_report_unchanged() {
    let run = Run::new(vec![], None);
    let report = |with_binding: bool| {
        let mut r = run.report();
        for s in
            [run.verified(0xa0), run.mismatch(0xa1), run.unparsed(0xa2, Status::Diagnosed, "e")]
        {
            let mut s = s;
            if !with_binding {
                s.binding = None;
            }
            add(&mut r, &s);
        }
        r
    };
    let (unbound, m1) = (report(false), report(true));
    assert_eq!(count(&unbound, State::Failed), 3);
    assert_eq!(unbound.verdict(), RunVerdict::Incomplete);
    let j1 = json::write_pretty(&m1.to_json()).unwrap();
    for member in ["\"overlay\"", "\"agreement\"", "\"spellings\""] {
        assert!(!j1.contains(member), "{member}");
    }
    let t = m1.to_text();
    assert!(t.contains("image identity\n"));
    assert!(!t.contains("agreement") && !t.contains("spelling") && !t.contains("image overlay"));
    assert_eq!(m1.verdict(), RunVerdict::NotVerified);
}

// §9.3: a run without rules takes no `rule_use`, not even one of the
// empty table's id with no use.
#[test]
fn s9_3_rule_use_without_rules_even_empty() {
    let run = Run::new(vec![], None);
    let mut s = run.verified(0xa0);
    s.rule_use = Some(slip_model::agreement::RuleUse {
        rules: run.declared.rules().id(),
        uses: vec![],
        kept: vec![],
    });
    s.validate().unwrap();
    assert_eq!(state_of(&run, &s), State::Failed);
}

// ---------------------------------------------------------------------------
// §7.1, §9.3 (WP-121 item 4): the message of a shard that is not this run's

/// The observer's `not_verified` message and its line on standard error,
/// for `s` as the only observer of `run`, with an `ok` map.
fn refusal(run: &Run, s: &slip_model::Shard) -> (String, String) {
    let mut r = run.report();
    add(&mut r, s);
    assert_eq!(count(&r, State::Failed), 1);
    let j = r.to_json();
    let nv = j.get("not_verified").unwrap().as_array().unwrap();
    let message = nv[0].get("message").unwrap().as_str().unwrap().to_owned();
    (message, r.error_lines())
}

// §9.3: each way a shard fails "this run's shard" from its own sections
// has its reason, which is the observer's message and the text after the
// main file on standard error. The shard's own message is never shown.
#[test]
fn s9_3_refused_shard_says_why() {
    let rules = two_rules();
    let with_overlay = Run::new(vec![], Some(overlay()));
    let mut o = overlay();
    o.spellings.clear();
    let no_obligation = Run::new(vec![], Some(o));
    let b = rules.binding();
    let verified = |run: &Run| run.verified(0xa0);
    let mut cases: Vec<(&Run, slip_model::Shard, &str)> = Vec::new();
    let mut s = verified(&rules);
    s.binding = None;
    cases.push((&rules, s, "the shard has no binding"));
    let mut s = rules.unparsed(0xa0, Status::Diagnosed, "own message");
    s.binding = None;
    cases.push((&rules, s, "the shard has no binding"));
    let mut s = verified(&rules);
    s.binding = Some(Binding { overlay: Some(d(0xee)), ..b });
    cases.push((&rules, s, "the shard's binding is not this run's: its overlay"));
    let mut s = verified(&rules);
    s.binding = Some(Binding { declared: d(DECLARED + 1), ..b });
    cases.push((&rules, s, "the shard's binding is not this run's: its declared changes"));
    let mut s = verified(&rules);
    s.binding = Some(Binding { pre: 0, ..b });
    let pre = format!("the shard's binding is not this run's: its pre round 0, not {ROUND}");
    cases.push((&rules, s, &pre));
    // Two fields differ: the first in the order overlay, declared, pre
    // is named (review F5).
    let mut s = verified(&rules);
    s.binding = Some(Binding { overlay: Some(d(0xee)), declared: d(DECLARED + 1), ..b });
    cases.push((&rules, s, "the shard's binding is not this run's: its overlay"));
    let mut s = verified(&rules);
    s.binding = Some(Binding { declared: d(DECLARED + 1), pre: 0, ..b });
    cases.push((&rules, s, "the shard's binding is not this run's: its declared changes"));
    let mut s = verified(&rules);
    s.rule_use = None;
    cases.push((&rules, s, "the shard has no rule_use"));
    let none = Run::new(vec![], None);
    let mut s = verified(&none);
    s.rule_use =
        Some(slip_model::agreement::RuleUse { rules: d(0x33), uses: vec![], kept: vec![] });
    cases.push((&none, s, "the shard has a rule_use and the run has no rule"));
    let mut s = verified(&rules);
    s.rule_use.as_mut().unwrap().rules = d(0x33);
    cases.push((&rules, s, "the shard's rule_use is under other rules than the run's"));
    let mut s = verified(&rules);
    s.rule_use.as_mut().unwrap().uses = vec![used(2, 1, 0)];
    cases.push((&rules, s, "the shard's rule_use names rule 2; the run has 2"));
    // A kept index equal to the number of kept keys is out of range
    // (WP-111 review, mutant R2: `>` for `>=`); one below it is a key.
    let mut kept_two = two_rules();
    kept_two.declared = kept_two.declared.clone().with_kept(vec![key(5), key(6)]).unwrap();
    let mut s = verified(&kept_two);
    s.rule_use.as_mut().unwrap().kept = vec![1, 2];
    cases.push((&kept_two, s, "the shard's rule_use names kept key 2; the run has 2"));
    let mut s = verified(&with_overlay);
    s.spellings = None;
    cases.push((&with_overlay, s, "the shard has no spellings"));
    let mut s = verified(&no_obligation);
    s.spellings = Some(slip_model::spellings::Spellings { seen: 0, held: vec![], failed: vec![] });
    cases.push((&no_obligation, s, "the shard has spellings and the run has no obligation"));
    for (run, mut s, want) in cases {
        if s.outcome.status == Status::Ok {
            s.outcome.message.clear();
        }
        let (message, line) = refusal(run, &s);
        assert_eq!(message, want);
        assert_eq!(line, format!("slip: failed: /m/a0.cc: {want}\n"));
    }
    let mut s = verified(&kept_two);
    s.rule_use.as_mut().unwrap().kept = vec![1];
    let mut r = kept_two.report();
    add(&mut r, &s);
    assert_eq!(count(&r, State::Failed), 0);
}

// §10.4 (WP-121 item 4): a section the fold refuses gives the fold's
// reason, for a shard that is folded (`verified`) and one that is only
// checked (`unverifiable`).
#[test]
fn s10_4_fold_refusal_says_why() {
    use slip_model::spellings::{Span, Spellings};
    let run = Run::new(vec![], Some(overlay()));
    for kind in [VerdictKind::Verified, VerdictKind::Unverifiable] {
        let mut v = run.verdict(kind);
        if kind == VerdictKind::Unverifiable {
            v.reason = "post-image: x".to_owned();
        }
        let mut s = run.ok(0xa0, v, &[0], &[]);
        // Obligation 2 is of /s/b.h, which this shard did not read.
        s.spellings =
            Some(Spellings { seen: 2, held: vec![Span { first: 2, count: 1 }], failed: vec![] });
        s.validate().unwrap();
        let (message, line) = refusal(&run, &s);
        assert_eq!(
            message,
            "the shard's spellings are refused: obligation 2 is of /s/b.h, which the shard did not read"
        );
        assert_eq!(line, format!("slip: failed: /m/a0.cc: {message}\n"));
    }
}

/// A map shard of the run's pre round for observer `obs`: `ok`, reading
/// `/s/x.cpp` and the overlay paths `read`.
fn map_shard(obs: u8, read: &[&str]) -> slip_model::Shard {
    let mut inputs = vec![file("/s/x.cpp", 1, 0x99)];
    inputs.extend(read.iter().map(|p| file(p, 10, 0x51)));
    inputs.sort_by(|a, b| a.path.cmp(&b.path));
    slip_model::Shard {
        observer: d(obs),
        phase: slip_model::Phase::Map,
        round: ROUND,
        outcome: slip_model::Outcome {
            status: Status::Ok,
            errors: 0,
            warnings: 0,
            message: String::new(),
        },
        inputs: Some(inputs),
        counters: vec![],
        digests: None,
        verdict: None,
        edits: None,
        binding: None,
        rule_use: None,
        spellings: None,
        names: None,
        negatives: None,
        found: None,
        rebuilt_use: None,
        existing_use: None,
    }
}

/// The identities of the files of [`overlay`] and of `/s/x.cpp`, each its
/// own, with `extra`.
fn ids(extra: &[(&str, (u64, u64))]) -> BTreeMap<Vec<u8>, (u64, u64)> {
    let mut m: BTreeMap<Vec<u8>, (u64, u64)> =
        [("/s/a.h", (1, 1)), ("/s/b.h", (1, 2)), ("/s/c.h", (1, 3)), ("/s/x.cpp", (1, 9))]
            .iter()
            .map(|(p, id)| (p.as_bytes().to_vec(), *id))
            .collect();
    m.extend(extra.iter().map(|(p, id)| (p.as_bytes().to_vec(), *id)));
    m
}

/// `run`'s report, given `ids` when the run has an overlay.
fn report_with(run: &Run, ids: Option<BTreeMap<Vec<u8>, (u64, u64)>>) -> Report {
    let mut r = run.report();
    if let (Some(_), Some(ids)) = (&run.overlay, ids) {
        r.identities(ids);
    }
    r
}

// spec/edits.md §8.2, §5.5 (WP-076): `add_unchanged_at` decides
// "unchanged" from the map shard itself and fails closed: the observer is
// `unchanged` only in a run with an overlay and no rule, from its own `ok`
// map shard of the pre round, with inputs none of which is an overlay file
// by path or by the identities given, every one with an identity.
#[test]
fn s5_5_unchanged_is_decided_by_the_report() {
    // No obligation: an observer that read no overlay file meets none.
    let bare = || Overlay { spellings: Vec::new(), ..overlay() };
    let no_rules = Run::new(vec![], Some(bare()));
    let mut r = report_with(&no_rules, Some(ids(&[])));
    r.add_unchanged_at(0, &d(0x10), b"/m/a.cc", &map_shard(0x10, &[]));
    assert_eq!(count(&r, State::Unchanged), 1);
    assert_eq!(r.verdict(), RunVerdict::Verified);
    assert_eq!(r.error_lines(), "");

    let wrong_with = |run: &Run, ids: Option<BTreeMap<Vec<u8>, (u64, u64)>>, map, why: &str| {
        let mut r = report_with(run, ids);
        r.add_unchanged_at(0, &d(0x10), b"/m/a.cc", &map);
        assert_eq!(count(&r, State::Unchanged), 0, "{why}");
        assert_eq!(count(&r, State::Failed), 1, "{why}");
        assert_ne!(r.verdict(), RunVerdict::Verified, "{why}");
        let lines = r.error_lines();
        assert!(
            lines.starts_with("slip: failed: /m/a.cc: the observer got no verify job"),
            "{lines}"
        );
        assert!(lines.contains(why), "{why}: {lines}");
    };
    let wrong = |run: &Run, map, why: &str| wrong_with(run, Some(ids(&[])), map, why);
    wrong(&no_rules, map_shard(0x10, &["/s/a.h"]), "read the overlay's /s/a.h");
    let mut m = map_shard(0x10, &[]);
    m.round = ROUND + 1;
    wrong(&no_rules, m, "not its shard of phase map");
    wrong(&no_rules, map_shard(0x11, &[]), "not its shard of phase map");
    let mut m = map_shard(0x10, &[]);
    m.outcome.status = Status::Diagnosed;
    wrong(&no_rules, m, "is not ok");
    let mut m = map_shard(0x10, &[]);
    m.inputs = None;
    wrong(&no_rules, m, "has no inputs");
    let rules = Run::new(vec![rule(KeyKind::Function, "f", "g")], Some(bare()));
    wrong(&rules, map_shard(0x10, &[]), "in a run with rules");
    wrong(&Run::new(vec![], None), map_shard(0x10, &[]), "without an overlay");

    // By identity (WP-076 fix round, B2): `/s/A.h` is `/s/a.h` under
    // another spelling (a case-insensitive volume, a hard link).
    wrong_with(
        &no_rules,
        Some(ids(&[("/s/A.h", (1, 1))])),
        map_shard(0x10, &["/s/A.h"]),
        "read an overlay file as /s/A.h",
    );
    // Omissions fail closed: no identities at all, an input without one,
    // an overlay file without one.
    wrong_with(&no_rules, None, map_shard(0x10, &[]), "the run gave no file identities");
    wrong_with(
        &no_rules,
        Some(ids(&[])),
        map_shard(0x10, &["/s/d.h"]),
        "its input /s/d.h has no identity",
    );
    let mut partial = ids(&[]);
    partial.remove(b"/s/c.h".as_slice());
    wrong_with(
        &no_rules,
        Some(partial),
        map_shard(0x10, &[]),
        "the overlay's /s/c.h has no identity",
    );
    // Another file with an identity of its own is not an overlay file.
    let mut r = report_with(&no_rules, Some(ids(&[("/s/d.h", (1, 4))])));
    r.add_unchanged_at(0, &d(0x10), b"/m/a.cc", &map_shard(0x10, &["/s/d.h"]));
    assert_eq!(count(&r, State::Unchanged), 1);
}

// §7.3 (WP-076 fix round, C5): a plan position is added once, by
// `add_unchanged_at` as by `add_at`.
#[test]
#[should_panic(expected = "position 0 added twice")]
fn s7_3_unchanged_at_a_position_twice() {
    let no_rules = Run::new(vec![], Some(Overlay { spellings: Vec::new(), ..overlay() }));
    let mut r = report_with(&no_rules, Some(ids(&[])));
    r.add_unchanged_at(0, &d(0x10), b"/m/a.cc", &map_shard(0x10, &[]));
    r.add_unchanged_at(0, &d(0x11), b"/m/b.cc", &map_shard(0x11, &[]));
}
