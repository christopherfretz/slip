//! The comparison of two digests under declared exemptions
//! (`spec/verify.md` §3, §4, §4.1) and the `Verdict` it gives (§6).
//!
//! Pure functions over `slip_model::digests` and `slip_model::verdict`.
//! This is trusted base: every rule below is a sentence of §4, and the
//! behaviour on inputs the spec leaves open is the one written in
//! `spec/landed/verify/WP-016.md`.

use std::cmp::Ordering;
use std::fmt;

use slip_model::Digest;
use slip_model::digests::{self, Digests, Group, GroupClass, Unit, group_digest, tu_digest};
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
    /// Equal `tu` digests.
    Equal,
    /// The groups, by `(class, file)`, whose sum is not the same in both
    /// images, a group in one image only included; ascending, no two equal.
    Differing(Vec<(GroupClass, Vec<u8>)>),
}

/// Compares group sums. Trusts `tu` (which `comparable` checks), and does
/// not rely on `groups` being sorted: an id is in `Differing` when its
/// sums `(count, digest)` in one image are not those in the other, so an
/// id given twice in one image only is reported (and `compare_groups`
/// rejects the duplicate) rather than hidden.
pub fn at_sums(pre: &Digests, post: &Digests) -> AtSums {
    if pre.tu == post.tu {
        return AtSums::Equal;
    }
    let (before, after) = (by_sum(pre), by_sum(post));
    let mut out: Vec<(GroupClass, Vec<u8>)> = Vec::new();
    runs(&before, &after, |g| group_id(g), |(class, file), here, there| {
        let sum = |g: &&Group| (g.count, g.digest);
        if !here.iter().map(sum).eq(there.iter().map(sum)) {
            out.push((class, file.to_vec()));
        }
    });
    AtSums::Differing(out)
}

/// The groups of `d`, sorted by `(class, file, count, digest)`.
fn by_sum(d: &Digests) -> Vec<&Group> {
    let mut v: Vec<&Group> = d.groups.iter().collect();
    v.sort_unstable_by(|a, b| {
        (a.class, &a.file, a.count, a.digest).cmp(&(b.class, &b.file, b.count, b.digest))
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
    /// Every finding, ordered by `(kind, file, key)` (§6).
    pub findings: Vec<Finding>,
    /// Derived keys only the pre-image has (§4 step 3).
    pub only_pre: u64,
    /// Derived keys only the post-image has.
    pub only_post: u64,
    /// Exemptions that prevented a finding; ascending, no two equal.
    pub used: Vec<Digest>,
}

/// A group given to `compare_groups` that breaks a precondition. `post`
/// says which image it came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompareError {
    /// The group has no unit list.
    MissingUnits { class: GroupClass, file: Vec<u8>, post: bool },
    /// The unit list is not strictly ascending (unsorted, or two equal).
    UnitsNotAscending { class: GroupClass, file: Vec<u8>, post: bool },
    /// A unit has a flag bit other than bit 0 set (`digest.md` §11).
    BadFlags { class: GroupClass, file: Vec<u8>, post: bool },
    /// `count` is not the length of the unit list.
    CountMismatch { class: GroupClass, file: Vec<u8>, post: bool },
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

/// One occurrence of a unit in the multiset of §3, with the file of its group.
#[derive(Clone, Copy)]
struct Occ<'a> {
    key: Digest,
    digest: Digest,
    indeterminate: bool,
    file: &'a [u8],
}

/// The declared multiset `D` (declared and `tu` groups) and the derived
/// multiset `V` of one image, each sorted by `(key, digest)`.
struct Multisets<'a> {
    declared: Vec<Occ<'a>>,
    derived: Vec<Occ<'a>>,
}

fn multisets<'a>(groups: &[&'a Group], post: bool) -> Result<Multisets<'a>, CompareError> {
    let mut ids: Vec<(GroupClass, &[u8])> = groups.iter().map(|g| group_id(g)).collect();
    ids.sort_unstable();
    if let Some(w) = ids.windows(2).find(|w| w[0] == w[1]) {
        return Err(CompareError::DuplicateGroup { class: w[0].0, file: w[0].1.to_vec(), post });
    }
    let mut out = Multisets { declared: Vec::new(), derived: Vec::new() };
    for g in groups {
        let (class, file) = (g.class, || g.file.clone());
        let Some(units) = &g.units else {
            return Err(CompareError::MissingUnits { class, file: file(), post });
        };
        if units.windows(2).any(|w| w[0] >= w[1]) {
            return Err(CompareError::UnitsNotAscending { class, file: file(), post });
        }
        if units.iter().any(|u| u.flags & !Unit::INDETERMINATE != 0) {
            return Err(CompareError::BadFlags { class, file: file(), post });
        }
        if u64::try_from(units.len()).ok() != Some(g.count) {
            return Err(CompareError::CountMismatch { class, file: file(), post });
        }
        if group_digest(units) != g.digest {
            return Err(CompareError::DigestMismatch { class, file: file(), post });
        }
        // §3: declared and `tu` units form D; derived units form V.
        let target = match g.class {
            GroupClass::Declared | GroupClass::Tu => &mut out.declared,
            GroupClass::Derived => &mut out.derived,
        };
        target.extend(units.iter().map(|u| Occ {
            key: u.key,
            digest: u.digest,
            indeterminate: u.is_indeterminate(),
            file: &g.file,
        }));
    }
    for v in [&mut out.declared, &mut out.derived] {
        v.sort_unstable_by_key(|o| (o.key, o.digest));
    }
    Ok(out)
}

/// The digests of a key's run, as a multiset: the run is sorted by digest.
fn same_digests(p: &[Occ<'_>], q: &[Occ<'_>]) -> bool {
    p.iter().map(|o| o.digest).eq(q.iter().map(|o| o.digest))
}

/// A finding for `key` (§6): `file` is the smallest file of the pre-image
/// groups holding the key, else of the post-image ones; `pre` and `post`
/// are the digest where that image has exactly one occurrence of the key.
fn finding(kind: FindingKind, key: Digest, p: &[Occ<'_>], q: &[Occ<'_>]) -> Finding {
    let side = if p.is_empty() { q } else { p };
    let file = side.iter().map(|o| o.file).min().unwrap_or_default().to_vec();
    let only = |run: &[Occ<'_>]| match run {
        [one] => Some(one.digest),
        _ => None,
    };
    Finding {
        kind,
        key: Some(key),
        file,
        pre: only(p),
        post: only(q),
        name: String::new(),
        location: None,
    }
}

/// §4 over the unit lists of the groups given: the groups of `G` from each
/// image (§4.1 step 2), or all of them. Every group must have its unit
/// list, consistent with its `count` and `digest`, and no `(class, file)`
/// may be given twice for one image; otherwise `Err`.
pub fn compare_groups(
    pre: &[&Group],
    post: &[&Group],
    exemptions: &Exemptions,
) -> Result<Comparison, CompareError> {
    let before = multisets(pre, false)?;
    let after = multisets(post, true)?;
    let mut findings = Vec::new();
    let mut used = Vec::new();

    // §4 steps 1 and 2, over D.
    runs(&before.declared, &after.declared, |o| o.key, |key, pk, qk| {
        // Step 1: an indeterminate unit on either side; no exemption applies.
        if pk.iter().chain(qk).any(|o| o.indeterminate) {
            findings.push(finding(FindingKind::Indeterminate, key, pk, qk));
            return;
        }
        // Step 2.
        let kind = match (pk.is_empty(), qk.is_empty()) {
            (false, false) if same_digests(pk, qk) => return,
            (false, false) => FindingKind::Changed,
            (false, true) => FindingKind::Removed,
            (true, false) => FindingKind::Added,
            (true, true) => unreachable!("runs: a key has a unit on some side"),
        };
        let exempt = exemptions.list_for(kind).is_some_and(|l| l.binary_search(&key).is_ok());
        if exempt {
            used.push(key);
        } else {
            findings.push(finding(kind, key, pk, qk));
        }
    });

    // §4 step 3, over V. No exemption applies to a derived key.
    let (mut only_pre, mut only_post) = (0u64, 0u64);
    runs(&before.derived, &after.derived, |o| o.key, |key, pk, qk| {
        if qk.is_empty() {
            only_pre += 1;
        } else if pk.is_empty() {
            only_post += 1;
        } else if pk.iter().chain(qk).any(|o| o.indeterminate) {
            findings.push(finding(FindingKind::DerivedIndeterminate, key, pk, qk));
        } else if !same_digests(pk, qk) {
            findings.push(finding(FindingKind::DerivedChanged, key, pk, qk));
        }
    });

    findings.sort_by(|a, b| (a.kind, &a.file, a.key).cmp(&(b.kind, &b.file, b.key)));
    // Each key is visited once in D, so `used` has no two equal; it is in
    // key order already, sorted again to make that independent of `runs`.
    used.sort_unstable();
    used.dedup();
    Ok(Comparison { findings, only_pre, only_post, used })
}

// ---------------------------------------------------------------------------
// §6: verdicts

/// The verdict of a comparison: `mismatch` if it has a finding, else
/// `verified`; `findings` cut to the first `limit`.
pub fn verdict(comparison: Comparison, pre_shard: Digest, rules: Digest, limit: usize) -> Verdict {
    let Comparison { mut findings, only_pre, only_post, used } = comparison;
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

/// §4.1 step 1: equal `tu` digests with `indeterminate` indeterminate
/// units. `verified` if 0, else `mismatch` with that many unnamed
/// `indeterminate` findings, cut to `limit`.
pub fn verdict_at_sums(indeterminate: u64, pre_shard: Digest, rules: Digest, limit: usize) -> Verdict {
    let shown = usize::try_from(indeterminate).map_or(limit, |n| n.min(limit));
    let unnamed = Finding {
        kind: FindingKind::Indeterminate,
        key: None,
        file: Vec::new(),
        pre: None,
        post: None,
        name: String::new(),
        location: None,
    };
    Verdict {
        kind: if indeterminate == 0 { VerdictKind::Verified } else { VerdictKind::Mismatch },
        pre: pre_shard,
        rules,
        reason: String::new(),
        only_pre: 0,
        only_post: 0,
        used: Vec::new(),
        total: indeterminate,
        findings: vec![unnamed; shown],
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
