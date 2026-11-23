//! `spec/verify.md` §3, §4, §4.1, §4.2 and §6 over unit lists written by
//! hand. Each test names the
//! step or sentence it covers.

use slip_model::digests::{Digests, Group, GroupClass, SCHEME, Unit, group_digest, tu_digest};
use slip_model::verdict::{Finding, FindingKind, Verdict, VerdictKind};
use slip_model::{Digest, Outcome, Phase, Shard, Status};
use slip_verify::compare::{NO_REASON, REASON_MAX};
use slip_verify::{
    AtSums, CompareError, Comparison, Exemptions, at_sums, comparable, compare_groups,
    unverifiable, verdict, verified,
};

use FindingKind::{Added, Changed, DerivedChanged, DerivedIndeterminate, Indeterminate, Removed};
use GroupClass::{Declared, Derived, Tu};

// ---------------------------------------------------------------------------
// Helpers

fn key(n: u8) -> Digest {
    let mut b = [0u8; 32];
    b[0] = 0x10;
    b[1] = n;
    Digest(b)
}

fn dg(n: u8) -> Digest {
    let mut b = [0u8; 32];
    b[0] = 0x20;
    b[1] = n;
    Digest(b)
}

fn u(k: u8, d: u8) -> Unit {
    Unit { key: key(k), digest: dg(d), flags: 0 }
}

fn ui(k: u8, d: u8) -> Unit {
    Unit { key: key(k), digest: dg(d), flags: Unit::INDETERMINATE }
}

/// A well-formed group: units sorted, `count` and `digest` computed.
fn group(class: GroupClass, file: &str, units: &[Unit]) -> Group {
    let mut units = units.to_vec();
    units.sort_unstable();
    units.dedup();
    Group {
        class,
        file: file.as_bytes().to_vec(),
        count: units.len() as u64,
        indeterminate: units.iter().filter(|u| u.is_indeterminate()).count() as u64,
        digest: group_digest(&units),
        units: Some(units),
    }
}

fn decl(file: &str, units: &[Unit]) -> Group {
    group(Declared, file, units)
}

fn deriv(file: &str, units: &[Unit]) -> Group {
    group(Derived, file, units)
}

fn tu(units: &[Unit]) -> Group {
    group(Tu, "", units)
}

fn run(pre: &[Group], post: &[Group], ex: &Exemptions) -> Comparison {
    let p: Vec<&Group> = pre.iter().collect();
    let q: Vec<&Group> = post.iter().collect();
    compare_groups(&p, &q, ex).expect("well-formed groups")
}

fn plain(pre: &[Group], post: &[Group]) -> Comparison {
    run(pre, post, &Exemptions::none())
}

fn ex(changed: &[u8], created: &[u8], removed: &[u8]) -> Exemptions {
    let keys = |l: &[u8]| {
        let mut v: Vec<Digest> = l.iter().map(|&n| key(n)).collect();
        v.sort_unstable();
        v
    };
    Exemptions::new(keys(changed), keys(created), keys(removed)).unwrap()
}

fn f(kind: FindingKind, k: u8, file: &str, pre: Option<u8>, post: Option<u8>) -> Finding {
    Finding {
        kind,
        key: Some(key(k)),
        file: file.as_bytes().to_vec(),
        pre: pre.map(dg),
        post: post.map(dg),
        name: String::new(),
        location: None,
    }
}

fn none() -> Comparison {
    Comparison { findings: vec![], only_pre: 0, only_post: 0, used: vec![] }
}

fn only(findings: Vec<Finding>) -> Comparison {
    Comparison { findings, ..none() }
}

fn used(keys: &[u8]) -> Comparison {
    Comparison { used: keys.iter().map(|&n| key(n)).collect(), ..none() }
}

/// The verdict is valid under `slip_model`'s invariants
/// (`spec/model.md` §8.3), as the section of a verify shard; and,
/// field by field, under the rules of §6 that this crate relies on.
fn check_valid(v: &Verdict) {
    let shard = Shard {
        observer: dg(1),
        phase: Phase::Verify,
        round: 1,
        outcome: Outcome { status: Status::Ok, errors: 0, warnings: 0, message: String::new() },
        inputs: None,
        counters: vec![],
        digests: Some(Digests { scheme: SCHEME, tu: tu_digest(&[]), groups: vec![] }),
        verdict: Some(v.clone()),
        edits: None,
        binding: None,
        rule_use: None,
        spellings: None,
    };
    if let Err(e) = shard.validate() {
        panic!("verdict rejected by slip_model: {e}\n{v:?}");
    }
    match v.kind {
        VerdictKind::Verified => {
            assert_eq!(v.total, 0);
            assert!(v.findings.is_empty());
            assert!(v.reason.is_empty());
        }
        VerdictKind::Mismatch => {
            assert!(v.total >= 1);
            assert!(v.reason.is_empty());
        }
        VerdictKind::Unverifiable => {
            assert!(!v.reason.is_empty());
            assert_eq!(v.total, 0);
            assert!(v.findings.is_empty());
        }
    }
    assert!(v.reason.len() <= REASON_MAX);
    assert!(v.findings.len() as u64 <= v.total);
    for w in v.findings.windows(2) {
        assert!(
            (w[0].kind.rank(), &w[0].file, w[0].key) <= (w[1].kind.rank(), &w[1].file, w[1].key)
        );
        if w[0].key.is_some() {
            assert!((w[0].kind, w[0].key) != (w[1].kind, w[1].key), "one finding per kind and key");
        }
    }
    for x in &v.findings {
        match x.kind {
            Indeterminate => {}
            Changed | DerivedChanged | DerivedIndeterminate => assert!(x.key.is_some()),
            Removed => assert!(x.key.is_some() && x.post.is_none()),
            Added => assert!(x.key.is_some() && x.pre.is_none()),
        }
    }
    assert!(v.used.windows(2).all(|w| w[0] < w[1]));
}

// ---------------------------------------------------------------------------
// §4: plain equality

// §4 "verified when there is no finding": equal images.
#[test]
fn s4_equal_images_have_no_finding() {
    let a = [decl("/a.h", &[u(1, 1), u(2, 2)]), deriv("/a.h", &[u(3, 3)]), tu(&[u(4, 4)])];
    assert_eq!(plain(&a, &a), none());
    assert_eq!(plain(&[], &[]), none());
}

// ---------------------------------------------------------------------------
// §4 step 2 and §2.2: each finding, each exemption preventing exactly its own

// §4 step 2: both non-empty and different is `changed`, prevented only by `changed`.
#[test]
fn s4_2_changed_and_its_exemption() {
    let pre = [decl("/a.h", &[u(1, 1), u(2, 2)])];
    let post = [decl("/a.h", &[u(1, 9), u(2, 2)])];
    let found = only(vec![f(Changed, 1, "/a.h", Some(1), Some(9))]);
    assert_eq!(plain(&pre, &post), found);
    assert_eq!(run(&pre, &post, &ex(&[1], &[], &[])), used(&[1]));
    assert_eq!(
        run(&pre, &post, &ex(&[], &[1], &[1])),
        found,
        "created/removed do not prevent changed"
    );
    assert_eq!(run(&pre, &post, &ex(&[2], &[], &[])), found, "another key's exemption");
}

// §4 step 2: Q(k) empty is `removed`, prevented only by `removed`.
#[test]
fn s4_2_removed_and_its_exemption() {
    let pre = [decl("/a.h", &[u(1, 1), u(2, 2)])];
    let post = [decl("/a.h", &[u(2, 2)])];
    let found = only(vec![f(Removed, 1, "/a.h", Some(1), None)]);
    assert_eq!(plain(&pre, &post), found);
    assert_eq!(run(&pre, &post, &ex(&[], &[], &[1])), used(&[1]));
    assert_eq!(
        run(&pre, &post, &ex(&[1], &[1], &[])),
        found,
        "changed/created do not prevent removed"
    );
}

// §4 step 2: P(k) empty is `added`, prevented only by `created`.
#[test]
fn s4_2_added_and_its_exemption() {
    let pre = [decl("/a.h", &[u(2, 2)])];
    let post = [decl("/a.h", &[u(1, 1), u(2, 2)])];
    let found = only(vec![f(Added, 1, "/a.h", None, Some(1))]);
    assert_eq!(plain(&pre, &post), found);
    assert_eq!(run(&pre, &post, &ex(&[], &[1], &[])), used(&[1]));
    assert_eq!(
        run(&pre, &post, &ex(&[1], &[], &[1])),
        found,
        "changed/removed do not prevent added"
    );
}

// §2.2, §6 `used`: an exemption that prevented nothing is not used; only
// the ones that did are, sorted.
#[test]
fn s4_2_used_holds_exactly_the_exemptions_that_prevented_a_finding() {
    let pre = [decl("/a.h", &[u(1, 1), u(2, 2), u(3, 3), u(5, 5)])];
    let post = [decl("/a.h", &[u(1, 9), u(2, 2), u(4, 4), u(5, 5)])];
    // 1 changed, 3 removed, 4 added; 2 and 5 equal.
    let e = ex(&[1, 2, 3], &[4, 5], &[3, 5]);
    assert_eq!(run(&pre, &post, &e), used(&[1, 3, 4]));
    // Without the exemption for 4 the others still are used and 4 is found.
    let e = ex(&[1], &[], &[3]);
    let c = run(&pre, &post, &e);
    assert_eq!(c.used, vec![key(1), key(3)]);
    assert_eq!(c.findings, vec![f(Added, 4, "/a.h", None, Some(4))]);
}

// §2.2: an exemption for a key that differs in no way is not used.
#[test]
fn s4_2_unneeded_exemptions_are_not_used() {
    let a = [decl("/a.h", &[u(1, 1)])];
    assert_eq!(run(&a, &a, &ex(&[1], &[1], &[1])), none());
}

// ---------------------------------------------------------------------------
// §4 step 1: indeterminate

type Case = (Vec<Unit>, Vec<Unit>, &'static str, Option<u8>, Option<u8>);

// §4 step 1: an indeterminate unit in the pre-image only, the post-image
// only, and both; never prevented by any exemption, never `used`. §6:
// `pre`/`post` are the digest of the key's indeterminate unit in that
// image, absent where the image has none; `file` is that of the first
// group holding an indeterminate unit, pre-image first.
#[test]
fn s4_1_indeterminate_on_either_side_is_never_exempt() {
    let all = ex(&[1], &[1], &[1]);
    // (pre units, post units, expected file, pre digest, post digest)
    let cases: [Case; 7] = [
        (vec![ui(1, 1)], vec![u(1, 1)], "/p.h", Some(1), None), // pre only, same digest
        (vec![u(1, 1)], vec![ui(1, 1)], "/q.h", None, Some(1)), // post only
        (vec![ui(1, 1)], vec![ui(1, 1)], "/p.h", Some(1), Some(1)), // both, equal
        (vec![ui(1, 1)], vec![ui(1, 2)], "/p.h", Some(1), Some(2)), // both, different
        (vec![ui(1, 1)], vec![], "/p.h", Some(1), None),        // pre only, absent after
        (vec![], vec![ui(1, 1)], "/q.h", None, Some(1)),        // post only, absent before
        (vec![ui(1, 1), u(1, 2)], vec![u(1, 2)], "/p.h", Some(1), None), // one of two units
    ];
    for (n, (p, q, file, dp, dq)) in cases.into_iter().enumerate() {
        let pre = [decl("/p.h", &p), decl("/x.h", &[u(7, 7)])];
        let post = [decl("/q.h", &q), decl("/x.h", &[u(7, 7)])];
        let found = only(vec![f(Indeterminate, 1, file, dp, dq)]);
        assert_eq!(plain(&pre, &post), found, "case {n}");
        assert_eq!(run(&pre, &post, &all), found, "case {n} with every exemption");
    }
}

// §4 step 1: "the key takes no further part": one finding per key, even
// with several indeterminate units of it, and no step-2 finding beside it.
#[test]
fn s4_1_one_indeterminate_finding_per_key() {
    let pre = [decl("/a.h", &[ui(1, 1), ui(1, 2)]), decl("/b.h", &[ui(1, 1)])];
    let post = [decl("/a.h", &[u(1, 3)])];
    let c = plain(&pre, &post);
    // §6: three indeterminate occurrences before, none after: no digest.
    assert_eq!(c.findings, vec![f(Indeterminate, 1, "/a.h", None, None)]);
}

// ---------------------------------------------------------------------------
// §3: the multiset; §4 step 2 with several units of one key

// §3 multiplicity: a content-keyed unit in two groups, 2 against 2.
#[test]
fn s3_multiplicity_two_against_two() {
    let pre = [decl("/a.h", &[u(1, 1)]), decl("/b.h", &[u(1, 1)])];
    let post = [decl("/a.h", &[u(1, 1)]), decl("/c.h", &[u(1, 1)])];
    assert_eq!(plain(&pre, &post), none());
}

// §4 step 2 on differences: two equal copies against one is `removed`
// (R = {d1}, A = {}), and the `removed` exemption prevents it, the
// `changed` one does not. §6: `pre` is the one element of R, and `file`
// the first group that differs (/a.h: /b.h is the same in both).
#[test]
fn s4_2_two_equal_copies_against_one_is_removed() {
    let pre = [decl("/a.h", &[u(1, 1)]), decl("/b.h", &[u(1, 1)])];
    let post = [decl("/b.h", &[u(1, 1)])];
    let found = only(vec![f(Removed, 1, "/a.h", Some(1), None)]);
    assert_eq!(plain(&pre, &post), found);
    assert_eq!(run(&pre, &post, &ex(&[], &[], &[1])), used(&[1]));
    assert_eq!(run(&pre, &post, &ex(&[1], &[1], &[])), found, "changed/created do not prevent");
}

// §4 step 2 on differences: one copy against two is `added` (R = {},
// A = {d1}), prevented by `created` only. §6: the pre-image has no group
// that differs holding the key, so `file` is the post-image's (/a.h).
#[test]
fn s4_2_one_copy_against_two_is_added() {
    let pre = [decl("/b.h", &[u(1, 1)])];
    let post = [decl("/a.h", &[u(1, 1)]), decl("/b.h", &[u(1, 1)])];
    let found = only(vec![f(Added, 1, "/a.h", None, Some(1))]);
    assert_eq!(plain(&pre, &post), found);
    assert_eq!(run(&pre, &post, &ex(&[], &[1], &[])), used(&[1]));
    assert_eq!(run(&pre, &post, &ex(&[1], &[], &[1])), found, "changed/removed do not prevent");
}

// §4 step 2, §6: `{d1, d2}` against `{d1}` is `removed` with `pre = d2`.
#[test]
fn s4_2_two_digests_against_one_is_removed_with_pre_the_difference() {
    let pre = [tu(&[u(1, 1), u(1, 2)])];
    let post = [tu(&[u(1, 1)])];
    assert_eq!(plain(&pre, &post), only(vec![f(Removed, 1, "", Some(2), None)]));
    assert_eq!(plain(&post, &pre), only(vec![f(Added, 1, "", None, Some(2))]));
}

// §4 step 2, §6: `{d1}` against `{d2, d3}` is `changed` with `pre = d1` and
// `post` absent (A has two elements).
#[test]
fn s4_2_one_digest_against_two_others_is_changed_with_post_absent() {
    let pre = [tu(&[u(1, 1)])];
    let post = [tu(&[u(1, 2), u(1, 3)])];
    assert_eq!(plain(&pre, &post), only(vec![f(Changed, 1, "", Some(1), None)]));
    assert_eq!(plain(&post, &pre), only(vec![f(Changed, 1, "", None, Some(1))]));
    // R(k) counts multiplicity: two equal copies removed is not one element.
    let pre = [decl("/a.h", &[u(1, 1)]), decl("/b.h", &[u(1, 1)])];
    let post = [decl("/a.h", &[u(1, 2)])];
    assert_eq!(plain(&pre, &post), only(vec![f(Changed, 1, "/a.h", None, Some(2))]));
}

// §4 step 2: two different units of one key on one side (merged
// namespaces that are not the same), classified by the differences.
#[test]
fn s4_2_two_units_of_one_key() {
    let two = [tu(&[u(1, 1), u(1, 2)])];
    assert_eq!(plain(&two, &two), none());
    let one = [tu(&[u(1, 1)])];
    assert_eq!(plain(&one, &two), only(vec![f(Added, 1, "", None, Some(2))]));
    assert_eq!(plain(&two, &one), only(vec![f(Removed, 1, "", Some(2), None)]));
    let other = [tu(&[u(1, 1), u(1, 3)])];
    assert_eq!(plain(&two, &other), only(vec![f(Changed, 1, "", Some(2), Some(3))]));
    let neither = [tu(&[u(1, 3), u(1, 4)])];
    assert_eq!(plain(&two, &neither), only(vec![f(Changed, 1, "", None, None)]));
}

// §4.1, last paragraph of §4.1: a declared unit that moved between groups
// compares equal.
#[test]
fn s4_declared_unit_moved_between_groups() {
    let pre = [decl("/a.h", &[u(1, 1), u(2, 2)]), decl("/b.h", &[u(3, 3)])];
    let post = [decl("/a.h", &[u(2, 2)]), decl("/b.h", &[u(1, 1), u(3, 3)])];
    assert_eq!(plain(&pre, &post), none());
}

// §3: the `tu` group's units and the declared ones are one multiset D.
#[test]
fn s3_tu_group_is_compared_with_the_declared_ones() {
    let pre = [decl("/a.h", &[u(1, 1)]), tu(&[u(2, 2)])];
    let post = [decl("/a.h", &[u(2, 2)]), tu(&[u(1, 1)])];
    assert_eq!(plain(&pre, &post), none());
    let post = [decl("/a.h", &[u(2, 3)]), tu(&[u(1, 1)])];
    assert_eq!(plain(&pre, &post), only(vec![f(Changed, 2, "", Some(2), Some(3))]));
    // But a derived unit is not in D: a declared key that became derived
    // is not equal, it is step 4's `changed`, with `file` from D.
    let post = [deriv("/a.h", &[u(2, 2)]), tu(&[u(1, 1)])];
    let c = plain(&pre, &post);
    assert_eq!(c.findings, vec![f(Changed, 2, "", Some(2), None)]);
}

// ---------------------------------------------------------------------------
// §4 step 3: derived units

// §4 step 3: both and equal (also with multiplicity): nothing.
#[test]
fn s4_3_derived_both_equal() {
    let pre = [deriv("/a.h", &[u(1, 1)]), deriv("/b.h", &[u(1, 1)])];
    let post = [deriv("/b.h", &[u(1, 1)]), deriv("/c.h", &[u(1, 1)])];
    assert_eq!(plain(&pre, &post), none());
}

// §4 step 3: both and different is `derived_changed`, which no exemption prevents.
#[test]
fn s4_3_derived_both_different() {
    let pre = [deriv("/a.h", &[u(1, 1)])];
    let post = [deriv("/a.h", &[u(1, 2)])];
    let found = only(vec![f(DerivedChanged, 1, "/a.h", Some(1), Some(2))]);
    assert_eq!(plain(&pre, &post), found);
    assert_eq!(run(&pre, &post, &ex(&[1], &[1], &[1])), found);
    // Multiplicity counts for derived units too, on the differences: two
    // equal copies against one is R = {d1}, A = {}, a one-sided count.
    let pre = [deriv("/a.h", &[u(1, 1)]), deriv("/b.h", &[u(1, 1)])];
    let post = [deriv("/a.h", &[u(1, 1)])];
    assert_eq!(plain(&pre, &post), Comparison { only_pre: 1, ..none() });
    assert_eq!(plain(&post, &pre), Comparison { only_post: 1, ..none() });
    // And two copies against one other digest is `derived_changed`.
    let post = [deriv("/a.h", &[u(1, 2)])];
    assert_eq!(plain(&pre, &post), only(vec![f(DerivedChanged, 1, "/a.h", None, Some(2))]));
}

// §4 step 3: both, one indeterminate, is `derived_indeterminate` (whether
// or not the digests differ), which no exemption prevents. §6: `pre`/`post`
// from the indeterminate unit of that image only.
#[test]
fn s4_3_derived_both_one_indeterminate() {
    let cases = [
        (ui(1, 1), u(1, 1), Some(1), None),
        (u(1, 1), ui(1, 2), None, Some(2)),
        (ui(1, 1), ui(1, 1), Some(1), Some(1)),
    ];
    for (p, q, dp, dq) in cases {
        let pre = [deriv("/a.h", &[p])];
        let post = [deriv("/a.h", &[q])];
        let found = only(vec![f(DerivedIndeterminate, 1, "/a.h", dp, dq)]);
        assert_eq!(plain(&pre, &post), found);
        assert_eq!(run(&pre, &post, &ex(&[1], &[1], &[1])), found);
    }
}

// §4 step 3: a derived key on one side only is counted, not a finding.
#[test]
fn s4_3_derived_one_side_only_is_counted() {
    let pre = [deriv("/a.h", &[u(1, 1), u(2, 2), u(3, 3), u(5, 5)])];
    let post = [deriv("/a.h", &[u(2, 2), u(5, 5)]), deriv("/b.h", &[u(4, 4), u(6, 6)])];
    let c = plain(&pre, &post);
    assert_eq!(c, Comparison { only_pre: 2, only_post: 2, ..none() });
    // Counted per key, not per unit.
    let pre = [deriv("/a.h", &[u(1, 1), u(1, 2)]), deriv("/b.h", &[u(1, 1)])];
    assert_eq!(plain(&pre, &[]), Comparison { only_pre: 1, ..none() });
    assert_eq!(plain(&[], &pre), Comparison { only_post: 1, ..none() });
}

// §4 step 3: "every key that has a unit with the indeterminate flag in Vp
// or in Vq is one finding derived_indeterminate": also when the other image
// does not have the key, and then it is not counted in only_pre/only_post.
#[test]
fn s4_3_derived_indeterminate_on_one_side_only_is_a_finding_not_counted() {
    let pre = [deriv("/a.h", &[u(1, 1), ui(3, 3)])];
    let post = [deriv("/b.h", &[ui(4, 4), u(6, 6)])];
    assert_eq!(
        plain(&pre, &post),
        Comparison {
            findings: vec![
                f(DerivedIndeterminate, 3, "/a.h", Some(3), None),
                f(DerivedIndeterminate, 4, "/b.h", None, Some(4)),
            ],
            only_pre: 1,
            only_post: 1,
            used: vec![],
        }
    );
    // No exemption prevents it.
    let c = run(&pre, &post, &ex(&[3, 4], &[3, 4], &[3, 4]));
    assert_eq!(c.findings.len(), 2);
    assert!(c.used.is_empty());
}

// ---------------------------------------------------------------------------
// §4 step 4: declared in one image, derived in the other

fn counts(findings: Vec<Finding>, only_pre: u64, only_post: u64, used: &[u8]) -> Comparison {
    Comparison { findings, only_pre, only_post, used: used.iter().map(|&n| key(n)).collect() }
}

// §4 step 4, both directions: declared before and derived after (an
// explicit specialisation removed, its uses now instantiating the primary
// template) is `changed`, not `removed`; derived before and declared after
// is `changed`, not `added`. §6: `pre`/`post` and `file` from D, where the
// finding was found. Step 3 does not count the key.
#[test]
fn s4_4_declared_on_one_side_derived_on_the_other_is_changed() {
    // Key 2 beside it, a plain one-sided derived key, is still counted.
    let d = [decl("/a.h", &[u(1, 1)]), deriv("/t.cpp", &[u(2, 2)])];
    let v = [deriv("/t.cpp", &[u(1, 5)])];
    assert_eq!(plain(&d, &v), counts(vec![f(Changed, 1, "/a.h", Some(1), None)], 1, 0, &[]));
    assert_eq!(plain(&v, &d), counts(vec![f(Changed, 1, "/a.h", None, Some(1))], 0, 1, &[]));
    // The same digest on both sides changes nothing: the units are not
    // compared (a written unit against a made one).
    let v = [deriv("/t.cpp", &[u(1, 1)])];
    assert_eq!(plain(&d, &v), counts(vec![f(Changed, 1, "/a.h", Some(1), None)], 1, 0, &[]));
    assert_eq!(plain(&v, &d), counts(vec![f(Changed, 1, "/a.h", None, Some(1))], 0, 1, &[]));
}

// §4 step 4 and §2.2: only `changed` prevents it, and is then used;
// `removed` and `created` do not prevent it and are not used by it, so an
// observer that needed only them reports them unused (§2.2, §9.2).
#[test]
fn s4_4_only_changed_prevents_it() {
    let d = [decl("/a.h", &[u(1, 1)])];
    let v = [deriv("/a.h", &[u(1, 1)])];
    let found = |pre: Option<u8>, post: Option<u8>| vec![f(Changed, 1, "/a.h", pre, post)];
    for (pre, post, dp, dq) in [(&d, &v, Some(1), None), (&v, &d, None, Some(1))] {
        assert_eq!(run(pre, post, &ex(&[1], &[], &[])), counts(vec![], 0, 0, &[1]));
        assert_eq!(run(pre, post, &ex(&[1], &[1], &[1])), counts(vec![], 0, 0, &[1]));
        for e in [ex(&[], &[], &[1]), ex(&[], &[1], &[]), ex(&[], &[1], &[1]), ex(&[2], &[], &[])] {
            assert_eq!(run(pre, post, &e), counts(found(dp, dq), 0, 0, &[]), "{e:?}");
        }
    }
}

// §4 step 4 with the derived unit indeterminate: `changed` (prevented by
// `changed` only) and, beside it, step 3's `derived_indeterminate`, which
// nothing prevents; not counted.
#[test]
fn s4_4_with_the_derived_unit_indeterminate() {
    let d = [decl("/a.h", &[u(1, 1)])];
    let v = [deriv("/t.cpp", &[ui(1, 4)])];
    assert_eq!(
        plain(&d, &v),
        counts(
            vec![
                f(Changed, 1, "/a.h", Some(1), None),
                f(DerivedIndeterminate, 1, "/t.cpp", None, Some(4)),
            ],
            0,
            0,
            &[],
        )
    );
    assert_eq!(
        plain(&v, &d),
        counts(
            vec![
                f(Changed, 1, "/a.h", None, Some(1)),
                f(DerivedIndeterminate, 1, "/t.cpp", Some(4), None),
            ],
            0,
            0,
            &[],
        )
    );
    assert_eq!(
        run(&d, &v, &ex(&[1], &[1], &[1])),
        counts(vec![f(DerivedIndeterminate, 1, "/t.cpp", None, Some(4))], 0, 0, &[1])
    );
}

// §4 step 4 with a multiplicity: the derived unit in two groups is one
// finding and no count; two copies of a declared unit against one, with
// the derived unit beside the one, is still step 2's `removed` turned
// `changed` (step 2 is stated on the differences).
#[test]
fn s4_4_with_a_multiplicity() {
    let d = [decl("/a.h", &[u(1, 1)])];
    let v = [deriv("/b.h", &[u(1, 2)]), deriv("/c.h", &[u(1, 2)])];
    assert_eq!(plain(&d, &v), counts(vec![f(Changed, 1, "/a.h", Some(1), None)], 0, 0, &[]));
    assert_eq!(plain(&v, &d), counts(vec![f(Changed, 1, "/a.h", None, Some(1))], 0, 0, &[]));
    let two = [decl("/a.h", &[u(1, 1)]), decl("/b.h", &[u(1, 1)])];
    let one = [decl("/a.h", &[u(1, 1)]), deriv("/c.h", &[u(1, 2)])];
    assert_eq!(plain(&two, &one), counts(vec![f(Changed, 1, "/b.h", Some(1), None)], 0, 0, &[]));
    assert_eq!(plain(&one, &two), counts(vec![f(Changed, 1, "/b.h", None, Some(1))], 0, 0, &[]));
    // Without the derived unit it is `removed` and `added` as before.
    let one = [decl("/a.h", &[u(1, 1)])];
    assert_eq!(plain(&two, &one).findings, vec![f(Removed, 1, "/b.h", Some(1), None)]);
    assert_eq!(plain(&one, &two).findings, vec![f(Added, 1, "/b.h", None, Some(1))]);
}

// §4 step 4 with the key under a collision: a collision makes every unit of
// the key indeterminate (digest.md §3.4, §10). In D, step 1 takes the key:
// one `indeterminate`, no `changed`, and step 4 does not apply, so step 3
// counts the derived unit as one-sided. In V, step 4 applies and step 3's
// `derived_indeterminate` stands beside it.
#[test]
fn s4_4_with_the_key_under_a_collision() {
    let d = [decl("/a.h", &[ui(1, 1), ui(1, 2)])];
    let v = [deriv("/t.cpp", &[u(1, 3)])];
    assert_eq!(plain(&d, &v), counts(vec![f(Indeterminate, 1, "/a.h", None, None)], 0, 1, &[]));
    assert_eq!(
        run(&v, &d, &ex(&[1], &[1], &[1])),
        counts(vec![f(Indeterminate, 1, "/a.h", None, None)], 1, 0, &[])
    );
    let d = [decl("/a.h", &[u(1, 1)])];
    let v = [deriv("/t.cpp", &[ui(1, 3), ui(1, 4)])];
    assert_eq!(
        plain(&d, &v),
        counts(
            vec![
                f(Changed, 1, "/a.h", Some(1), None),
                f(DerivedIndeterminate, 1, "/t.cpp", None, None),
            ],
            0,
            0,
            &[],
        )
    );
}

// Step 1 comes before step 4: a declared unit that is indeterminate (one
// unit, no collision: a pending one), the key derived in the other image,
// and `changed` declared for it. The finding is `indeterminate`, which no
// exemption prevents; `changed` is not used; the derived unit is counted
// by step 3, since step 4 did not take the key. Not verified, either way.
#[test]
fn s4_4_step_1_comes_first() {
    let d = [decl("/a.h", &[ui(1, 1)])];
    let v = [deriv("/t.cpp", &[u(1, 3)])];
    let changed = ex(&[1], &[], &[]);
    let found = run(&d, &v, &changed);
    assert_eq!(found, counts(vec![f(Indeterminate, 1, "/a.h", Some(1), None)], 0, 1, &[]));
    assert_eq!(verdict(found, dg(2), dg(3), 8).kind, VerdictKind::Mismatch);
    let found = run(&v, &d, &changed);
    assert_eq!(found, counts(vec![f(Indeterminate, 1, "/a.h", None, Some(1))], 1, 0, &[]));
    assert_eq!(verdict(found, dg(2), dg(3), 8).kind, VerdictKind::Mismatch);
    // With every exemption: still not verified.
    let all = ex(&[1], &[1], &[1]);
    assert_eq!(verdict(run(&d, &v, &all), dg(2), dg(3), 8).kind, VerdictKind::Mismatch);
}

// §4 step 4 is read on the differences of V, as steps 2 and 3 are (§4.1):
// a derived unit of the key that both images have equally does not make a
// `removed` a `changed`; one the post-image has besides does. (Not an input
// the walk makes: a key with a determinate declared unit has no derived
// unit in its image.) And a key whose derived units differ both ways is
// `changed` by step 4 and `derived_changed` by step 3.
#[test]
fn s4_4_is_read_on_the_differences_of_v() {
    let pre = [decl("/a.h", &[u(1, 1)]), deriv("/t.cpp", &[u(1, 5)])];
    let post = [deriv("/t.cpp", &[u(1, 5)])];
    assert_eq!(plain(&pre, &post), counts(vec![f(Removed, 1, "/a.h", Some(1), None)], 0, 0, &[]));
    assert_eq!(plain(&post, &pre), counts(vec![f(Added, 1, "/a.h", None, Some(1))], 0, 0, &[]));
    let post = [deriv("/t.cpp", &[u(1, 5)]), deriv("/u.cpp", &[u(1, 5)])];
    assert_eq!(plain(&pre, &post), counts(vec![f(Changed, 1, "/a.h", Some(1), None)], 0, 0, &[]));
    assert_eq!(plain(&post, &pre), counts(vec![f(Changed, 1, "/a.h", None, Some(1))], 0, 0, &[]));
    let post = [deriv("/t.cpp", &[u(1, 6)])];
    assert_eq!(
        plain(&pre, &post),
        counts(
            vec![
                f(Changed, 1, "/a.h", Some(1), None),
                f(DerivedChanged, 1, "/t.cpp", Some(5), Some(6)),
            ],
            0,
            0,
            &[],
        )
    );
    // A `changed` of step 2 is not touched by step 4: the derived unit
    // beside it is step 3's alone, and counted.
    let pre = [decl("/a.h", &[u(1, 1)])];
    let post = [decl("/a.h", &[u(1, 2)]), deriv("/t.cpp", &[u(1, 5)])];
    assert_eq!(
        plain(&pre, &post),
        counts(vec![f(Changed, 1, "/a.h", Some(1), Some(2))], 0, 1, &[])
    );
}

// §4 steps 1 and 3 are separate: a key indeterminate in D does not affect
// the comparison of the same key in V.
#[test]
fn s4_declared_and_derived_of_one_key_are_compared_separately() {
    let pre = [decl("/a.h", &[ui(1, 1)]), deriv("/a.h", &[u(1, 1)])];
    let post = [decl("/a.h", &[ui(1, 1)]), deriv("/a.h", &[u(1, 2)])];
    assert_eq!(
        plain(&pre, &post),
        // In the order of findings by rank (§6): derived_changed first.
        only(vec![
            f(DerivedChanged, 1, "/a.h", Some(1), Some(2)),
            f(Indeterminate, 1, "/a.h", Some(1), Some(1)),
        ])
    );
}

// ---------------------------------------------------------------------------
// §6: file of a finding, order of findings

// §6 `file`: the first group, in (class, file) order, among the groups
// that differ, holding a unit of the key in the pre-image; else in the
// post-image.
#[test]
fn s6_file_of_a_finding() {
    let pre = [decl("/b.h", &[u(1, 1)]), decl("/c.h", &[u(1, 1)])];
    let post = [decl("/a.h", &[u(1, 2)])];
    assert_eq!(plain(&pre, &post).findings[0].file, b"/b.h");
    let post = [decl("/z.h", &[u(1, 2)]), decl("/y.h", &[u(1, 2)])];
    assert_eq!(plain(&[], &post).findings[0].file, b"/y.h");
    // By class first: a declared group comes before the `tu` group, whose
    // file is empty.
    let pre = [decl("/z.h", &[u(1, 1)]), tu(&[u(1, 1)])];
    let post = [decl("/z.h", &[u(1, 2)]), tu(&[u(1, 2)])];
    assert_eq!(plain(&pre, &post), only(vec![f(Changed, 1, "/z.h", None, None)]));
}

// §6 `file`: a key with a unit in a group that differs and one in a group
// that does not: the group that differs, although the other comes first.
#[test]
fn s6_file_skips_a_group_that_does_not_differ() {
    let pre = [decl("/a.h", &[u(1, 1)]), decl("/b.h", &[u(1, 1), u(2, 2)])];
    let post = [decl("/a.h", &[u(1, 1)]), decl("/b.h", &[u(1, 2), u(2, 2)])];
    assert_eq!(plain(&pre, &post), only(vec![f(Changed, 1, "/b.h", Some(1), Some(2))]));
    // The same with the derived class.
    let pre = [deriv("/a.h", &[u(1, 1)]), deriv("/b.h", &[u(1, 1), u(2, 2)])];
    let post = [deriv("/a.h", &[u(1, 1)]), deriv("/b.h", &[u(1, 2), u(2, 2)])];
    assert_eq!(plain(&pre, &post), only(vec![f(DerivedChanged, 1, "/b.h", Some(1), Some(2))]));
}

// §6 `file`: a moved unit beside a real difference. Key 3 moved from /c.h
// to /a.h (no finding of its own) and so /a.h differs; key 1's copy in
// /b.h changed, its copy in /a.h did not. The first group that differs and
// holds key 1 in the pre-image is /a.h, which is what is reported.
#[test]
fn s6_file_with_a_moved_unit_beside_a_real_difference() {
    let pre = [
        decl("/a.h", &[u(1, 1)]),
        decl("/b.h", &[u(1, 1), u(2, 2)]),
        decl("/c.h", &[u(3, 3), u(4, 4)]),
    ];
    let post = [
        decl("/a.h", &[u(1, 1), u(3, 3)]),
        decl("/b.h", &[u(1, 2), u(2, 2)]),
        decl("/c.h", &[u(4, 4)]),
    ];
    assert_eq!(plain(&pre, &post), only(vec![f(Changed, 1, "/a.h", Some(1), Some(2))]));
    // A group whose units differ only in a flag differs too (§6: "units",
    // flags included): key 2 became indeterminate in /a.h, so /a.h is
    // reported for key 1 although key 1's copy there did not change.
    let pre2 = [decl("/a.h", &[u(1, 1), u(2, 2)]), decl("/b.h", &[u(1, 1)])];
    let post2 = [decl("/a.h", &[u(1, 1), ui(2, 2)]), decl("/b.h", &[u(1, 3)])];
    assert_eq!(
        plain(&pre2, &post2),
        only(vec![
            f(Changed, 1, "/a.h", Some(1), Some(3)),
            f(Indeterminate, 2, "/a.h", None, Some(2)),
        ])
    );
    // Without the move, /a.h does not differ and /b.h is reported.
    let post = [decl("/a.h", &[u(1, 1)]), decl("/b.h", &[u(1, 2), u(2, 2)]), pre[2].clone()];
    assert_eq!(plain(&pre, &post), only(vec![f(Changed, 1, "/b.h", Some(1), Some(2))]));
}

// §6 `file` for an indeterminate key: the first group holding an
// indeterminate unit of the key, in the pre-image if it has one, whether
// or not that group differs.
#[test]
fn s6_file_of_an_indeterminate_key() {
    let a = decl("/a.h", &[u(1, 1)]);
    let b = decl("/b.h", &[ui(1, 2)]);
    let same = [a.clone(), b.clone()];
    assert_eq!(plain(&same, &same), only(vec![f(Indeterminate, 1, "/b.h", Some(2), Some(2))]));
    // Not in the pre-image: the post-image's.
    let post = [a.clone(), decl("/c.h", &[ui(1, 3)])];
    assert_eq!(
        plain(std::slice::from_ref(&a), &post),
        only(vec![f(Indeterminate, 1, "/c.h", None, Some(3))])
    );
    // Pre-image first, even when the post-image's group comes first.
    let pre = [a.clone(), decl("/c.h", &[ui(1, 3)])];
    let post = [decl("/a.h", &[ui(1, 1)])];
    assert_eq!(plain(&pre, &post), only(vec![f(Indeterminate, 1, "/c.h", Some(3), Some(1))]));
    // Derived.
    let pre = [deriv("/a.h", &[u(1, 1)]), deriv("/b.h", &[ui(1, 2)])];
    assert_eq!(plain(&pre, &pre), only(vec![f(DerivedIndeterminate, 1, "/b.h", Some(2), Some(2))]));
}

// §6: findings ordered by rank, then file, then key; `derived_changed`
// (rank 3) before `indeterminate` (rank 4).
#[test]
fn s6_order_of_findings() {
    let pre = [
        decl("/a.h", &[u(9, 1), u(4, 4)]),
        decl("/b.h", &[u(1, 1), u(5, 5)]),
        deriv("/a.h", &[u(20, 1)]),
        deriv("/b.h", &[u(21, 1), ui(22, 1)]),
    ];
    let post = [
        decl("/a.h", &[u(9, 2), u(3, 3), ui(6, 6)]),
        decl("/b.h", &[u(1, 2), u(2, 2)]),
        deriv("/a.h", &[u(20, 2), u(22, 1)]),
        deriv("/b.h", &[u(21, 2)]),
    ];
    let c = plain(&pre, &post);
    let kinds: Vec<(FindingKind, &[u8], u8)> =
        c.findings.iter().map(|x| (x.kind, x.file.as_slice(), x.key.unwrap().0[1])).collect();
    let expect: Vec<(FindingKind, &[u8], u8)> = vec![
        (Changed, b"/a.h", 9),
        (Changed, b"/b.h", 1),
        (Removed, b"/a.h", 4),
        (Removed, b"/b.h", 5),
        (Added, b"/a.h", 3),
        (Added, b"/b.h", 2),
        (DerivedChanged, b"/a.h", 20),
        (DerivedChanged, b"/b.h", 21),
        (Indeterminate, b"/a.h", 6),
        (DerivedIndeterminate, b"/b.h", 22),
    ];
    assert_eq!(kinds, expect);
    // Within one kind and file, by key.
    let pre = [decl("/a.h", &[u(3, 1), u(1, 1), u(2, 1)])];
    let c = plain(&pre, &[]);
    let keys: Vec<u8> = c.findings.iter().map(|x| x.key.unwrap().0[1]).collect();
    assert_eq!(keys, vec![1, 2, 3]);
}

/// A key of a wide range, apart from those of `key`.
fn wide(n: u32) -> Digest {
    let mut b = [0u8; 32];
    b[0] = 0x30;
    b[1..5].copy_from_slice(&n.to_be_bytes());
    Digest(b)
}

/// Ten thousand indeterminate units, half declared and half derived, on
/// both sides, all in the file `/a.h`, which sorts before `/z.h`.
fn ten_thousand_indeterminate() -> (Vec<Unit>, Vec<Unit>) {
    let ind = |n: u32| Unit { key: wide(n), digest: dg(1), flags: Unit::INDETERMINATE };
    ((0..5000).map(ind).collect(), (5000..10_000).map(ind).collect())
}

// §6 "a `derived_changed` finding is within the first `limit` (and the
// first `locate`) however many indeterminate units the observer has"
// (verify.md "Tests": compare.rs, the order of findings by rank).
#[test]
fn derived_changed_is_within_the_limit() {
    let (d, v) = ten_thousand_indeterminate();
    // `z`: declared units of `/z.h`; `digest`: that of the derived unit.
    let image = |z: &[Unit], digest: u8| {
        let mut groups = vec![
            decl("/a.h", &d),
            deriv("/a.h", &v),
            // The derived unit whose digest differs, in a file after `/a.h`.
            deriv("/z.h", &[u(1, digest)]),
        ];
        if !z.is_empty() {
            groups.push(decl("/z.h", z));
        }
        groups
    };
    let (pre, post) = (image(&[], 1), image(&[], 2));
    let derived_changed = f(DerivedChanged, 1, "/z.h", Some(1), Some(2));
    let c = plain(&pre, &post);
    assert_eq!(c.findings.len(), 10_001);
    let v64 = verdict(c.clone(), dg(100), dg(101), 64);
    check_valid(&v64);
    assert_eq!(v64.total, 10_001);
    assert_eq!(v64.findings.len(), 64);
    assert_eq!(v64.findings[0], derived_changed);
    assert!(v64.findings[1..].iter().all(|x| x.kind == Indeterminate));
    let v1 = verdict(c, dg(100), dg(101), 1);
    check_valid(&v1);
    assert_eq!(v1.findings, vec![derived_changed.clone()]);

    // With one changed (2), one removed (3) and one added (4) unit beside
    // it: those three, then it, then the indeterminate ones.
    let pre = image(&[u(2, 1), u(3, 1)], 1);
    let post = image(&[u(2, 2), u(4, 1)], 2);
    let c = plain(&pre, &post);
    let four = vec![
        f(Changed, 2, "/z.h", Some(1), Some(2)),
        f(Removed, 3, "/z.h", Some(1), None),
        f(Added, 4, "/z.h", None, Some(1)),
        derived_changed,
    ];
    let v64 = verdict(c.clone(), dg(100), dg(101), 64);
    check_valid(&v64);
    assert_eq!(v64.total, 10_004);
    assert_eq!(v64.findings[..4], four[..]);
    assert!(v64.findings[4..].iter().all(|x| x.kind == Indeterminate));
    let v4 = verdict(c, dg(100), dg(101), 4);
    check_valid(&v4);
    assert_eq!(v4.findings, four);
}

/// xorshift64 with a fixed seed, as `tests/groups.rs`.
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

/// One image of generated units: per key, up to two units, each in a
/// declared, derived or `tu` group of one of three files, with one of
/// three digests, indeterminate one time in eight. No group holds a key
/// twice.
fn generated(rng: &mut Rng, keys: u8) -> Vec<Group> {
    const FILES: [&str; 3] = ["/a.h", "/b.h", "/c.h"];
    let mut by_group: std::collections::BTreeMap<(GroupClass, &str), Vec<Unit>> =
        std::collections::BTreeMap::new();
    for k in 0..keys {
        for _ in 0..rng.below(3) {
            let class = [Declared, Derived, Tu][rng.below(3) as usize];
            let file = if class == Tu { "" } else { FILES[rng.below(3) as usize] };
            let units = by_group.entry((class, file)).or_default();
            if units.iter().any(|x| x.key == key(k)) {
                continue;
            }
            let digest = rng.below(3) as u8;
            units.push(if rng.below(8) == 0 { ui(k, digest) } else { u(k, digest) });
        }
    }
    by_group.into_iter().map(|((class, file), units)| group(class, file, &units)).collect()
}

/// §4 over all units, by key, without groups and without any order of
/// findings: the `(kind, key)` of every finding, sorted by number. A
/// reference for what the comparison finds, independent of how it sorts.
fn reference(pre: &[Group], post: &[Group]) -> (Vec<(FindingKind, Digest)>, u64, u64) {
    use std::collections::BTreeMap;
    // key -> (declared?) -> [pre digests, post digests], and indeterminate.
    type Sides = ([Vec<Digest>; 2], bool);
    let mut d: BTreeMap<Digest, Sides> = BTreeMap::new();
    let mut v: BTreeMap<Digest, Sides> = BTreeMap::new();
    for (side, image) in [pre, post].into_iter().enumerate() {
        for g in image {
            let m = if g.class == Derived { &mut v } else { &mut d };
            for x in g.units.as_ref().unwrap() {
                let e = m.entry(x.key).or_default();
                e.0[side].push(x.digest);
                e.1 |= x.is_indeterminate();
            }
        }
    }
    let differences = |p: &[Digest], q: &[Digest]| {
        let minus = |a: &[Digest], b: &[Digest]| {
            let mut b = b.to_vec();
            a.iter().any(|x| match b.iter().position(|y| y == x) {
                Some(i) => {
                    b.remove(i);
                    false
                }
                None => true,
            })
        };
        (minus(p, q), minus(q, p))
    };
    let mut out = Vec::new();
    // Step 4: what V has of the key only in the pre-image, only in the post-image.
    let v_differs = |k: &Digest| v.get(k).map_or((false, false), |([p, q], _)| differences(p, q));
    let mut taken = std::collections::BTreeSet::new();
    for (k, ([p, q], ind)) in &d {
        if *ind {
            out.push((Indeterminate, *k));
            continue;
        }
        let (only_v_pre, only_v_post) = v_differs(k);
        match differences(p, q) {
            (true, true) => out.push((Changed, *k)),
            (true, false) if only_v_post => {
                out.push((Changed, *k));
                taken.insert(*k);
            }
            (false, true) if only_v_pre => {
                out.push((Changed, *k));
                taken.insert(*k);
            }
            (true, false) => out.push((Removed, *k)),
            (false, true) => out.push((Added, *k)),
            (false, false) => {}
        }
    }
    let (mut only_pre, mut only_post) = (0, 0);
    for (k, ([p, q], ind)) in &v {
        if *ind {
            out.push((DerivedIndeterminate, *k));
            continue;
        }
        match differences(p, q) {
            (true, true) => out.push((DerivedChanged, *k)),
            _ if taken.contains(k) => {}
            (true, false) => only_pre += 1,
            (false, true) => only_post += 1,
            (false, false) => {}
        }
    }
    out.sort();
    (out, only_pre, only_post)
}

// §6, the order of findings, over generated images (a fixed seed): the
// findings are ascending by `(rank, file, key)`, and re-sorted by the
// former order `(kind, file, key)` they are a permutation of the
// comparison's findings by kind and key, none lost, none doubled, none
// of another kind (checked against a reference that does not sort).
#[test]
fn s6_order_by_rank_is_a_permutation() {
    let mut rng = Rng(0x5eed_0055_0000_0de5);
    let mut ranks_together = 0;
    let mut step4 = 0;
    for case in 0..2000 {
        let keys = 1 + rng.below(24) as u8;
        let (pre, post) = (generated(&mut rng, keys), generated(&mut rng, keys));
        let c = plain(&pre, &post);
        let order = |x: &Finding| (x.kind.rank(), x.file.clone(), x.key);
        assert!(
            c.findings.windows(2).all(|w| order(&w[0]) < order(&w[1])),
            "case {case}: not strictly ascending by (rank, file, key)"
        );
        let mut former = c.findings.clone();
        former.sort_by(|a, b| (a.kind, &a.file, a.key).cmp(&(b.kind, &b.file, b.key)));
        let mut by_rank_again = former.clone();
        by_rank_again.sort_by_key(order);
        assert_eq!(by_rank_again, c.findings, "case {case}: one set of findings, two orders");
        let mut found: Vec<(FindingKind, Digest)> =
            former.iter().map(|x| (x.kind, x.key.unwrap())).collect();
        found.sort();
        let (expected, only_pre, only_post) = reference(&pre, &post);
        assert_eq!(found, expected, "case {case}");
        assert_eq!((c.only_pre, c.only_post), (only_pre, only_post), "case {case}: counts");
        if expected.iter().any(|&(kind, k)| {
            kind == Changed && expected.binary_search(&(DerivedChanged, k)).is_err() && {
                let in_d = |g: &[Group]| {
                    g.iter().any(|x| {
                        x.class != Derived && x.units.as_ref().unwrap().iter().any(|y| y.key == k)
                    })
                };
                in_d(&pre) != in_d(&post)
            }
        }) {
            step4 += 1;
        }
        let kinds: Vec<FindingKind> = c.findings.iter().map(|x| x.kind).collect();
        if kinds.contains(&Indeterminate) && kinds.contains(&DerivedChanged) {
            ranks_together += 1;
        }
        check_valid(&verdict(c, dg(100), dg(101), 64));
    }
    assert!(ranks_together >= 100, "too few cases with ranks 3 and 4 together: {ranks_together}");
    assert!(step4 >= 100, "too few cases of step 4: {step4}");
}

// ---------------------------------------------------------------------------
// §6: verdicts

fn three_findings() -> Comparison {
    let pre = [decl("/a.h", &[u(1, 1), u(2, 2), u(3, 3)])];
    let mut c = plain(&pre, &[]);
    c.only_pre = 4;
    c.only_post = 5;
    c.used = vec![key(7)];
    c
}

// §6: `mismatch`, `total` all findings, `findings` the first `limit`,
// below, at and above the number of findings.
#[test]
fn s6_verdict_limit() {
    let all = three_findings().findings;
    for (limit, shown) in [(0, 0), (2, 2), (3, 3), (64, 3)] {
        let v = verdict(three_findings(), dg(100), dg(101), limit);
        check_valid(&v);
        assert_eq!(v.kind, VerdictKind::Mismatch);
        assert_eq!(v.total, 3);
        assert_eq!(v.findings, all[..shown].to_vec(), "limit {limit}");
        assert_eq!((v.pre, v.rules), (dg(100), dg(101)));
        assert_eq!((v.only_pre, v.only_post), (4, 5));
        assert_eq!(v.used, vec![key(7)]);
        assert!(v.reason.is_empty());
    }
}

// §6: no finding is `verified`, keeping counts and `used`.
#[test]
fn s6_verdict_verified() {
    let c = Comparison { only_pre: 1, only_post: 2, used: vec![key(1)], ..none() };
    let v = verdict(c, dg(100), dg(101), 64);
    check_valid(&v);
    assert_eq!(v.kind, VerdictKind::Verified);
    assert_eq!((v.only_pre, v.only_post, v.total), (1, 2, 0));
    assert_eq!(v.used, vec![key(1)]);
}

// §4.1 step 1: `verified` replaces the verdict at the sums: no finding,
// no count, nothing used.
#[test]
fn s4_1_verified() {
    let v = verified(dg(100), dg(101));
    check_valid(&v);
    assert_eq!(v.kind, VerdictKind::Verified);
    assert_eq!((v.pre, v.rules), (dg(100), dg(101)));
    assert_eq!((v.only_pre, v.only_post, v.total), (0, 0, 0));
    assert!(v.used.is_empty() && v.findings.is_empty() && v.reason.is_empty());
    assert_eq!(v, verdict(none(), dg(100), dg(101), 64));
}

// §6 `unverifiable`: reason kept, cut to 4096 bytes at a character
// boundary; never empty (landed rule).
#[test]
fn s6_unverifiable() {
    let v = unverifiable("no pre shard", dg(0), dg(101));
    check_valid(&v);
    assert_eq!(v.kind, VerdictKind::Unverifiable);
    assert_eq!(v.reason, "no pre shard");
    assert_eq!((v.pre, v.rules), (dg(0), dg(101)));

    let exact = "x".repeat(REASON_MAX);
    assert_eq!(unverifiable(&exact, dg(0), dg(0)).reason, exact);
    let long = format!("{}é tail", "x".repeat(REASON_MAX - 1)); // 'é' straddles byte 4096
    let v = unverifiable(&long, dg(0), dg(0));
    check_valid(&v);
    assert_eq!(v.reason, "x".repeat(REASON_MAX - 1));
    assert_eq!(unverifiable("", dg(0), dg(0)).reason, NO_REASON);
}

// ---------------------------------------------------------------------------
// Preconditions (landed): rejected, never a wrong answer

// §2: each exemption list is sorted with no duplicate.
#[test]
fn s2_exemptions_must_be_strictly_ascending() {
    assert!(Exemptions::new(vec![key(1), key(2)], vec![key(1)], vec![]).is_ok());
    for bad in [vec![key(2), key(1)], vec![key(1), key(1)]] {
        assert!(Exemptions::new(bad.clone(), vec![], vec![]).is_err());
        assert!(Exemptions::new(vec![], bad.clone(), vec![]).is_err());
        assert!(Exemptions::new(vec![], vec![], bad).is_err());
    }
}

// §2, §2.2: the exemptions of declared changes are its three lists, and
// its rules are not among them.
#[test]
fn s2_exemptions_from_declared() {
    use slip_model::keys::{KeyKind, Name, Parent, Rule};
    let rule = Rule {
        parent: Parent::Root,
        kind: KeyKind::Namespace,
        name: Name::Identifier(b"b".to_vec()),
        disc: None,
        to_parent: Parent::Root,
        to_name: Name::Identifier(b"a".to_vec()),
    };
    let declared = slip_model::declared::Declared::new(
        vec![rule],
        vec![key(1), key(2)],
        vec![key(2)],
        vec![key(3)],
    )
    .unwrap();
    let e = Exemptions::from_declared(&declared);
    assert_eq!(e, Exemptions::new(vec![key(1), key(2)], vec![key(2)], vec![key(3)]).unwrap());
    let none = Exemptions::from_declared(&slip_model::declared::Declared::none());
    assert_eq!(none, Exemptions::none());
}

// §4.2: malformed groups are an error naming the group and the image.
#[test]
fn s4_2_malformed_groups_are_rejected() {
    let good = decl("/a.h", &[u(1, 1), u(2, 2)]);
    let file = b"/a.h".to_vec();
    let check = |g: Group, expect: fn(Vec<u8>, bool) -> CompareError| {
        let ok = decl("/z.h", &[u(9, 9)]);
        let e = Exemptions::none();
        assert_eq!(compare_groups(&[&g, &ok], &[&ok], &e), Err(expect(file.clone(), false)));
        assert_eq!(compare_groups(&[&ok], &[&ok, &g], &e), Err(expect(file.clone(), true)));
    };
    check(Group { units: None, ..good.clone() }, |file, post| CompareError::MissingUnits {
        class: Declared,
        file,
        post,
    });
    let unsorted = vec![u(2, 2), u(1, 1)];
    check(
        Group { digest: group_digest(&unsorted), units: Some(unsorted), ..good.clone() },
        |file, post| CompareError::UnitsNotAscending { class: Declared, file, post },
    );
    let dup = vec![u(1, 1), u(1, 1)];
    check(Group { digest: group_digest(&dup), units: Some(dup), ..good.clone() }, |file, post| {
        CompareError::UnitsNotAscending { class: Declared, file, post }
    });
    // digest.md §11: sorted by (key, digest); one key and digest is one unit.
    let flagged = vec![u(1, 1), ui(1, 1)];
    check(
        Group { digest: group_digest(&flagged), units: Some(flagged), ..good.clone() },
        |file, post| CompareError::UnitsNotAscending { class: Declared, file, post },
    );
    let flags = vec![u(1, 1), Unit { flags: 2, ..u(2, 2) }];
    check(
        Group { digest: group_digest(&flags), units: Some(flags), ..good.clone() },
        |file, post| CompareError::BadFlags { class: Declared, file, post },
    );
    check(Group { count: 3, ..good.clone() }, |file, post| CompareError::CountMismatch {
        class: Declared,
        file,
        post,
    });
    // §4.2: `indeterminate` is the number of flagged units; checked after
    // `count` and before `digest`.
    check(Group { indeterminate: 1, ..good.clone() }, |file, post| {
        CompareError::IndeterminateMismatch { class: Declared, file, post }
    });
    let flagged = decl("/a.h", &[u(1, 1), ui(2, 2)]);
    assert_eq!(flagged.indeterminate, 1);
    check(Group { indeterminate: 0, ..flagged.clone() }, |file, post| {
        CompareError::IndeterminateMismatch { class: Declared, file, post }
    });
    check(Group { indeterminate: 2, ..flagged.clone() }, |file, post| {
        CompareError::IndeterminateMismatch { class: Declared, file, post }
    });
    check(Group { count: 3, indeterminate: 0, ..flagged.clone() }, |file, post| {
        CompareError::CountMismatch { class: Declared, file, post }
    });
    check(Group { indeterminate: 0, digest: dg(0), ..flagged.clone() }, |file, post| {
        CompareError::IndeterminateMismatch { class: Declared, file, post }
    });
    check(Group { digest: dg(0), ..good.clone() }, |file, post| CompareError::DigestMismatch {
        class: Declared,
        file,
        post,
    });
    let nothing = Exemptions::none();
    let other = decl("/a.h", &[u(3, 3)]);
    assert_eq!(
        compare_groups(&[&good, &other], &[], &nothing),
        Err(CompareError::DuplicateGroup { class: Declared, file: file.clone(), post: false })
    );
    assert_eq!(
        compare_groups(&[], &[&good, &good], &nothing),
        Err(CompareError::DuplicateGroup { class: Declared, file, post: true })
    );
    // One file in two classes is two groups.
    let d = decl("/a.h", &[u(1, 1)]);
    let v = deriv("/a.h", &[u(1, 1)]);
    assert!(compare_groups(&[&d, &v], &[&d, &v], &nothing).is_ok());
    // Groups need not be given in order.
    let a = decl("/a.h", &[u(1, 1)]);
    let b = decl("/b.h", &[u(2, 1)]);
    assert_eq!(compare_groups(&[&b, &a], &[&a, &b], &nothing), Ok(none()));
}

// ---------------------------------------------------------------------------
// §3 "Shards" and §4.1: comparable, at_sums

fn digests(groups: Vec<Group>) -> Digests {
    Digests { scheme: SCHEME, tu: tu_digest(&groups), groups }
}

// §3: both digests of the scheme this build implements.
#[test]
fn s3_comparable_schemes() {
    let a = digests(vec![decl("/a.h", &[u(1, 1)])]);
    assert_eq!(comparable(&a, &a), Ok(()));
    let other = Digests { scheme: SCHEME + 1, ..a.clone() };
    assert!(comparable(&a, &other).unwrap_err().contains("post-image"));
    assert!(comparable(&other, &a).unwrap_err().contains("pre-image"));
    // Equal but not implemented here.
    assert!(comparable(&other, &other).is_err());
}

// §3: `comparable` also checks what §4.1 step 1 trusts.
#[test]
fn s3_comparable_checks_group_order_and_tu() {
    let groups = vec![decl("/b.h", &[u(1, 1)]), decl("/a.h", &[u(2, 2)])];
    let unsorted = Digests { scheme: SCHEME, tu: tu_digest(&groups), groups };
    let a = digests(vec![decl("/a.h", &[u(1, 1)])]);
    assert!(comparable(&a, &unsorted).is_err());
    let twice = digests(vec![decl("/a.h", &[u(1, 1)]), decl("/a.h", &[u(1, 1)])]);
    assert!(comparable(&twice, &a).is_err());
    let lying = Digests { tu: dg(0), ..a.clone() };
    assert!(comparable(&a, &lying).is_err());
    assert!(comparable(&lying, &a).is_err());
}

// §4.1: equal tu is `Equal`; otherwise the groups whose sum differs,
// including groups in one image only, sorted by (class, file).
#[test]
fn s4_1_at_sums() {
    let same = decl("/s.h", &[u(1, 1)]);
    let pre = digests(vec![
        decl("/a.h", &[u(2, 2)]),
        same.clone(),
        deriv("/a.h", &[u(3, 3)]),
        tu(&[u(4, 4)]),
    ]);
    assert_eq!(at_sums(&pre, &pre), AtSums::Equal);
    let post =
        digests(vec![decl("/b.h", &[u(2, 2)]), same, deriv("/a.h", &[u(3, 4)]), tu(&[u(4, 4)])]);
    assert_eq!(
        at_sums(&pre, &post),
        AtSums::Differing(vec![
            (Declared, b"/a.h".to_vec()),
            (Declared, b"/b.h".to_vec()),
            (Derived, b"/a.h".to_vec()),
        ])
    );
    // A group whose count differs but whose digest is the same still differs.
    let g = decl("/a.h", &[u(1, 1)]);
    let p = digests(vec![g.clone()]);
    let q = digests(vec![Group { count: 2, ..g.clone() }]);
    assert_eq!(at_sums(&p, &q), AtSums::Differing(vec![(Declared, b"/a.h".to_vec())]));
    // Likewise `indeterminate` (§4.1: the sum is count, indeterminate, digest).
    let q = digests(vec![Group { indeterminate: 1, ..g }]);
    assert_eq!(at_sums(&p, &q), AtSums::Differing(vec![(Declared, b"/a.h".to_vec())]));
}

// §4.1 step 1: equal `tu` digests with an indeterminate unit are not
// `Equal`: `Differing` names every group with an indeterminate unit, and
// only those.
#[test]
fn s4_1_at_sums_equal_tu_with_an_indeterminate_unit() {
    let pre = digests(vec![
        decl("/a.h", &[u(1, 1)]),
        decl("/b.h", &[u(2, 2), ui(3, 3)]),
        deriv("/a.h", &[ui(4, 4)]),
        tu(&[u(5, 5)]),
    ]);
    assert_eq!(
        at_sums(&pre, &pre),
        AtSums::Differing(vec![(Declared, b"/b.h".to_vec()), (Derived, b"/a.h".to_vec())])
    );
    // And the comparison over those groups reports both keys.
    let AtSums::Differing(g) = at_sums(&pre, &pre) else { unreachable!() };
    let sel: Vec<&Group> = pre
        .groups
        .iter()
        .filter(|x| g.iter().any(|(c, f)| (*c, f.as_slice()) == (x.class, x.file.as_slice())))
        .collect();
    let c = compare_groups(&sel, &sel, &Exemptions::none()).unwrap();
    assert_eq!(
        c.findings,
        vec![
            f(Indeterminate, 3, "/b.h", Some(3), Some(3)),
            f(DerivedIndeterminate, 4, "/a.h", Some(4), Some(4)),
        ]
    );
    // Different tu digests: the differing groups and the indeterminate ones,
    // on either side.
    let post = digests(vec![
        decl("/a.h", &[u(1, 9)]),
        decl("/b.h", &[u(2, 2), u(3, 3)]),
        deriv("/a.h", &[u(4, 4)]),
        deriv("/z.h", &[u(6, 6)]),
        tu(&[u(5, 5)]),
    ]);
    assert_eq!(
        at_sums(&pre, &post),
        AtSums::Differing(vec![
            (Declared, b"/a.h".to_vec()),
            (Declared, b"/b.h".to_vec()),
            (Derived, b"/a.h".to_vec()),
            (Derived, b"/z.h".to_vec()),
        ])
    );
    // An indeterminate unit in the post-image only, in a group otherwise
    // the same.
    let p = digests(vec![decl("/a.h", &[u(1, 1)]), decl("/b.h", &[u(2, 2)])]);
    let q = digests(vec![
        decl("/a.h", &[u(1, 1)]),
        decl("/b.h", &[u(2, 2)]),
        deriv("/c.h", &[ui(3, 3)]),
    ]);
    assert_eq!(at_sums(&p, &q), AtSums::Differing(vec![(Derived, b"/c.h".to_vec())]));
    let q = digests(vec![decl("/a.h", &[ui(1, 1)]), decl("/b.h", &[u(2, 2)])]);
    assert_eq!(at_sums(&p, &q), AtSums::Differing(vec![(Declared, b"/a.h".to_vec())]));
    // at_sums states no precondition: `Equal` needs every `indeterminate` of
    // *both* images to be 0, read from the groups, even when the `tu`
    // digests given are equal (here forged equal).
    let forged = Digests { tu: p.tu, ..q.clone() };
    assert_eq!(at_sums(&p, &forged), AtSums::Differing(vec![(Declared, b"/a.h".to_vec())]));
    assert_eq!(
        at_sums(&Digests { tu: q.tu, ..p.clone() }, &q),
        AtSums::Differing(vec![(Declared, b"/a.h".to_vec())])
    );
}

// §4.1: at_sums does not rely on sorted groups, and reports an id given
// twice in one image only.
#[test]
fn s4_1_at_sums_unsorted_and_duplicate_groups() {
    let a = decl("/a.h", &[u(1, 1)]);
    let b = decl("/b.h", &[u(2, 2)]);
    let b2 = decl("/b.h", &[u(2, 3)]);
    let pre = Digests { scheme: SCHEME, tu: dg(1), groups: vec![b.clone(), a.clone()] };
    let post = Digests { scheme: SCHEME, tu: dg(2), groups: vec![a.clone(), b2] };
    assert_eq!(at_sums(&pre, &post), AtSums::Differing(vec![(Declared, b"/b.h".to_vec())]));
    let twice =
        Digests { scheme: SCHEME, tu: dg(3), groups: vec![a.clone(), a.clone(), b.clone()] };
    let once = Digests { scheme: SCHEME, tu: dg(4), groups: vec![a, b] };
    assert_eq!(at_sums(&twice, &once), AtSums::Differing(vec![(Declared, b"/a.h".to_vec())]));
}
