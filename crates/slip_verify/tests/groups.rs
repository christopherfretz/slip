//! `spec/verify.md` §4.1: comparing through group sums against comparing
//! all units, over generated pairs of images.
//!
//! The generator is a fixed-seed xorshift (no dependency); a failure prints
//! the seed and case, and `SLIP_VERIFY_SEED=<n>` reruns with another seed.
//!
//! §4.1 says restricting §4 to the groups `G` (those whose sums differ and
//! those with an indeterminate unit) is exact: the same findings, counts
//! and used exemptions as over all units. That is checked on every
//! generated pair, with no pair set aside: `compare_groups` over all groups
//! equals `compare_groups` over the groups `at_sums` lists, field for
//! field; and `at_sums` is `Equal` exactly when every group has the same
//! sum in both images and none has an indeterminate unit, and then the
//! full comparison has no finding, no count and nothing used.

use std::collections::BTreeSet;

use slip_model::digests::{Digests, Group, GroupClass, SCHEME, Unit, group_digest, tu_digest};
use slip_model::verdict::{Finding, FindingKind};
use slip_model::{Digest, Outcome, Phase, Shard, Status};
use slip_verify::{AtSums, Comparison, Exemptions, at_sums, comparable, compare_groups, verdict};

const SEED: u64 = 0x5eed_0016_c0de_f00d;
const CASES: usize = 12_000;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64
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
    fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }
}

fn key(n: u64) -> Digest {
    let mut b = [0u8; 32];
    b[0] = 0x10;
    b[1..9].copy_from_slice(&n.to_le_bytes());
    Digest(b)
}

fn dg(n: u64) -> Digest {
    let mut b = [0u8; 32];
    b[0] = 0x20;
    b[1..9].copy_from_slice(&n.to_le_bytes());
    Digest(b)
}

const FILES: [&[u8]; 4] = [b"/s/a.h", b"/s/b.h", b"/s/c.h", b"/s/d.cpp"];

/// An image before grouping: every unit with its group.
type Image = Vec<(GroupClass, Vec<u8>, Unit)>;

fn file_for(rng: &mut Rng, class: GroupClass) -> Vec<u8> {
    match class {
        GroupClass::Tu => Vec::new(),
        GroupClass::Declared | GroupClass::Derived => {
            FILES[rng.below(FILES.len() as u64) as usize].to_vec()
        }
    }
}

fn class_for(rng: &mut Rng) -> GroupClass {
    match rng.below(10) {
        0..=6 => GroupClass::Declared,
        7 | 8 => GroupClass::Derived,
        _ => GroupClass::Tu,
    }
}

fn base(rng: &mut Rng) -> Image {
    let mut img = Image::new();
    let n = rng.below(20);
    for k in 0..n {
        let class = class_for(rng);
        let file = file_for(rng, class);
        let flags = if rng.chance(1) { Unit::INDETERMINATE } else { 0 };
        let unit = Unit { key: key(k), digest: dg(rng.below(3)), flags };
        img.push((class, file, unit));
        // A content-keyed unit written in another file.
        if class == GroupClass::Declared && rng.chance(8) {
            let other = file_for(rng, class);
            img.push((class, other, unit));
        }
        // Merged namespaces that are not the same: two units of one key.
        if class == GroupClass::Tu && rng.chance(20) {
            img.push((class, Vec::new(), Unit { digest: dg(7), ..unit }));
        }
    }
    img
}

/// What edits were made and what the pairs held, for the coverage
/// assertions: each counter is a number of pairs (`edits` and `findings`:
/// of edits and findings).
#[derive(Default)]
struct Coverage {
    edits: [u64; 7],
    /// `at_sums` was `Equal`.
    equal: u64,
    /// `Differing` with equal `tu` digests (only indeterminate groups).
    equal_tu_differing: u64,
    /// A key with units both in a group of `G` and in a group outside it.
    straddling: u64,
    /// A group with an indeterminate unit and the same sum in both images.
    indeterminate_unchanged: u64,
    findings: [u64; 6],
    used: u64,
    only_pre: u64,
    only_post: u64,
    /// A `derived_indeterminate` key the other image's `V` does not have.
    derived_indeterminate_one_sided: u64,
    /// A `removed` or `added` finding for a key with a unit in both images
    /// (several units of one key, classified by the differences).
    one_sided_kind_on_shared_key: u64,
    /// A `changed` finding for a key that D has in one image only (§4 step 4).
    step4: u64,
    /// A `removed` (`added`) finding for a key with a derived unit in the
    /// post-image (pre-image): one that both images derive equally, so not
    /// step 4, which is read on the differences of `V`. A reading of step 4
    /// as "has a unit in `Vq`" would give `changed` over all groups and
    /// `removed` through `G`, where that group is not.
    v_on_both_sides: u64,
}

fn edit(rng: &mut Rng, img: &mut Image, next_key: &mut u64, cov: &mut Coverage) {
    let which = rng.below(7) as usize;
    if img.is_empty() && which != 2 {
        return;
    }
    let at = if img.is_empty() { 0 } else { rng.below(img.len() as u64) as usize };
    match which {
        // Change a digest.
        0 => img[at].2.digest = dg(3 + rng.below(3)),
        // Remove.
        1 => {
            img.swap_remove(at);
        }
        // Add.
        2 => {
            let class = class_for(rng);
            let file = file_for(rng, class);
            let k = if rng.chance(15) && !img.is_empty() {
                img[at].2.key // a second unit of an existing key
            } else {
                *next_key += 1;
                key(*next_key)
            };
            img.push((class, file, Unit { key: k, digest: dg(rng.below(3)), flags: 0 }));
        }
        // Move to another file (the first declaration moved).
        3 => {
            let class = img[at].0;
            img[at].1 = file_for(rng, class);
        }
        // Duplicate a content-keyed unit in another file.
        4 => {
            let (class, _, unit) = img[at].clone();
            let file = file_for(rng, class);
            img.push((class, file, unit));
        }
        // Flag indeterminate.
        5 => img[at].2.flags = Unit::INDETERMINATE,
        // Declared becomes derived or the reverse.
        _ => {
            let class = match img[at].0 {
                GroupClass::Declared => GroupClass::Derived,
                GroupClass::Derived | GroupClass::Tu => GroupClass::Declared,
            };
            img[at].0 = class;
            img[at].1 = file_for(rng, class);
        }
    }
    cov.edits[which] += 1;
}

/// The `Digests` of an image, with every unit list (`digest.md` §11).
fn digests(img: &Image) -> Digests {
    let mut sorted = img.clone();
    sorted.sort();
    // Two units of one group with one key and digest are one unit
    // (digest.md §11); indeterminate if either is.
    sorted.dedup_by(|later, kept| {
        let same = (kept.0, &kept.1, kept.2.key, kept.2.digest)
            == (later.0, &later.1, later.2.key, later.2.digest);
        if same {
            kept.2.flags |= later.2.flags;
        }
        same
    });
    let mut groups: Vec<Group> = Vec::new();
    for (class, file, unit) in sorted {
        match groups.last_mut() {
            Some(g) if g.class == class && g.file == file => {
                g.units.as_mut().expect("built with units").push(unit);
            }
            _ => groups.push(Group {
                class,
                file,
                count: 0,
                indeterminate: 0,
                digest: Digest([0; 32]),
                units: Some(vec![unit]),
            }),
        }
    }
    for g in &mut groups {
        let units = g.units.as_ref().expect("built with units");
        g.count = units.len() as u64;
        g.indeterminate = units.iter().filter(|u| u.is_indeterminate()).count() as u64;
        g.digest = group_digest(units);
    }
    Digests { scheme: SCHEME, tu: tu_digest(&groups), groups }
}

fn exemptions(rng: &mut Rng, keys: &BTreeSet<Digest>) -> Exemptions {
    let mut lists: [Vec<Digest>; 3] = Default::default();
    for k in keys {
        for list in &mut lists {
            if rng.chance(25) {
                list.push(*k);
            }
        }
    }
    let [changed, created, removed] = lists;
    Exemptions::new(changed, created, removed).expect("built sorted")
}

fn id(g: &Group) -> (GroupClass, &[u8]) {
    (g.class, &g.file)
}

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

type Id<'a> = (GroupClass, &'a [u8]);

/// Coverage only: `derived_indeterminate` keys on one side, removed/added
/// findings for keys both images have, and step-4 findings.
fn coverage(pre: &Digests, post: &Digests, full: &Comparison, cov: &mut Coverage) {
    let keys_of = |d: &Digests, derived: bool| -> BTreeSet<Digest> {
        d.groups
            .iter()
            .filter(|gr| (gr.class == GroupClass::Derived) == derived)
            .flat_map(|gr| gr.units.as_deref().unwrap_or_default())
            .map(|u| u.key)
            .collect()
    };
    let (vp, vq) = (keys_of(pre, true), keys_of(post, true));
    let (dp, dq) = (keys_of(pre, false), keys_of(post, false));
    let has = |set: &BTreeSet<Digest>, k: &Option<Digest>| k.is_some_and(|k| set.contains(&k));
    cov.derived_indeterminate_one_sided += u64::from(full.findings.iter().any(|f| {
        f.kind == FindingKind::DerivedIndeterminate && has(&vp, &f.key) != has(&vq, &f.key)
    }));
    cov.one_sided_kind_on_shared_key += u64::from(full.findings.iter().any(|f| {
        matches!(f.kind, FindingKind::Removed | FindingKind::Added)
            && has(&dp, &f.key)
            && has(&dq, &f.key)
    }));
    let step4: Vec<Digest> = full
        .findings
        .iter()
        .filter(|f| f.kind == FindingKind::Changed && has(&dp, &f.key) != has(&dq, &f.key))
        .filter_map(|f| f.key)
        .collect();
    cov.step4 += u64::from(!step4.is_empty());
    cov.v_on_both_sides += u64::from(full.findings.iter().any(|f| match f.kind {
        FindingKind::Removed => has(&vq, &f.key),
        FindingKind::Added => has(&vp, &f.key),
        _ => false,
    }));
}

/// The property of §4.1 for one pair; `Err` says what failed.
fn check(pre: &Digests, post: &Digests, ex: &Exemptions, cov: &mut Coverage) -> Result<(), String> {
    comparable(pre, post)?;
    let all_pre: Vec<&Group> = pre.groups.iter().collect();
    let all_post: Vec<&Group> = post.groups.iter().collect();
    let full = compare_groups(&all_pre, &all_post, ex).map_err(|e| e.to_string())?;
    for f in &full.findings {
        cov.findings[kind_index(f.kind)] += 1;
    }
    // §6: the verdict of every comparison is valid under slip_model's rules.
    let shard = Shard {
        observer: dg(1),
        phase: Phase::Verify,
        round: 1,
        outcome: Outcome { status: Status::Ok, errors: 0, warnings: 0, message: String::new() },
        inputs: None,
        counters: vec![],
        digests: Some(post.clone()),
        verdict: Some(verdict(full.clone(), dg(2), dg(3), 8)),
        edits: None,
        binding: None,
        rule_use: None,
        spellings: None,
        names: Some(slip_model::names::Names::default()),
    };
    shard.validate()?;
    cov.used += u64::from(!full.used.is_empty());
    cov.only_pre += u64::from(full.only_pre > 0);
    cov.only_post += u64::from(full.only_post > 0);

    // G, computed independently of at_sums: the ids whose sum differs, a
    // group in one image only included, and the ids with an indeterminate
    // unit in either image.
    let ids: BTreeSet<Id<'_>> = pre.groups.iter().chain(&post.groups).map(id).collect();
    let find = |d: &'_ Digests, i: Id<'_>| {
        d.groups.iter().find(|g| id(g) == i).map(|g| (g.count, g.indeterminate, g.digest))
    };
    let flagged = |d: &'_ Digests, i: Id<'_>| find(d, i).is_some_and(|(_, n, _)| n > 0);
    let g: Vec<(GroupClass, Vec<u8>)> = ids
        .iter()
        .filter(|&&i| find(pre, i) != find(post, i) || flagged(pre, i) || flagged(post, i))
        .map(|&(c, f)| (c, f.to_vec()))
        .collect();
    cov.indeterminate_unchanged +=
        u64::from(ids.iter().any(|&i| find(pre, i) == find(post, i) && flagged(pre, i)));

    coverage(pre, post, &full, cov);

    match at_sums(pre, post) {
        AtSums::Equal => {
            cov.equal += 1;
            if !g.is_empty() {
                return Err(format!("at_sums: Equal, but G is {g:?}"));
            }
            if pre.groups != post.groups {
                return Err("at_sums: Equal, but the groups differ".into());
            }
            if full
                != (Comparison {
                    findings: vec![],
                    only_pre: 0,
                    only_post: 0,
                    used: vec![],
                    notes: vec![],
                })
            {
                return Err(format!("at_sums: Equal, but the full comparison is {full:?}"));
            }
            Ok(())
        }
        AtSums::Differing(differing) => {
            if g.is_empty() {
                return Err(
                    "at_sums: Differing, but every sum is equal and none indeterminate".into()
                );
            }
            if differing != g {
                return Err(format!("at_sums: {differing:?}, expected {g:?}"));
            }
            cov.equal_tu_differing += u64::from(pre.tu == post.tu);
            let in_g = |gr: &&Group| g.iter().any(|(c, f)| (*c, f.as_slice()) == id(gr));
            let sel_pre: Vec<&Group> = pre.groups.iter().filter(in_g).collect();
            let sel_post: Vec<&Group> = post.groups.iter().filter(in_g).collect();
            let narrow = compare_groups(&sel_pre, &sel_post, ex).map_err(|e| e.to_string())?;

            let units = |groups: &mut dyn Iterator<Item = &Group>| -> BTreeSet<Digest> {
                groups
                    .flat_map(|gr| gr.units.as_deref().unwrap_or_default())
                    .map(|u| u.key)
                    .collect()
            };
            let outside = units(&mut pre.groups.iter().filter(|gr| !in_g(gr)));
            let inside = units(&mut sel_pre.iter().chain(&sel_post).copied());
            cov.straddling += u64::from(inside.intersection(&outside).next().is_some());

            if narrow != full {
                return Err(format!("through groups {narrow:?}\nover all units {full:?}"));
            }
            Ok(())
        }
    }
}

// §4.1: through groups equals over all units, for generated pairs.
#[test]
fn s4_1_through_groups_equals_over_all_units() {
    let seed = std::env::var("SLIP_VERIFY_SEED")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|&s| s != 0)
        .unwrap_or(SEED);
    let mut rng = Rng(seed);
    let mut cov = Coverage::default();
    for case in 0..CASES {
        let pre_img = base(&mut rng);
        let mut post_img = pre_img.clone();
        let mut next_key = 1000;
        for _ in 0..rng.below(4) {
            edit(&mut rng, &mut post_img, &mut next_key, &mut cov);
        }
        let (pre, post) = (digests(&pre_img), digests(&post_img));
        let keys: BTreeSet<Digest> =
            pre_img.iter().chain(&post_img).map(|(_, _, u)| u.key).collect();
        let ex = exemptions(&mut rng, &keys);
        if let Err(why) = check(&pre, &post, &ex, &mut cov) {
            panic!(
                "§4.1 property failed: seed {seed:#x}, case {case}: {why}\n\
                 pre: {pre:#?}\npost: {post:#?}\nexemptions: {ex:?}"
            );
        }
    }
    // The generator really exercises what the brief lists.
    let msg = format!(
        "seed {seed:#x}, {CASES} pairs: edits {:?}, equal {}, equal tu but differing {}, \
         straddling G {}, indeterminate in an unchanged group {}, findings {:?}, used {}, \
         only_pre {}, only_post {}, one-sided derived_indeterminate {}, \
         removed/added on a key both have {}, step 4 {} (a derived unit of the key on both sides {})",
        cov.edits,
        cov.equal,
        cov.equal_tu_differing,
        cov.straddling,
        cov.indeterminate_unchanged,
        cov.findings,
        cov.used,
        cov.only_pre,
        cov.only_post,
        cov.derived_indeterminate_one_sided,
        cov.one_sided_kind_on_shared_key,
        cov.step4,
        cov.v_on_both_sides,
    );
    eprintln!("groups.rs coverage: {msg}");
    assert!(cov.edits.iter().all(|&n| n > 500), "{msg}");
    assert!(cov.findings.iter().all(|&n| n > 50), "{msg}");
    assert!(cov.equal > 1000 && cov.used > 500, "{msg}");
    assert!(cov.only_pre > 200 && cov.only_post > 200, "{msg}");
    assert!(cov.straddling > 200 && cov.indeterminate_unchanged > 200, "{msg}");
    assert!(cov.equal_tu_differing > 20 && cov.derived_indeterminate_one_sided > 20, "{msg}");
    assert!(cov.one_sided_kind_on_shared_key > 20, "{msg}");
    assert!(cov.step4 > 200 && cov.v_on_both_sides > 20, "{msg}");
}

// ---------------------------------------------------------------------------
// §4 step 4 through groups

fn unit(k: u64, d: u64, flags: u64) -> Unit {
    Unit { key: key(k), digest: dg(d), flags }
}

/// An image from its units, each with its group.
fn image(units: &[(GroupClass, &[u8], Unit)]) -> Digests {
    digests(&units.iter().map(|(c, f, u)| (*c, f.to_vec(), *u)).collect())
}

/// §4.1 for one pair, through `G` and over all groups: they must be equal;
/// returns the comparison and `G`.
fn through_groups(
    pre: &Digests,
    post: &Digests,
    ex: &Exemptions,
) -> (Comparison, Vec<Id<'static>>) {
    comparable(pre, post).expect("comparable");
    let AtSums::Differing(g) = at_sums(pre, post) else { panic!("at_sums: Equal") };
    let in_g = |gr: &&Group| g.iter().any(|(c, f)| (*c, f.as_slice()) == id(gr));
    let sel_pre: Vec<&Group> = pre.groups.iter().filter(in_g).collect();
    let sel_post: Vec<&Group> = post.groups.iter().filter(in_g).collect();
    let narrow = compare_groups(&sel_pre, &sel_post, ex).expect("well-formed");
    let all_pre: Vec<&Group> = pre.groups.iter().collect();
    let all_post: Vec<&Group> = post.groups.iter().collect();
    let full = compare_groups(&all_pre, &all_post, ex).expect("well-formed");
    assert_eq!(narrow, full, "§4.1: through G and over all groups");
    let ids = g
        .into_iter()
        .map(|(c, f)| {
            let f: &'static [u8] =
                FILES.iter().copied().find(|x| *x == f.as_slice()).unwrap_or(b"");
            (c, f)
        })
        .collect();
    (narrow, ids)
}

fn finding(kind: FindingKind, k: u64, file: &[u8], pre: Option<u64>, post: Option<u64>) -> Finding {
    Finding {
        kind,
        key: Some(key(k)),
        file: file.to_vec(),
        pre: pre.map(dg),
        post: post.map(dg),
        name: String::new(),
        location: None,
    }
}

// §4 step 4 through groups (§4.1): the declared unit of key 1 in a header's
// group (`a.h`), the derived unit of the same key in the translation
// unit's (`d.cpp`), in each direction. Both groups are in `G` (each holds a
// unit of the key that the other image lacks); the groups that are the same
// in both (`b.h`, the `tu` group) are not, and the comparison through `G` is
// the full one: `changed`, prevented by `changed` only, not counted.
#[test]
fn s4_4_through_groups() {
    let (ah, bh, dc) = (FILES[0], FILES[1], FILES[3]);
    let same: [(GroupClass, &[u8], Unit); 3] = [
        (GroupClass::Declared, bh, unit(5, 5, 0)),
        (GroupClass::Tu, b"", unit(6, 6, 0)),
        (GroupClass::Derived, dc, unit(7, 7, 0)),
    ];
    let with = |extra: &[(GroupClass, &[u8], Unit)]| {
        let mut v = same.to_vec();
        v.extend_from_slice(extra);
        image(&v)
    };
    let declared = with(&[(GroupClass::Declared, ah, unit(1, 1, 0))]);
    let derived = with(&[(GroupClass::Derived, dc, unit(1, 2, 0))]);
    let g_ids = vec![(GroupClass::Declared, ah), (GroupClass::Derived, dc)];
    let none = Exemptions::none();
    let only_changed = Exemptions::new(vec![key(1)], vec![], vec![]).unwrap();
    let not_changed = Exemptions::new(vec![], vec![key(1)], vec![key(1)]).unwrap();
    for (pre, post, dp, dq) in
        [(&declared, &derived, Some(1), None), (&derived, &declared, None, Some(1))]
    {
        let (cmp, ids) = through_groups(pre, post, &none);
        assert_eq!(ids, g_ids);
        let found = Comparison {
            findings: vec![finding(FindingKind::Changed, 1, ah, dp, dq)],
            only_pre: 0,
            only_post: 0,
            used: vec![],
            notes: vec![],
        };
        assert_eq!(cmp, found);
        assert_eq!(through_groups(pre, post, &not_changed).0, found);
        let (cmp, _) = through_groups(pre, post, &only_changed);
        assert_eq!(cmp, Comparison { findings: vec![], used: vec![key(1)], ..found.clone() });
    }
    // The derived unit indeterminate: its group is in `G` for that reason
    // too; `derived_indeterminate` beside the `changed`.
    let derived = with(&[(GroupClass::Derived, dc, unit(1, 2, Unit::INDETERMINATE))]);
    let (cmp, ids) = through_groups(&declared, &derived, &none);
    assert_eq!(ids, g_ids);
    assert_eq!(
        cmp.findings,
        vec![
            finding(FindingKind::Changed, 1, ah, Some(1), None),
            finding(FindingKind::DerivedIndeterminate, 1, dc, None, Some(2)),
        ]
    );
    // A multiplicity: the derived unit in two groups of the post-image, one
    // of them a header's that the pre-image has unchanged for another key.
    let declared = with(&[
        (GroupClass::Declared, ah, unit(1, 1, 0)),
        (GroupClass::Derived, bh, unit(8, 8, 0)),
    ]);
    let derived = with(&[
        (GroupClass::Derived, dc, unit(1, 2, 0)),
        (GroupClass::Derived, bh, unit(8, 8, 0)),
        (GroupClass::Derived, bh, unit(1, 2, 0)),
    ]);
    let (cmp, ids) = through_groups(&declared, &derived, &none);
    assert_eq!(
        ids,
        vec![(GroupClass::Declared, ah), (GroupClass::Derived, bh), (GroupClass::Derived, dc)]
    );
    assert_eq!(cmp.findings, vec![finding(FindingKind::Changed, 1, ah, Some(1), None)]);
    assert_eq!((cmp.only_pre, cmp.only_post), (0, 0));
}
