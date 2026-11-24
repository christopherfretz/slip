//! The comparison of two digests under declared exemptions
//! (`spec/verify.md` §3, §4, §4.1) and the `Verdict` it gives (§6).
//!
//! Pure functions over `slip_model::digests` and `slip_model::verdict`.
//! This is trusted base: every rule below is a sentence of §4, and the
//! preconditions on its inputs are those of §4.2.

use std::cmp::Ordering;
use std::fmt;

use slip_model::Digest;
use slip_model::declared::Declared;
use slip_model::digests::{self, Digests, Group, GroupClass, Unit, group_digest, tu_digest};
use slip_model::names::NamesEntry;
use slip_model::verdict::{Finding, FindingKind, Verdict, VerdictKind};

/// Longest `Verdict::reason`, in bytes (`spec/verify.md` §6, `spec/model.md` §8).
pub const REASON_MAX: usize = 4096;

/// The reason written when `unverifiable` is given an empty one: an
/// `unverifiable` verdict always says why.
pub const NO_REASON: &str = "no reason given";

// ---------------------------------------------------------------------------
// Exemptions (§2.2)

/// The exemption lists of the declared changes (§2, §2.2), by mapped key.
/// Each list is strictly ascending; a key may be in more than one list.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Exemptions {
    changed: Vec<Digest>,
    created: Vec<Digest>,
    removed: Vec<Digest>,
}

impl Exemptions {
    /// No exemption: the claim is plain equality (§2).
    pub fn none() -> Exemptions {
        Exemptions::default()
    }

    /// Fails if a list is not strictly ascending (unsorted, or two equal).
    pub fn new(
        changed: Vec<Digest>,
        created: Vec<Digest>,
        removed: Vec<Digest>,
    ) -> Result<Exemptions, String> {
        for (name, list) in [("changed", &changed), ("created", &created), ("removed", &removed)] {
            if let Some(i) = (1..list.len()).find(|&i| list[i - 1] >= list[i]) {
                return Err(format!("exemptions `{name}` are not strictly ascending at index {i}"));
            }
        }
        Ok(Exemptions { changed, created, removed })
    }

    /// The exemption lists of `declared` (§2.2), which are already
    /// strictly ascending. Its rules are not exemptions: they are applied
    /// by the walk of the post-image (§2.1).
    pub fn from_declared(declared: &Declared) -> Exemptions {
        Exemptions {
            changed: declared.changed().to_vec(),
            created: declared.created().to_vec(),
            removed: declared.removed().to_vec(),
        }
    }

    pub fn changed(&self) -> &[Digest] {
        &self.changed
    }

    pub fn created(&self) -> &[Digest] {
        &self.created
    }

    pub fn removed(&self) -> &[Digest] {
        &self.removed
    }

    /// The list that may prevent a finding of `kind`, if any (§4 step 2).
    /// Only `changed`, `removed` and `added` can be prevented.
    fn list_for(&self, kind: FindingKind) -> Option<&[Digest]> {
        match kind {
            FindingKind::Changed => Some(&self.changed),
            FindingKind::Removed => Some(&self.removed),
            FindingKind::Added => Some(&self.created),
            FindingKind::Indeterminate
            | FindingKind::DerivedChanged
            | FindingKind::DerivedOnlyPre
            | FindingKind::DerivedOnlyPost
            | FindingKind::DerivedIndeterminate => None,
        }
    }
}

// ---------------------------------------------------------------------------
// §3 "Shards"

/// Why `pre` and `post` cannot be compared, if they cannot: each must have
/// the scheme this build implements (`digests::SCHEME`, §3), its groups
/// strictly ascending by `(class, file)` and its `tu` equal to
/// `tu_digest(groups)` (`digest.md` §11), since §4.1 step 1 trusts `tu`.
pub fn comparable(pre: &Digests, post: &Digests) -> Result<(), String> {
    for (image, d) in [("pre", pre), ("post", post)] {
        if d.scheme != digests::SCHEME {
            return Err(format!(
                "the {image}-image digests have scheme {}; this build implements scheme {}",
                d.scheme,
                digests::SCHEME
            ));
        }
    }
    for (image, d) in [("pre", pre), ("post", post)] {
        if let Some(i) = (1..d.groups.len())
            .find(|&i| group_id(&d.groups[i - 1]).cmp(&group_id(&d.groups[i])) != Ordering::Less)
        {
            return Err(format!(
                "the {image}-image groups are not strictly ascending by (class, file) at index {i}"
            ));
        }
        if d.tu != tu_digest(&d.groups) {
            return Err(format!("the {image}-image tu digest is not the digest of its groups"));
        }
    }
    Ok(())
}

fn group_id(g: &Group) -> (GroupClass, &[u8]) {
    (g.class, &g.file)
}

// ---------------------------------------------------------------------------
// §4.1: group sums

/// §4.1 step 1 and the definition of `G`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AtSums {
    /// Equal `tu` digests and no group with an indeterminate unit in either
    /// image: the observer is verified (§4.1 step 1).
    Equal,
    /// `G` (§4.1 step 2): the groups, by `(class, file)`, whose sum is not
    /// the same in both images, a group in one image only included, and
    /// every group of either image with an indeterminate unit; ascending,
    /// no two equal. Returned also when the `tu` digests are equal.
    Differing(Vec<(GroupClass, Vec<u8>)>),
}

/// Compares group sums. Trusts `tu` (which `comparable` checks), and does
/// not rely on `groups` being sorted: an id is in `Differing` when its
/// sums `(count, indeterminate, digest)` in one image are not those in the
/// other, so an id given twice in one image only is reported (and
/// `compare_groups` rejects the duplicate) rather than hidden; and when a
/// group of that id in either image has `indeterminate > 0`.
pub fn at_sums(pre: &Digests, post: &Digests) -> AtSums {
    let flagged = |d: &Digests| d.groups.iter().any(|g| g.indeterminate > 0);
    if pre.tu == post.tu && !flagged(pre) && !flagged(post) {
        return AtSums::Equal;
    }
    let (before, after) = (by_sum(pre), by_sum(post));
    let mut out: Vec<(GroupClass, Vec<u8>)> = Vec::new();
    runs(
        &before,
        &after,
        |g| group_id(g),
        |(class, file), here, there| {
            let sum = |g: &&Group| (g.count, g.indeterminate, g.digest);
            let differs = !here.iter().map(sum).eq(there.iter().map(sum));
            let indeterminate = here.iter().chain(there).any(|g| g.indeterminate > 0);
            if differs || indeterminate {
                out.push((class, file.to_vec()));
            }
        },
    );
    AtSums::Differing(out)
}

/// The groups of `d`, sorted by `(class, file)` and then by sum.
fn by_sum(d: &Digests) -> Vec<&Group> {
    let mut v: Vec<&Group> = d.groups.iter().collect();
    v.sort_unstable_by(|a, b| {
        (a.class, &a.file, a.count, a.indeterminate, a.digest).cmp(&(
            b.class,
            &b.file,
            b.count,
            b.indeterminate,
            b.digest,
        ))
    });
    v
}

/// Walks two lists sorted by `id` and calls `f` once per id in either,
/// with the run of that id in each (either run may be empty, not both).
/// Linear in the lengths of the lists.
fn runs<'t, T, K: Ord>(
    before: &'t [T],
    after: &'t [T],
    id: impl Fn(&'t T) -> K,
    mut f: impl FnMut(K, &'t [T], &'t [T]),
) {
    let (mut at_before, mut at_after) = (0, 0);
    loop {
        let next = match (before.get(at_before), after.get(at_after)) {
            (Some(x), Some(y)) => std::cmp::min(id(x), id(y)),
            (Some(x), None) => id(x),
            (None, Some(y)) => id(y),
            (None, None) => return,
        };
        let start_before = at_before;
        while before.get(at_before).is_some_and(|x| id(x) == next) {
            at_before += 1;
        }
        let start_after = at_after;
        while after.get(at_after).is_some_and(|y| id(y) == next) {
            at_after += 1;
        }
        f(next, &before[start_before..at_before], &after[start_after..at_after]);
    }
}

// ---------------------------------------------------------------------------
// §4: the comparison

/// The outcome of §4 over the groups given.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Comparison {
    /// Every finding, ordered by `(rank, file, key)` (§6, `FindingKind::rank`).
    pub findings: Vec<Finding>,
    /// Derived keys only the pre-image has (§4 step 3).
    pub only_pre: u64,
    /// Derived keys only the post-image has.
    pub only_post: u64,
    /// Exemptions that prevented a finding; ascending, no two equal.
    pub used: Vec<Digest>,
    /// The keys of the notes `names_changed` (§4.4, WP-112): ascending,
    /// no two equal. [`compare_groups`] leaves it empty (it sees no names
    /// digest); the caller sets it from [`names_changed`]. Not a finding:
    /// [`verdict`] does not read it.
    pub notes: Vec<Digest>,
}

/// A group given to `compare_groups` that breaks a precondition. `post`
/// says which image it came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompareError {
    /// The group has no unit list.
    MissingUnits { class: GroupClass, file: Vec<u8>, post: bool },
    /// The unit list is not strictly ascending by `(key, digest)` (unsorted,
    /// or two units with one key and digest, whatever their flags).
    UnitsNotAscending { class: GroupClass, file: Vec<u8>, post: bool },
    /// A unit has a flag bit other than bit 0 set (`digest.md` §11).
    BadFlags { class: GroupClass, file: Vec<u8>, post: bool },
    /// `count` is not the length of the unit list.
    CountMismatch { class: GroupClass, file: Vec<u8>, post: bool },
    /// `indeterminate` is not the number of units with the indeterminate flag.
    IndeterminateMismatch { class: GroupClass, file: Vec<u8>, post: bool },
    /// `digest` is not `group_digest` of the unit list.
    DigestMismatch { class: GroupClass, file: Vec<u8>, post: bool },
    /// Two groups given for one image have the same `(class, file)`.
    DuplicateGroup { class: GroupClass, file: Vec<u8>, post: bool },
}

impl fmt::Display for CompareError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (what, class, file, post) = match self {
            CompareError::MissingUnits { class, file, post } => {
                ("has no unit list", class, file, post)
            }
            CompareError::UnitsNotAscending { class, file, post } => {
                ("has units that are not strictly ascending", class, file, post)
            }
            CompareError::BadFlags { class, file, post } => {
                ("has a unit with a flag other than indeterminate", class, file, post)
            }
            CompareError::CountMismatch { class, file, post } => {
                ("has a count that is not its number of units", class, file, post)
            }
            CompareError::IndeterminateMismatch { class, file, post } => {
                ("has an indeterminate that is not its number of flagged units", class, file, post)
            }
            CompareError::DigestMismatch { class, file, post } => {
                ("has a digest that is not the digest of its units", class, file, post)
            }
            CompareError::DuplicateGroup { class, file, post } => {
                ("is given twice", class, file, post)
            }
        };
        let image = if *post { "post" } else { "pre" };
        write!(
            f,
            "the {image}-image group ({}, {:?}) {what}",
            class_name(*class),
            String::from_utf8_lossy(file)
        )
    }
}

impl std::error::Error for CompareError {}

fn class_name(class: GroupClass) -> &'static str {
    match class {
        GroupClass::Declared => "declared",
        GroupClass::Derived => "derived",
        GroupClass::Tu => "tu",
    }
}

/// Checks the preconditions of `compare_groups` for the groups of one
/// image, in the order of `spec/verify.md` §4.2.
fn check_groups(groups: &[&Group], post: bool) -> Result<(), CompareError> {
    let mut ids: Vec<(GroupClass, &[u8])> = groups.iter().map(|g| group_id(g)).collect();
    ids.sort_unstable();
    if let Some(w) = ids.windows(2).find(|w| w[0] == w[1]) {
        return Err(CompareError::DuplicateGroup { class: w[0].0, file: w[0].1.to_vec(), post });
    }
    for g in groups {
        let (class, file) = (g.class, || g.file.clone());
        let Some(units) = &g.units else {
            return Err(CompareError::MissingUnits { class, file: file(), post });
        };
        if units.windows(2).any(|w| (w[0].key, w[0].digest) >= (w[1].key, w[1].digest)) {
            return Err(CompareError::UnitsNotAscending { class, file: file(), post });
        }
        if units.iter().any(|u| u.flags & !Unit::FLAGS != 0) {
            return Err(CompareError::BadFlags { class, file: file(), post });
        }
        if u64::try_from(units.len()).ok() != Some(g.count) {
            return Err(CompareError::CountMismatch { class, file: file(), post });
        }
        let flagged = units.iter().filter(|u| u.is_indeterminate()).count();
        if u64::try_from(flagged).ok() != Some(g.indeterminate) {
            return Err(CompareError::IndeterminateMismatch { class, file: file(), post });
        }
        if group_digest(units) != g.digest {
            return Err(CompareError::DigestMismatch { class, file: file(), post });
        }
    }
    Ok(())
}

/// The unit list of a group `check_groups` accepted.
fn units_of(g: &Group) -> &[Unit] {
    match &g.units {
        Some(units) => units,
        None => unreachable!("compare_groups: a group without units passed check_groups"),
    }
}

/// One occurrence of a unit in the multiset of §3, with its group's id and
/// whether that group's units are the same in both images (§6 `file`).
#[derive(Clone, Copy)]
struct Occ<'a> {
    key: Digest,
    digest: Digest,
    indeterminate: bool,
    /// The unit is marked "emitted unreferenced" (flag bit 1, WP-155).
    emitted: bool,
    class: GroupClass,
    file: &'a [u8],
    /// The group's unit list is not the same in both of the images given
    /// (a group given for one image only included).
    differs: bool,
}

/// The declared multiset `D` (declared and `tu` groups) and the derived
/// multiset `V` of one image, each sorted by `(key, digest)`.
#[derive(Default)]
struct Multisets<'a> {
    declared: Vec<Occ<'a>>,
    derived: Vec<Occ<'a>>,
}

impl<'a> Multisets<'a> {
    fn add(&mut self, g: &'a Group, differs: bool) {
        // §3: declared and `tu` units form D; derived units form V.
        let target = match g.class {
            GroupClass::Declared | GroupClass::Tu => &mut self.declared,
            GroupClass::Derived => &mut self.derived,
        };
        target.extend(units_of(g).iter().map(|u| Occ {
            key: u.key,
            digest: u.digest,
            indeterminate: u.is_indeterminate(),
            emitted: u.is_emitted(),
            class: g.class,
            file: &g.file,
            differs,
        }));
    }

    fn sort(&mut self) {
        for v in [&mut self.declared, &mut self.derived] {
            v.sort_unstable_by_key(|o| (o.key, o.digest));
        }
    }
}

/// The multisets of both images, with each group paired with the group of
/// the same `(class, file)` in the other image to know whether it differs.
fn multisets<'a>(pre: &[&'a Group], post: &[&'a Group]) -> (Multisets<'a>, Multisets<'a>) {
    let sorted = |groups: &[&'a Group]| {
        let mut v: Vec<&'a Group> = groups.to_vec();
        v.sort_unstable_by(|a, b| group_id(a).cmp(&group_id(b)));
        v
    };
    let (before, after) = (sorted(pre), sorted(post));
    let (mut p, mut q) = (Multisets::default(), Multisets::default());
    runs(
        &before,
        &after,
        |g| group_id(g),
        |_, here, there| {
            // The mark is part of no digest and no sum (WP-155): a group
            // differs when its units do without it, as its sum does.
            let differs = match (here, there) {
                ([x], [y]) => !same_units(units_of(x), units_of(y)),
                _ => true,
            };
            for g in here {
                p.add(g, differs);
            }
            for g in there {
                q.add(g, differs);
            }
        },
    );
    p.sort();
    q.sort();
    (p, q)
}

/// Whether two unit lists are the same, the mark "emitted unreferenced"
/// left out (it is part of no digest, `digest.md` §11).
fn same_units(x: &[Unit], y: &[Unit]) -> bool {
    let strip = |u: &Unit| (u.key, u.digest, u.flags & !Unit::EMITTED);
    x.len() == y.len() && x.iter().map(strip).eq(y.iter().map(strip))
}

/// Whether a one-sided derived key is emitted (§4 step 3, WP-155): `run`
/// (the key's units in the image that has `only`, its difference) has a
/// marked unit, in a group that differs, whose digest is in `only`.
fn emitted(run: &[Occ<'_>], only: &[Digest]) -> bool {
    run.iter().any(|o| o.emitted && o.differs && only.contains(&o.digest))
}

/// `R(k) = P(k) − Q(k)` and `A(k) = Q(k) − P(k)` (§4 step 2), as multisets
/// of digests; each run is sorted by digest.
fn differences(p: &[Occ<'_>], q: &[Occ<'_>]) -> (Vec<Digest>, Vec<Digest>) {
    let (mut only_p, mut only_q) = (Vec::new(), Vec::new());
    let (mut at_p, mut at_q) = (0, 0);
    while let (Some(x), Some(y)) = (p.get(at_p), q.get(at_q)) {
        match x.digest.cmp(&y.digest) {
            Ordering::Less => {
                only_p.push(x.digest);
                at_p += 1;
            }
            Ordering::Greater => {
                only_q.push(y.digest);
                at_q += 1;
            }
            Ordering::Equal => {
                at_p += 1;
                at_q += 1;
            }
        }
    }
    only_p.extend(p[at_p..].iter().map(|o| o.digest));
    only_q.extend(q[at_q..].iter().map(|o| o.digest));
    (only_p, only_q)
}

/// The one element of `v`, counting multiplicity; else absent.
fn only<T: Copy>(v: &[T]) -> Option<T> {
    match v {
        [one] => Some(*one),
        _ => None,
    }
}

/// The file of the first occurrence, in `(class, file)` order, that `keep`
/// keeps.
fn first_file<'a>(run: &[Occ<'a>], keep: impl Fn(&Occ<'a>) -> bool) -> Option<&'a [u8]> {
    run.iter().filter(|o| keep(o)).map(|o| (o.class, o.file)).min().map(|(_, file)| file)
}

/// A finding of kind `changed`, `removed`, `added`, `derived_changed`,
/// `derived_only_pre` or `derived_only_post`
/// for `key` (§6): `pre`/`post` from `R(k)`/`A(k)` when it has exactly one
/// element; `file` from the first group that differs between the images
/// and holds a unit of the key, in the pre-image, else in the post-image.
fn difference(kind: FindingKind, key: Digest, p: &[Occ<'_>], q: &[Occ<'_>]) -> Finding {
    let (r, a) = differences(p, q);
    let file = first_file(p, |o| o.differs).or_else(|| first_file(q, |o| o.differs));
    let Some(file) = file else {
        unreachable!("§6: a key with a difference has a unit in a group that differs")
    };
    Finding {
        kind,
        key: Some(key),
        file: file.to_vec(),
        pre: only(&r),
        post: only(&a),
        name: String::new(),
        location: None,
    }
}

/// A finding of kind `indeterminate` or `derived_indeterminate` for `key`
/// (§6): `pre`/`post` the digest of the key's indeterminate unit in that
/// image when it has exactly one; `file` from the first group holding an
/// indeterminate unit of the key, in the pre-image, else in the post-image.
fn indeterminate(kind: FindingKind, key: Digest, p: &[Occ<'_>], q: &[Occ<'_>]) -> Finding {
    let flagged = |run: &[Occ<'_>]| -> Vec<Digest> {
        run.iter().filter(|o| o.indeterminate).map(|o| o.digest).collect()
    };
    let file = first_file(p, |o| o.indeterminate).or_else(|| first_file(q, |o| o.indeterminate));
    let Some(file) = file else {
        unreachable!("§6: an indeterminate key has an indeterminate unit")
    };
    Finding {
        kind,
        key: Some(key),
        file: file.to_vec(),
        pre: only(&flagged(p)),
        post: only(&flagged(q)),
        name: String::new(),
        location: None,
    }
}

/// §4 over the unit lists of the groups given: the groups of `G` from each
/// image (§4.1 step 2), or all of them; the result is the same (§4.1).
/// Steps 1 to 4: step 4 (a key declared in one image and derived in the
/// other) is read on the differences of `V`, as steps 2 and 3 are, so the
/// restriction to `G` stays exact.
/// Groups are paired across the images by `(class, file)`. Every group
/// must have its unit list, consistent with its `count`, `indeterminate`
/// and `digest`, and no `(class, file)` may be given twice for one image;
/// otherwise `Err`.
pub fn compare_groups(
    pre: &[&Group],
    post: &[&Group],
    exemptions: &Exemptions,
) -> Result<Comparison, CompareError> {
    check_groups(pre, false)?;
    check_groups(post, true)?;
    let (before, after) = multisets(pre, post);
    let mut findings = Vec::new();
    let mut used = Vec::new();

    // The derived multiset by key: each key of `Vp` or `Vq` with its run in
    // each, ascending by key. Step 4 asks it about keys of `D`, step 3
    // walks it.
    let mut derived: Vec<(Digest, &[Occ<'_>], &[Occ<'_>])> = Vec::new();
    runs(&before.derived, &after.derived, |o| o.key, |key, pk, qk| derived.push((key, pk, qk)));
    let derived_runs = |key: Digest| -> (&[Occ<'_>], &[Occ<'_>]) {
        match derived.binary_search_by_key(&key, |&(k, _, _)| k) {
            Ok(i) => (derived[i].1, derived[i].2),
            Err(_) => (&[], &[]),
        }
    };
    // The keys step 4 gave `changed` (or that `changed` prevented),
    // ascending: step 3 does not count them.
    let mut step4: Vec<Digest> = Vec::new();

    // §4 steps 1, 2 and 4, over D.
    runs(
        &before.declared,
        &after.declared,
        |o| o.key,
        |key, pk, qk| {
            // Step 1: an indeterminate unit on either side; no exemption applies.
            if pk.iter().chain(qk).any(|o| o.indeterminate) {
                findings.push(indeterminate(FindingKind::Indeterminate, key, pk, qk));
                return;
            }
            // Step 2, on the differences.
            let (r, a) = differences(pk, qk);
            let mut kind = match (r.is_empty(), a.is_empty()) {
                (true, true) => return,
                (false, false) => FindingKind::Changed,
                (false, true) => FindingKind::Removed,
                (true, false) => FindingKind::Added,
            };
            // Step 4: what one image declares and the other derives is in
            // both images, and not the same. "Has a unit in `Vq`" is read on
            // the differences of `V` (what `Vq` has of the key that `Vp` has
            // not), so that it too is unchanged by leaving out the groups
            // outside `G` (§4.1); on what the walk produces the two readings
            // are one, since a key with a determinate declared unit has no
            // derived unit in the same image (digest.md §3.4, §10; the two
            // runs `collides` does not mark are in spec/verify.md §4 step 4).
            if kind != FindingKind::Changed {
                let (vp, vq) = derived_runs(key);
                let (rv, av) = differences(vp, vq);
                let other_side_derives = match kind {
                    FindingKind::Removed => !av.is_empty(),
                    _ => !rv.is_empty(),
                };
                if other_side_derives {
                    kind = FindingKind::Changed;
                    step4.push(key);
                }
            }
            let exempt = exemptions.list_for(kind).is_some_and(|l| l.binary_search(&key).is_ok());
            if exempt {
                used.push(key);
            } else {
                findings.push(difference(kind, key, pk, qk));
            }
        },
    );

    // §4 step 3, over V. No exemption applies to a derived key.
    let (mut only_pre, mut only_post) = (0u64, 0u64);
    for &(key, pk, qk) in &derived {
        // An indeterminate unit on either side, also on one side only.
        if pk.iter().chain(qk).any(|o| o.indeterminate) {
            findings.push(indeterminate(FindingKind::DerivedIndeterminate, key, pk, qk));
            continue;
        }
        let (r, a) = differences(pk, qk);
        match (r.is_empty(), a.is_empty()) {
            (true, true) => {}
            (false, false) => {
                findings.push(difference(FindingKind::DerivedChanged, key, pk, qk));
            }
            // Step 4 took the key: it is a difference, not a count.
            _ if step4.binary_search(&key).is_ok() => {}
            // Emitted unreferenced (WP-155): a finding, once per key.
            (false, true) if emitted(pk, &r) => {
                findings.push(difference(FindingKind::DerivedOnlyPre, key, pk, qk));
            }
            (true, false) if emitted(qk, &a) => {
                findings.push(difference(FindingKind::DerivedOnlyPost, key, pk, qk));
            }
            (false, true) => only_pre += 1,
            (true, false) => only_post += 1,
        }
    }

    findings.sort_by(|a, b| (a.kind.rank(), &a.file, a.key).cmp(&(b.kind.rank(), &b.file, b.key)));
    // Each key is visited once in D, so `used` has no two equal; it is in
    // key order already, sorted again to make that independent of `runs`.
    used.sort_unstable();
    used.dedup();
    Ok(Comparison { findings, only_pre, only_post, used, notes: Vec::new() })
}

/// §4.4 (WP-112): the keys that have a names digest in both images and
/// whose names digests differ, ascending. A key with a names digest in one
/// image only gives nothing. `pre` and `post` are a `names` section's
/// `units`: strictly ascending by key (`spec/model.md` §8.7).
pub fn names_changed(pre: &[NamesEntry], post: &[NamesEntry]) -> Vec<Digest> {
    let mut out = Vec::new();
    let (mut i, mut j) = (0, 0);
    while let (Some(p), Some(q)) = (pre.get(i), post.get(j)) {
        match p.key.cmp(&q.key) {
            Ordering::Less => i += 1,
            Ordering::Greater => j += 1,
            Ordering::Equal => {
                if p.names != q.names {
                    out.push(p.key);
                }
                i += 1;
                j += 1;
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// §6: verdicts

/// The verdict of a comparison: `mismatch` if it has a finding, else
/// `verified`; `findings` cut to the first `limit`.
pub fn verdict(comparison: Comparison, pre_shard: Digest, rules: Digest, limit: usize) -> Verdict {
    // The notes are not findings (§4.4): they take no part in the verdict.
    let Comparison { mut findings, only_pre, only_post, used, notes: _ } = comparison;
    let total = findings.len() as u64;
    let kind = if findings.is_empty() { VerdictKind::Verified } else { VerdictKind::Mismatch };
    findings.truncate(limit);
    Verdict {
        kind,
        pre: pre_shard,
        rules,
        reason: String::new(),
        only_pre,
        only_post,
        used,
        total,
        findings,
    }
}

/// The verdict when `at_sums` is `Equal` (§4.1 step 1): `verified`, with
/// no finding, no count and nothing used.
pub fn verified(pre_shard: Digest, rules: Digest) -> Verdict {
    Verdict {
        kind: VerdictKind::Verified,
        pre: pre_shard,
        rules,
        reason: String::new(),
        only_pre: 0,
        only_post: 0,
        used: Vec::new(),
        total: 0,
        findings: Vec::new(),
    }
}

/// An `unverifiable` verdict. `reason` is cut to at most `REASON_MAX`
/// bytes at a character boundary; an empty one becomes `NO_REASON`.
pub fn unverifiable(reason: &str, pre_shard: Digest, rules: Digest) -> Verdict {
    let mut end = reason.len().min(REASON_MAX);
    while !reason.is_char_boundary(end) {
        end -= 1;
    }
    let reason = if end == 0 { NO_REASON } else { &reason[..end] };
    Verdict {
        kind: VerdictKind::Unverifiable,
        pre: pre_shard,
        rules,
        reason: reason.to_owned(),
        only_pre: 0,
        only_post: 0,
        used: Vec::new(),
        total: 0,
        findings: Vec::new(),
    }
}
