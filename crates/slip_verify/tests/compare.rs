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
/// (`spec/landed/model/WP-015.md`), as the section of a verify shard; and,
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
        assert!((w[0].kind, &w[0].file, w[0].key) <= (w[1].kind, &w[1].file, w[1].key));
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
    // But a derived unit is not in D: a declared key that became derived.
    let post = [deriv("/a.h", &[u(2, 2)]), tu(&[u(1, 1)])];
    let c = plain(&pre, &post);
    assert_eq!(c.findings, vec![f(Removed, 2, "", Some(2), None)]);
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

// §4 steps 2 and 3: a key declared in one image and derived in the other
// is `removed` (or `added`) by step 2, and counted one-sided by step 3.
#[test]
fn s4_key_declared_in_one_image_and_derived_in_the_other() {
    let d = [decl("/a.h", &[u(1, 1)])];
    let v = [deriv("/a.h", &[u(1, 1)])];
    assert_eq!(
        plain(&d, &v),
        Comparison { findings: vec![f(Removed, 1, "/a.h", Some(1), None)], only_post: 1, ..none() }
    );
    assert_eq!(
        plain(&v, &d),
        Comparison { findings: vec![f(Added, 1, "/a.h", None, Some(1))], only_pre: 1, ..none() }
    );
    // The exemption prevents the step-2 finding; the count stays.
    assert_eq!(
        run(&d, &v, &ex(&[], &[], &[1])),
        Comparison { only_post: 1, used: vec![key(1)], ..none() }
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
        only(vec![
            f(Indeterminate, 1, "/a.h", Some(1), Some(1)),
            f(DerivedChanged, 1, "/a.h", Some(1), Some(2)),
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

// §6: findings ordered by kind, then file, then key.
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
        (Indeterminate, b"/a.h", 6),
        (DerivedChanged, b"/a.h", 20),
        (DerivedChanged, b"/b.h", 21),
        (DerivedIndeterminate, b"/b.h", 22),
    ];
    assert_eq!(kinds, expect);
    // Within one kind and file, by key.
    let pre = [decl("/a.h", &[u(3, 1), u(1, 1), u(2, 1)])];
    let c = plain(&pre, &[]);
    let keys: Vec<u8> = c.findings.iter().map(|x| x.key.unwrap().0[1]).collect();
    assert_eq!(keys, vec![1, 2, 3]);
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

// §4.2: malformed groups are an error naming the group and the image.
#[test]
fn landed_malformed_groups_are_rejected() {
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
fn landed_comparable_checks_group_order_and_tu() {
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
fn landed_at_sums_unsorted_and_duplicate_groups() {
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
