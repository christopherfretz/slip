//! `spec/verify.md` §4.1: comparing through group sums against comparing
//! all units, over generated pairs of images.
//!
//! The generator is a fixed-seed xorshift (no dependency); a failure prints
//! the seed and case, and `SLIP_VERIFY_SEED=<n>` reruns with another seed.
//!
//! §4.1 says restricting §4 to the groups `G` whose sums differ is exact.
//! That holds when no key has a unit in a group outside `G` and no unit
//! outside `G` is indeterminate; it does not hold otherwise (the tests at
//! the end of this file, and `spec/landed/verify/WP-016.md` "Open
//! questions"). The property checked is therefore: exact equality in the
//! first case, and in every case equality on the keys that have no unit
//! outside `G`.

use std::collections::BTreeSet;

use slip_model::Digest;
use slip_model::digests::{Digests, Group, GroupClass, SCHEME, Unit, group_digest, tu_digest};
use slip_model::verdict::{Finding, FindingKind};
use slip_verify::{AtSums, Comparison, Exemptions, at_sums, comparable, compare_groups};

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

/// What edits were made, for the coverage counters.
#[derive(Default)]
struct Coverage {
    edits: [u64; 7],
    equal: u64,
    exact: u64,
    partial: u64,
    divergent: u64,
    /// Divergent with an indeterminate unit outside G, and with a key
    /// straddling G (both can hold).
    divergent_indeterminate: u64,
    divergent_straddling: u64,
    findings: [u64; 6],
    used: u64,
    only: u64,
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
    sorted.dedup(); // two units of one group with one key and digest are one
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
                digest: Digest([0; 32]),
                units: Some(vec![unit]),
            }),
        }
    }
    for g in &mut groups {
        let units = g.units.as_ref().expect("built with units");
        g.count = units.len() as u64;
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

fn restrict(c: &Comparison, keep: impl Fn(&Digest) -> bool) -> (Vec<Finding>, Vec<Digest>) {
    let findings =
        c.findings.iter().filter(|f| f.key.as_ref().is_some_and(&keep)).cloned().collect();
    let used = c.used.iter().copied().filter(|k| keep(k)).collect();
    (findings, used)
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
    cov.used += u64::from(!full.used.is_empty());
    cov.only += u64::from(full.only_pre + full.only_post > 0);

    // G, computed independently of at_sums: the ids whose group differs.
    let ids: BTreeSet<(GroupClass, &[u8])> =
        pre.groups.iter().chain(&post.groups).map(id).collect();
    let find = |d: &'_ Digests, i: (GroupClass, &[u8])| {
        d.groups.iter().find(|g| id(g) == i).map(|g| (g.count, g.digest))
    };
    let g: Vec<(GroupClass, Vec<u8>)> = ids
        .iter()
        .filter(|&&i| find(pre, i) != find(post, i))
        .map(|&(c, f)| (c, f.to_vec()))
        .collect();

    match at_sums(pre, post) {
        AtSums::Equal => {
            cov.equal += 1;
            if !g.is_empty() || pre.groups != post.groups {
                return Err("at_sums: Equal, but groups differ".into());
            }
            // Equal multisets: only step 1 and its derived counterpart can find.
            let only_indeterminate = full.findings.iter().all(|f| match f.kind {
                FindingKind::Indeterminate | FindingKind::DerivedIndeterminate => true,
                FindingKind::Changed
                | FindingKind::Removed
                | FindingKind::Added
                | FindingKind::DerivedChanged => false,
            });
            if !only_indeterminate || full.only_pre != 0 || full.only_post != 0 {
                return Err(format!("Equal sums, but the full comparison found {full:?}"));
            }
            if !full.used.is_empty() {
                return Err("Equal sums, but an exemption was used".into());
            }
            Ok(())
        }
        AtSums::Differing(differing) => {
            if differing != g {
                return Err(format!("at_sums: {differing:?}, expected {g:?}"));
            }
            let in_g = |gr: &&Group| g.iter().any(|(c, f)| (*c, f.as_slice()) == id(gr));
            let sel_pre: Vec<&Group> = pre.groups.iter().filter(in_g).collect();
            let sel_post: Vec<&Group> = post.groups.iter().filter(in_g).collect();
            let narrow = compare_groups(&sel_pre, &sel_post, ex).map_err(|e| e.to_string())?;

            // Units outside G (equal in both images).
            let outside: Vec<&Unit> = pre
                .groups
                .iter()
                .filter(|gr| !in_g(gr))
                .flat_map(|gr| gr.units.as_deref().unwrap_or_default())
                .collect();
            let touched: BTreeSet<Digest> = outside.iter().map(|u| u.key).collect();
            // The condition under which §4.1's restriction is exact: no
            // indeterminate unit outside G, and no key with units both
            // outside and inside G.
            let inside_g = sel_pre
                .iter()
                .chain(&sel_post)
                .flat_map(|gr| gr.units.as_deref().unwrap_or_default());
            let straddling = inside_g.clone().any(|u| touched.contains(&u.key));
            if outside.iter().all(|u| !u.is_indeterminate()) && !straddling {
                cov.exact += 1;
                if narrow != full {
                    return Err(format!("through groups {narrow:?}\nover all units {full:?}"));
                }
            } else {
                cov.partial += 1;
                if narrow != full {
                    cov.divergent += 1;
                    cov.divergent_indeterminate +=
                        u64::from(outside.iter().any(|u| u.is_indeterminate()));
                    cov.divergent_straddling += u64::from(straddling);
                }
            }
            // In every case: equal on the keys with no unit outside G.
            let keep = |k: &Digest| !touched.contains(k);
            if restrict(&narrow, keep) != restrict(&full, keep) {
                return Err(format!(
                    "on untouched keys: through groups {narrow:?}\nover all units {full:?}"
                ));
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
        "edits {:?}, equal {}, exact {}, partial {} (divergent {}: indeterminate outside G {}, \
         straddling G {}), findings {:?}, used {}, one-sided derived {}",
        cov.edits,
        cov.equal,
        cov.exact,
        cov.partial,
        cov.divergent,
        cov.divergent_indeterminate,
        cov.divergent_straddling,
        cov.findings,
        cov.used,
        cov.only
    );
    eprintln!("groups.rs coverage: {msg}");
    assert!(cov.edits.iter().all(|&n| n > 500), "{msg}");
    assert!(cov.findings.iter().all(|&n| n > 50), "{msg}");
    assert!(cov.equal > 1000 && cov.exact > 5000 && cov.used > 500 && cov.only > 200, "{msg}");
}

// ---------------------------------------------------------------------------
// Where §4.1's exactness does not hold (open questions of
// spec/landed/verify/WP-016.md). Each test pins what the comparison does
// with the spec as written, so that a change of the spec shows up here.

fn unit(k: u64, d: u64, flags: u64) -> Unit {
    Unit { key: key(k), digest: dg(d), flags }
}

fn group(class: GroupClass, file: &[u8], units: &[Unit]) -> Group {
    let mut units = units.to_vec();
    units.sort_unstable();
    Group {
        class,
        file: file.to_vec(),
        count: units.len() as u64,
        digest: group_digest(&units),
        units: Some(units),
    }
}

fn through_groups(pre: &[Group], post: &[Group], ex: &Exemptions) -> (Comparison, Comparison) {
    let (p, q) = (
        Digests { scheme: SCHEME, tu: tu_digest(pre), groups: pre.to_vec() },
        Digests { scheme: SCHEME, tu: tu_digest(post), groups: post.to_vec() },
    );
    let AtSums::Differing(g) = at_sums(&p, &q) else { panic!("sums differ") };
    let pick = |d: &[Group]| -> Vec<Group> {
        d.iter()
            .filter(|gr| g.iter().any(|(c, f)| (*c, f) == (gr.class, &gr.file)))
            .cloned()
            .collect()
    };
    let (sp, sq) = (pick(pre), pick(post));
    let narrow =
        compare_groups(&sp.iter().collect::<Vec<_>>(), &sq.iter().collect::<Vec<_>>(), ex).unwrap();
    let full =
        compare_groups(&pre.iter().collect::<Vec<_>>(), &post.iter().collect::<Vec<_>>(), ex)
            .unwrap();
    (narrow, full)
}

// Open question 1: an indeterminate unit in a group outside G gives a step-1
// finding over all units, and none through groups.
#[test]
fn s4_1_open_indeterminate_outside_g_is_not_seen() {
    let a = group(GroupClass::Declared, b"/a.h", &[unit(1, 1, Unit::INDETERMINATE)]);
    let pre = [a.clone(), group(GroupClass::Declared, b"/b.h", &[unit(2, 1, 0)])];
    let post = [a, group(GroupClass::Declared, b"/b.h", &[unit(2, 2, 0)])];
    let (narrow, full) = through_groups(&pre, &post, &Exemptions::none());
    assert_eq!(narrow.findings.len(), 1);
    assert_eq!(narrow.findings[0].kind, FindingKind::Changed);
    assert_eq!(full.findings.len(), 2);
    assert_eq!(full.findings[1].kind, FindingKind::Indeterminate);
}

// Open question 2: a content-keyed unit in two files, one copy removed:
// `changed` (2 against 1) over all units, `removed` through groups, so a
// `removed` exemption prevents it only through groups.
#[test]
fn s4_1_open_content_keyed_copy_removed_changes_kind() {
    let a = group(GroupClass::Declared, b"/a.h", &[unit(1, 1, 0)]);
    let pre = [a.clone(), group(GroupClass::Declared, b"/b.h", &[unit(1, 1, 0), unit(2, 2, 0)])];
    let post = [a, group(GroupClass::Declared, b"/b.h", &[unit(2, 2, 0)])];
    let removed = Exemptions::new(vec![], vec![], vec![key(1)]).unwrap();
    let (narrow, full) = through_groups(&pre, &post, &removed);
    assert!(narrow.findings.is_empty());
    assert_eq!(narrow.used, vec![key(1)]);
    assert_eq!(full.findings.len(), 1);
    assert_eq!(full.findings[0].kind, FindingKind::Changed);
    assert!(full.used.is_empty());
}

// Open question 3: a derived key with a unit outside G and one inside G on
// one side only: `derived_changed` over all units, a one-sided count
// through groups.
#[test]
fn s4_1_open_derived_key_partly_outside_g_is_counted() {
    let a = group(GroupClass::Derived, b"/a.h", &[unit(1, 1, 0)]);
    let pre = [a.clone(), group(GroupClass::Derived, b"/b.h", &[unit(1, 1, 0)])];
    let post = [a, group(GroupClass::Derived, b"/b.h", &[unit(2, 2, 0)])];
    let (narrow, full) = through_groups(&pre, &post, &Exemptions::none());
    assert!(narrow.findings.is_empty());
    assert_eq!((narrow.only_pre, narrow.only_post), (1, 1));
    assert_eq!(full.findings.len(), 1);
    assert_eq!(full.findings[0].kind, FindingKind::DerivedChanged);
    assert_eq!((full.only_pre, full.only_post), (0, 1));
}
