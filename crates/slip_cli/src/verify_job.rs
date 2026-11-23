//! The verify task (`spec/verify.md` §5): one observer, one round, an
//! image, the declared changes; one shard of phase `verify` with a
//! verdict. Work package WP-052.
//!
//! [`verify`] does steps 0 to 6 of §5.2 with every guard of §5.3, writes
//! the post shard of §5.4 and the counters of §7.2. It takes everything
//! it is parameterised by from [`Params`]; it reads neither the job's
//! `post` nor the worker's arguments (`spec/exec.md` §8.6).
//!
//! This decides verdicts. A verdict `verified` is made in two places
//! only: at the sums (step 3, `compare::verified`) and from a comparison
//! without findings (step 5, `compare::verdict`). Every other path ends
//! in an `unverifiable` verdict, a shard without a verdict (the
//! post-image does not parse), or the job's `Err`.

use slip_digest::map::counters as walk_counters;
use slip_digest::{Detail, Options, digest};
use slip_exec::store::Store;
use slip_exec::worker::Job;
use slip_model::declared::Declared;
use slip_model::digests::{Digests, Group, GroupSum};
use slip_model::formats::content_id;
use slip_model::keys::RuleTable;
use slip_model::verdict::{FindingKind, Verdict};
use slip_model::{Counter, Digest, FileRef, Phase, Shard, Status};
use slip_verify::compare::{
    AtSums, Comparison, CompareError, Exemptions, at_sums, comparable, compare_groups,
    unverifiable, verdict, verified,
};

use crate::image::{Image, Side};
use crate::locate;

/// A verify job's parameters (`spec/verify.md` §5; `spec/exec.md` §8.6).
/// The worker makes them from `job.post` and its own options, in one
/// place; nothing under [`verify`] reads either.
#[derive(Clone, Debug)]
pub struct Params {
    /// The round of the map shard compared against (`Job.post.pre`,
    /// §5.2 step 0). Not in `spec/exec.md` §8.6's `Params` as written:
    /// without it `verify` would have to read the job's `post`.
    pub pre: u64,
    pub declared: Declared,
    pub image: Image,
    /// The findings a verdict keeps.
    pub limit: usize,
    /// The findings that are named (and, from WP-054, localised).
    pub locate: usize,
}

/// The pre shard of step 0, as read from the store.
struct Pre {
    shard: Shard,
    /// Its content id.
    id: Digest,
}

/// What the closure of step 4's parse gives back: the pre-image's
/// digests, the comparison, and the pre side of localisation when the
/// comparison has a finding to name.
type PreParse = (Digests, Result<Comparison, CompareError>, Option<locate::Side>);

/// `spec/verify.md` §5.2: the verify job of `job` with `params`, reading
/// the pre shard from `store`. The shard returned is for `job`'s observer,
/// phase `verify` and round; the worker validates and publishes it.
///
/// # Errors
///
/// A failure of the job, never a verdict: a job that is not of phase
/// `verify` or has a `transform`; a store that cannot be read; a pre shard
/// that is there but is not a valid shard of this observer, phase `map`
/// and round `params.pre`; a parse that fails (`Image::parse`); a
/// `CompareError` (step 5), with the observer's main file.
pub fn verify(store: &Store, job: &Job, params: &Params) -> Result<Shard, String> {
    if job.phase != Phase::Verify {
        return Err("the verify task runs phase verify only; the job is of phase map".to_owned());
    }
    if job.transform.is_some() {
        return Err("a verify job with a transform".to_owned());
    }
    let rules = params.declared.rules();
    let rules_id = rules.id();
    let observer = &job.observer;

    // Step 0: the pre shard.
    let pre = read_pre(store, job, params.pre)?;
    let pre_id = pre.as_ref().map_or(Digest([0; 32]), |p| p.id);
    let pre_digests = pre
        .as_ref()
        .filter(|p| p.shard.outcome.status == Status::Ok)
        .and_then(|p| p.shard.digests.as_ref());

    // Step 1: the post-image, with detail `differing` from the pre shard's
    // sums when it is usable.
    let sums: Vec<GroupSum> =
        pre_digests.map(|d| d.groups.iter().map(Group::sum).collect()).unwrap_or_default();
    let detail = if pre_digests.is_some() { Detail::Differing(&sums) } else { Detail::None };
    let options = Options { rules, detail, names: false };
    let (mut shard, output) =
        params.image.parse(observer, Side::Post, Phase::Verify, job.round, |input| {
            digest(input, &options)
        })?;
    if shard.outcome.status != Status::Ok {
        // `not_parsed`: the parse task's shard, no digests, no verdict.
        return Ok(shard);
    }
    let Some(output) = output else {
        return Err("the post-image parsed without error and the walk did not run".to_owned());
    };
    let mut counters = std::mem::take(&mut shard.counters);
    counters.extend(walk_counters(&output));
    let mut post =
        Post { counters, digests: output.digests, differing: 0, reparsed: 0, by_kind: [0; 6] };
    let unverifiable_now =
        |post: Post, mut shard: Shard, reason: &str| -> Result<Shard, String> {
            Ok(post.finish(&mut shard, unverifiable(reason, pre_id, rules_id)))
        };

    // Step 2: P1 to P3, G1, C.
    let Some(pre) = pre else {
        return unverifiable_now(post, shard, "pre-image: no map shard");
    };
    if pre.shard.outcome.status != Status::Ok {
        let reason = format!("pre-image: the map shard is {}", pre.shard.outcome.status.name());
        return unverifiable_now(post, shard, &reason);
    }
    let Some(pre_digests) = pre.shard.digests.as_ref() else {
        return unverifiable_now(post, shard, "pre-image: the map shard has no digests");
    };
    let Some(post_inputs) = shard.inputs.as_ref() else {
        return Err("the post-image parsed without error and has no inputs".to_owned());
    };
    if let Some(read) = post_inputs.iter().find(|f| params.image.under_subject(&f.path)) {
        let reason = format!("post-image: read {}, a file of the subject", show_path(&read.path));
        return unverifiable_now(post, shard, &reason);
    }
    if let Err(e) = comparable(pre_digests, &post.digests) {
        return unverifiable_now(post, shard, &format!("digests: {e}"));
    }

    // Step 3: the sums.
    let g = match at_sums(pre_digests, &post.digests) {
        AtSums::Equal => {
            let v = verified(pre.id, rules_id);
            return Ok(post.finish(&mut shard, v));
        }
        AtSums::Differing(g) => g,
    };
    if g.is_empty() {
        // `at_sums` names a group whenever the `tu` digests differ or a
        // group is flagged, and `comparable` tied each `tu` to its groups:
        // an empty `G` here is a defect, not a comparison of nothing.
        return Err("at_sums gave no differing group for sums that differ".to_owned());
    }
    post.differing = g.len() as u64;

    // Step 4: the pre-image, with detail for the groups of `G`; the
    // comparison (and the pre side of step 6) inside the same parse.
    let in_g = |grp: &&Group| g.binary_search_by(|(c, f)| (*c, f.as_slice()).cmp(&(grp.class, &grp.file))).is_ok();
    let post_g: Vec<&Group> = post.digests.groups.iter().filter(in_g).collect();
    let exemptions = Exemptions::from_declared(&params.declared);
    let (limit, locate_n) = (params.limit, params.locate);
    let no_rules = RuleTable::empty();
    let pre_options = Options { rules: &no_rules, detail: Detail::Groups(&g), names: false };
    let parsed = params.image.parse(observer, Side::Pre, Phase::Map, params.pre, |input| {
        let out = digest(input, &pre_options);
        let pre_g: Vec<&Group> = out.digests.groups.iter().filter(in_g).collect();
        let comparison = compare_groups(&pre_g, &post_g, &exemptions);
        let side = match &comparison {
            Ok(c) if !c.findings.is_empty() && locate_n > 0 => {
                let keys = locate::wanted(&c.findings[..c.findings.len().min(limit)], locate_n);
                Some(locate::take(input, &no_rules, &keys))
            }
            _ => None,
        };
        (out.digests, comparison, side)
    });
    post.reparsed += 1;
    let (pre_shard, pre_parse): (Shard, Option<PreParse>) = parsed?;
    // R1.
    if pre_shard.outcome.status != Status::Ok {
        let reason = format!(
            "pre-image: no longer parses ({}): {}",
            pre_shard.outcome.status.name(),
            pre_shard.outcome.message
        );
        return unverifiable_now(post, shard, &reason);
    }
    let Some((pre_parsed, comparison, pre_side)) = pre_parse else {
        return Err("the pre-image parsed without error and the walk did not run".to_owned());
    };
    // G2.
    if let Some(path) = first_difference(
        pre_shard.inputs.as_deref().unwrap_or_default(),
        pre.shard.inputs.as_deref().unwrap_or_default(),
    ) {
        let reason = format!("pre-image: {} changed since the map phase", show_path(path));
        return unverifiable_now(post, shard, &reason);
    }
    // R2.
    if without_units(&pre_parsed) != without_units(pre_digests) {
        return unverifiable_now(post, shard, "pre-image: its digests are not those of the map shard");
    }

    // Step 5: the verdict from the comparison.
    let comparison = comparison.map_err(|e| {
        format!("{e} (observer {}, main file {})", observer.id, show_path(&observer.main.path))
    })?;
    for f in &comparison.findings {
        post.by_kind[kind_index(f.kind)] += 1;
    }
    let mut v = verdict(comparison, pre.id, rules_id, limit);

    // Step 6: names, from a second parse of the post-image; G3.
    if v.total > 0 && locate_n > 0 {
        let keys = locate::wanted(&v.findings, locate_n);
        let again = params.image.parse(observer, Side::Post, Phase::Verify, job.round, |input| {
            locate::take(input, rules, &keys)
        });
        post.reparsed += 1;
        let (again_shard, post_side) = again?;
        let same = again_shard.outcome.status == Status::Ok
            && again_shard.inputs == shard.inputs
            && post_side.as_ref().is_some_and(|s| *s.tu() == post.digests.tu);
        match post_side {
            Some(post_side) if same => {
                locate::fill(&mut v.findings, locate_n, pre_side.as_ref(), &post_side);
            }
            _ => {
                post.by_kind = [0; 6];
                v = unverifiable("post-image: changed during the job", pre.id, rules_id);
            }
        }
    }
    Ok(post.finish(&mut shard, v))
}

/// What the post shard is made of, gathered along the steps.
struct Post {
    /// The parse's and the walk's counters of step 1.
    counters: Vec<Counter>,
    /// Step 1's `digests`, with the unit lists of detail `differing`.
    digests: Digests,
    /// `verify.groups.differing`.
    differing: u64,
    /// `verify.reparsed`.
    reparsed: u64,
    /// The findings of each kind, by [`kind_index`].
    by_kind: [u64; 6],
}

impl Post {
    /// The post shard of §5.4: step 1's outcome and inputs (already in
    /// `shard`), its counters and those of §7.2, sorted by name; step 1's
    /// `digests` without unit lists; `v`.
    fn finish(self, shard: &mut Shard, v: Verdict) -> Shard {
        let Post { mut counters, mut digests, differing, reparsed, by_kind } = self;
        let by_name = [
            ("verify.groups.differing", differing),
            ("verify.findings", v.total),
            ("verify.derived.only_pre", v.only_pre),
            ("verify.derived.only_post", v.only_post),
            ("verify.exemptions.used", v.used.len() as u64),
            ("verify.reparsed", reparsed),
        ];
        counters.extend(by_name.iter().map(|&(n, value)| Counter { name: n.to_owned(), value }));
        counters.extend(KINDS.iter().zip(by_kind).map(|(&(_, n), value)| Counter {
            name: format!("verify.findings.{n}"),
            value,
        }));
        counters.sort_by(|a, b| a.name.cmp(&b.name));
        for grp in &mut digests.groups {
            grp.units = None;
        }
        let mut out = shard.clone();
        out.counters = counters;
        out.digests = Some(digests);
        out.verdict = Some(v);
        out
    }
}

/// The six kinds, in the order of `by_kind`, with their counter word.
const KINDS: [(FindingKind, &str); 6] = [
    (FindingKind::Changed, "changed"),
    (FindingKind::Removed, "removed"),
    (FindingKind::Added, "added"),
    (FindingKind::Indeterminate, "indeterminate"),
    (FindingKind::DerivedChanged, "derived_changed"),
    (FindingKind::DerivedIndeterminate, "derived_indeterminate"),
];

fn kind_index(kind: FindingKind) -> usize {
    KINDS.iter().position(|&(k, _)| k == kind).unwrap_or_else(|| unreachable!("every kind is listed"))
}

/// Step 0: the observer's shard of phase `map` and round `round`. `None`
/// if the store has none. A file that is there but is not a valid shard,
/// or is a shard of another observer, phase or round, fails the job: the
/// store was altered, and no verdict is made from it.
fn read_pre(store: &Store, job: &Job, round: u64) -> Result<Option<Pre>, String> {
    let path = store.shard_path(&job.plan, Phase::Map, round, &job.observer.id);
    let Some(bytes) = store
        .read_shard(&job.plan, Phase::Map, round, &job.observer.id)
        .map_err(|e| format!("cannot read the map shard: {e}"))?
    else {
        return Ok(None);
    };
    let invalid = |e: &dyn std::fmt::Display| {
        format!("the map shard {} is not a valid shard: {e}", path.display())
    };
    let shard = Shard::from_bytes(&bytes).map_err(|e| invalid(&e))?;
    let id = content_id(&bytes).map_err(|e| invalid(&e))?;
    if shard.observer != job.observer.id || shard.phase != Phase::Map || shard.round != round {
        return Err(format!(
            "the map shard {} is for observer {} {:?} round {}, not observer {} map round {round}",
            path.display(),
            shard.observer,
            shard.phase,
            shard.round,
            job.observer.id
        ));
    }
    Ok(Some(Pre { shard, id }))
}

/// Guard G2: the first path, in path order, whose entry is not the same
/// (path, size and digest) in both lists or is in one only. Both lists
/// are sorted by path, as a shard's `inputs` are.
fn first_difference<'a>(a: &'a [FileRef], b: &'a [FileRef]) -> Option<&'a [u8]> {
    let (mut i, mut j) = (0, 0);
    loop {
        match (a.get(i), b.get(j)) {
            (None, None) => return None,
            (Some(x), None) => return Some(&x.path),
            (None, Some(y)) => return Some(&y.path),
            (Some(x), Some(y)) => match x.path.cmp(&y.path) {
                std::cmp::Ordering::Less => return Some(&x.path),
                std::cmp::Ordering::Greater => return Some(&y.path),
                std::cmp::Ordering::Equal if x != y => return Some(&x.path),
                std::cmp::Ordering::Equal => {
                    i += 1;
                    j += 1;
                }
            },
        }
    }
}

/// Guard R2's view of a `digests` section: everything but the unit lists.
fn without_units(d: &Digests) -> Digests {
    Digests {
        scheme: d.scheme,
        tu: d.tu,
        groups: d.groups.iter().map(|g| Group { units: None, ..g.clone() }).collect(),
    }
}

/// A path in a reason (§5.3): its bytes where they are UTF-8, each other
/// byte as `\xNN`.
fn show_path(path: &[u8]) -> String {
    let mut out = String::with_capacity(path.len());
    for chunk in path.utf8_chunks() {
        out.push_str(chunk.valid());
        for b in chunk.invalid() {
            out.push_str(&format!("\\x{b:02x}"));
        }
    }
    out
}
