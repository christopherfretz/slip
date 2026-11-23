//! The verify task (`spec/verify.md` §5): one observer, one round, an
//! image, the declared changes; one shard of phase `verify` with a
//! verdict. Work package WP-052.
//!
//! [`verify`] does steps 0 to 6 of §5.2 with every guard of §5.3, writes
//! the post shard of §5.4 and the counters of §7.2; under a rule table
//! that is not empty, the shard's `binding` and `rule_use` of §9.1 (work
//! package WP-121; with an empty table the shard is M1's, without
//! either). It takes everything
//! it is parameterised by from [`Params`]; it reads neither the job's
//! `post` nor the worker's arguments (`spec/exec.md` §8.6).
//!
//! This decides verdicts. A verdict `verified` is made in two places
//! only: at the sums (step 3, `compare::verified`) and from a comparison
//! without findings (step 5, `compare::verdict`). Every other path ends
//! in an `unverifiable` verdict, a shard without a verdict (the
//! post-image does not parse), or the job's `Err`.

use slip_digest::map::counters as walk_counters;
use slip_digest::{Detail, Options, Output, ReadOnly, ReadOnlyFiles, digest};
use slip_exec::store::Store;
use slip_exec::worker::Job;
use slip_kiln::Lookups;
use slip_model::agreement::{Binding, RuleUse};
use slip_model::declared::Declared;
use slip_model::digests::{Digests, Group, GroupClass, GroupSum};
use slip_model::formats::content_id;
use slip_model::keys::RuleTable;
use slip_model::verdict::{FindingKind, Verdict};
use slip_model::{Command, Counter, Digest, FileRef, Observer, Phase, Shard, Status};
use slip_verify::compare::{
    AtSums, CompareError, Comparison, Exemptions, at_sums, comparable, compare_groups,
    unverifiable, verdict, verified,
};

use crate::image::{Image, Resolver, Side, show_path};
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
    /// The content id of the `DECL` container `declared` was read from
    /// (`Job.post.declared`): the `declared` of the shard's `binding`
    /// (`spec/verify.md` §9.1).
    pub declared_id: Digest,
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

/// What step 4 gives when its guards hold: the comparison, and the pre
/// side of step 6.
type Compared = (Result<Comparison, CompareError>, Option<locate::Side>);

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
    let files = params.declared.files();
    let rules_id = params.declared.rules_id();
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
    let (mut shard, output) = post_walk(params, observer, job.round, detail)?;
    if shard.outcome.status != Status::Ok {
        // `not_parsed`: the parse task's shard, no digests, no verdict.
        return Ok(shard);
    }
    let Some((output, lookups)) = output else {
        return Err("the post-image parsed without error and the walk did not run".to_owned());
    };
    let mut post = Post::new(&mut shard, output, rules_id, !rules.is_empty());
    let unverifiable_now = |post: Post, mut shard: Shard, reason: &str| -> Result<Shard, String> {
        Ok(post.finish(&mut shard, unverifiable(reason, pre_id, rules_id)))
    };

    // Step 2: P1 to P3, G1, L1, G4, O2, C.
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
    if let Some(reason) = image_guards(params, observer, &pre, post_inputs, &lookups) {
        return unverifiable_now(post, shard, &reason);
    }
    if let Some(reason) = not_comparable(pre_digests, &post.digests) {
        return unverifiable_now(post, shard, &reason);
    }

    // Step 3: the sums.
    let g = match at_sums(pre_digests, &post.digests) {
        AtSums::Equal => return Ok(post.finish(&mut shard, verified(pre.id, rules_id))),
        AtSums::Differing(g) => g,
    };
    if g.is_empty() {
        // `at_sums` names a group whenever the `tu` digests differ or a
        // group is flagged, and `comparable` tied each `tu` to its groups:
        // an empty `G` here is a defect, not a comparison of nothing.
        return Err("at_sums gave no differing group for sums that differ".to_owned());
    }
    post.differing = g.len() as u64;

    // Step 4: the pre-image, and R1, G2, R2.
    let parsed = pre_image(observer, params, &pre, &g, &post.digests)?;
    post.reparsed += 1;
    let (comparison, pre_side) = match parsed {
        Ok(parsed) => parsed,
        Err(reason) => return unverifiable_now(post, shard, &reason),
    };

    // Step 5: the verdict from the comparison.
    let comparison = compared(comparison, observer)?;
    for f in &comparison.findings {
        post.by_kind[kind_index(f.kind)] += 1;
    }
    let mut v = verdict(comparison, pre.id, rules_id, params.limit);

    // Step 6: names, from a second parse of the post-image; G3.
    let locate_n = params.locate;
    if v.total > 0 && locate_n > 0 {
        let keys = locate::wanted(&v.findings, locate_n);
        let again = params.image.parse(observer, Side::Post, Phase::Verify, job.round, |input| {
            locate::take(input, rules, files, &keys)
        });
        post.reparsed += 1;
        let (again_shard, post_side) = again?;
        let same = again_shard.outcome.status == Status::Ok
            && again_shard.inputs == shard.inputs
            && post_side.as_ref().is_some_and(|s| *s.tu() == post.digests.tu);
        match post_side {
            Some(post_side) if same => {
                locate::fill(&mut v.findings, locate_n, pre_side.as_ref(), &post_side);
                (post.unstable, post.panicked) = locate::unplaced(pre_side.as_ref(), &post_side);
            }
            _ => {
                post.by_kind = [0; 6];
                v = unverifiable("post-image: changed during the job", pre.id, rules_id);
            }
        }
    }
    Ok(post.finish(&mut shard, v))
}

/// Step 1 (§5.2): the post-image parsed and walked under the declared
/// rules, with the read-only files decided from each buffer's real path
/// (`Input::subject_path` maps the real path, never the path as the front
/// end spells it; `spec/verify.md` §2.1), and the lookups for L1.
///
/// §9.1: under a rule table that is not empty the shard has `binding`,
/// whatever the parse's outcome (a `failed` one would be the
/// coordinator's, which has none), and the walk counts the rule use. With
/// an empty table the shard is M1's (§9.3's temporary exception), byte
/// for byte.
fn post_walk(
    params: &Params,
    observer: &Observer,
    round: u64,
    detail: Detail<'_>,
) -> Result<(Shard, Option<(Output, Lookups)>), String> {
    let (rules, files) = (params.declared.rules(), params.declared.files());
    params
        .image
        .parse_looking(observer, Side::Post, Phase::Verify, round, |input| {
            let read_only = ReadOnlyFiles::new(files, input.sm, input.subject_path);
            let options = Options {
                rules,
                read_only: ReadOnly::files(&read_only),
                detail,
                names: false,
                rule_use: !rules.is_empty(),
            };
            digest(input, &options)
        })
        .map(|(mut shard, output)| {
            if !rules.is_empty() && shard.outcome.status != Status::Failed {
                shard.binding = Some(binding(params));
            }
            (shard, output)
        })
}

/// The `binding` of the job's shards (`spec/verify.md` §9.1), from its
/// parameters, which are `Job.post`'s: no overlay (the image is a tree),
/// the declared changes' content id, the round of the pre shard.
fn binding(params: &Params) -> Binding {
    Binding { overlay: None, declared: params.declared_id, pre: params.pre }
}

/// Step 4 (§5.2): the pre-image parsed with detail for the groups of `G`,
/// with the comparison (step 5's `compare_groups`) and the pre side of
/// step 6 done inside the same parse, then guards R1, G2 and R2 in that
/// order. `Ok(Err(reason))` is the first guard that fails; `Err` a failure
/// of the job (the parse failed, or the walk did not run after a parse
/// without error).
fn pre_image(
    observer: &Observer,
    params: &Params,
    pre: &Pre,
    g: &[(GroupClass, Vec<u8>)],
    post_digests: &Digests,
) -> Result<Result<Compared, String>, String> {
    let in_g = |grp: &&Group| {
        g.binary_search_by(|(c, f)| (*c, f.as_slice()).cmp(&(grp.class, &grp.file))).is_ok()
    };
    let post_g: Vec<&Group> = post_digests.groups.iter().filter(in_g).collect();
    let exemptions = Exemptions::from_declared(&params.declared);
    let (limit, locate_n) = (params.limit, params.locate);
    let no_rules = RuleTable::empty();
    let options = Options {
        rules: &no_rules,
        read_only: ReadOnly::EVERY_FILE,
        detail: Detail::Groups(g),
        names: false,
        rule_use: false,
    };
    let (pre_shard, parsed): (Shard, Option<PreParse>) =
        params.image.parse(observer, Side::Pre, Phase::Map, params.pre, |input| {
            let out = digest(input, &options);
            let pre_g: Vec<&Group> = out.digests.groups.iter().filter(in_g).collect();
            let comparison = compare_groups(&pre_g, &post_g, &exemptions);
            let side = match &comparison {
                Ok(c) if !c.findings.is_empty() && locate_n > 0 => {
                    let keys = locate::wanted(&c.findings[..c.findings.len().min(limit)], locate_n);
                    Some(locate::take(input, &no_rules, None, &keys))
                }
                _ => None,
            };
            (out.digests, comparison, side)
        })?;
    // R1.
    if pre_shard.outcome.status != Status::Ok {
        return Ok(Err(format!(
            "pre-image: no longer parses ({}): {}",
            pre_shard.outcome.status.name(),
            pre_shard.outcome.message
        )));
    }
    let Some((pre_parsed, comparison, pre_side)) = parsed else {
        return Err("the pre-image parsed without error and the walk did not run".to_owned());
    };
    // G2.
    if let Some(path) = first_difference(
        pre_shard.inputs.as_deref().unwrap_or_default(),
        pre.shard.inputs.as_deref().unwrap_or_default(),
    ) {
        return Ok(Err(format!("pre-image: {} changed since the map phase", show_path(path))));
    }
    // R2.
    let stored = pre.shard.digests.as_ref().map(without_units);
    if stored.as_ref() != Some(&without_units(&pre_parsed)) {
        return Ok(Err("pre-image: its digests are not those of the map shard".to_owned()));
    }
    Ok(Ok((comparison, pre_side)))
}

/// What the post shard is made of, gathered along the steps.
struct Post {
    /// The parse's and the walk's counters of step 1.
    counters: Vec<Counter>,
    /// Step 1's `digests`, with the unit lists of detail `differing`.
    digests: Digests,
    /// Step 1's rule use (§9.1), when the rule table is not empty.
    rule_use: Option<RuleUse>,
    /// `verify.groups.differing`.
    differing: u64,
    /// `verify.reparsed`.
    reparsed: u64,
    /// The findings of each kind, by [`kind_index`].
    by_kind: [u64; 6],
    /// `verify.traces.unstable` (§7.2), present only when not 0.
    unstable: u64,
    /// `verify.traces.panicked` (§7.2): the images whose second writing
    /// panicked; present only when not 0.
    panicked: u64,
}

impl Post {
    /// After step 1: the parse's counters (taken from `shard`) and the
    /// walk's, its `digests`, and its rule use. §9.1: `rule_use` in every
    /// `ok` shard under a rule table that is not empty (`counted`: the
    /// walk counted only then), from step 1's walk, the only walk that counts (step
    /// 6's second parse is tied to it by G3).
    fn new(shard: &mut Shard, output: Output, rules_id: Digest, counted: bool) -> Post {
        let mut counters = std::mem::take(&mut shard.counters);
        counters.extend(walk_counters(&output));
        Post {
            counters,
            rule_use: counted.then_some(RuleUse { rules: rules_id, uses: output.rule_use }),
            digests: output.digests,
            differing: 0,
            reparsed: 0,
            by_kind: [0; 6],
            unstable: 0,
            panicked: 0,
        }
    }

    /// The post shard of §5.4: step 1's outcome and inputs (already in
    /// `shard`), its counters and those of §7.2, sorted by name; step 1's
    /// `digests` without unit lists; `v`.
    fn finish(self, shard: &mut Shard, v: Verdict) -> Shard {
        let Post {
            mut counters,
            mut digests,
            rule_use,
            differing,
            reparsed,
            by_kind,
            unstable,
            panicked,
        } = self;
        let by_name = [
            ("verify.groups.differing", differing),
            ("verify.findings", v.total),
            ("verify.derived.only_pre", v.only_pre),
            ("verify.derived.only_post", v.only_post),
            ("verify.exemptions.used", v.used.len() as u64),
            ("verify.reparsed", reparsed),
        ];
        counters.extend(by_name.iter().map(|&(n, value)| Counter { name: n.to_owned(), value }));
        for (name, value) in
            [("verify.traces.unstable", unstable), ("verify.traces.panicked", panicked)]
        {
            if value > 0 {
                counters.push(Counter { name: name.to_owned(), value });
            }
        }
        counters.extend(
            KINDS
                .iter()
                .zip(by_kind)
                .map(|(&(_, n), value)| Counter { name: format!("verify.findings.{n}"), value }),
        );
        counters.sort_by(|a, b| a.name.cmp(&b.name));
        for grp in &mut digests.groups {
            grp.units = None;
        }
        let mut out = shard.clone();
        out.counters = counters;
        out.digests = Some(digests);
        out.verdict = Some(v);
        out.rule_use = rule_use;
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
    KINDS
        .iter()
        .position(|&(k, _)| k == kind)
        .unwrap_or_else(|| unreachable!("every kind is listed"))
}

/// Guard C (§5.3): the reason of the `unverifiable` verdict when `pre`
/// and `post` cannot be compared. Both have been validated by then (the
/// pre shard by its decoder, the post digests by the walk that made
/// them), so this is reached only by a defect; it is kept, and tested
/// below, because §4.1 trusts what it checks.
fn not_comparable(pre: &Digests, post: &Digests) -> Option<String> {
    comparable(pre, post).err().map(|e| format!("digests: {e}"))
}

/// Step 5: a `CompareError` fails the job, naming the observer and its
/// main file. Both lists come from walks this job ran with detail for the
/// groups of `G`, so this too is reached only by a defect.
fn compared(
    comparison: Result<Comparison, CompareError>,
    observer: &Observer,
) -> Result<Comparison, String> {
    comparison.map_err(|e| {
        format!("{e} (observer {}, main file {})", observer.id, show_path(&observer.main.path))
    })
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
fn first_difference<'a>(left: &'a [FileRef], right: &'a [FileRef]) -> Option<&'a [u8]> {
    let (mut i, mut j) = (0, 0);
    loop {
        match (left.get(i), right.get(j)) {
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

/// Step 2's guards on what the post-image parse read and looked at, in
/// this order (§5.3): G1, L1, G4, G5, then O2 of the tree form (not with the
/// identity, where every change since the map phase is the edit
/// compared). The reason of the first that fails.
fn image_guards(
    params: &Params,
    observer: &Observer,
    pre: &Pre,
    post_inputs: &[FileRef],
    lookups: &Lookups,
) -> Option<String> {
    let mut resolver = params.image.resolver();
    if let Some(read) = post_inputs.iter().find(|f| resolver.under_subject(&f.path)) {
        return Some(format!("post-image: read {}, a file of the subject", show_path(&read.path)));
    }
    let post = params.image.post_command(&observer.command);
    if let Some(reason) = resolver.looked_in_subject(&post.command.directory, lookups) {
        return Some(reason);
    }
    if let Some(word) = response_file(&post.command) {
        return Some(format!(
            "post-image: the command names the response file {}, whose words slip cannot examine",
            show_path(word)
        ));
    }
    if let Some(reason) = post.conflict {
        return Some(reason);
    }
    if let Some((name, prefix)) = mapped_line_name(&post.maps, lookups) {
        return Some(format!(
            "post-image: the #line name {name} begins with {}, which the post command maps",
            show_path(prefix)
        ));
    }
    if params.image.is_identity() {
        return None;
    }
    let pre_inputs = pre.shard.inputs.as_deref().unwrap_or_default();
    drifted(&mut resolver, post_inputs, pre_inputs)
        .map(|path| format!("pre-image: {} changed since the map phase", show_path(path)))
}

/// Guard G5's second part (§5.3): the first `#line` name of the post
/// parse that a map pair of the post command applies to, with the pair's
/// post prefix. The name is the source's text, which the real post-image
/// keeps; the pair would make it another.
fn mapped_line_name<'a>(
    maps: &'a [(Vec<u8>, Vec<u8>)],
    lookups: &'a Lookups,
) -> Option<(&'a str, &'a [u8])> {
    lookups.line_names.iter().find_map(|name| {
        maps.iter()
            .find(|(post, _)| name.as_bytes().starts_with(post))
            .map(|(post, _)| (name.as_str(), post.as_slice()))
    })
}

/// Guard G4 (§5.3): the first word after `argv[0]` that Kiln's driver
/// reads as a response file (`@` and a name; `kiln_driver/src/rsp.rs`).
fn response_file(command: &Command) -> Option<&[u8]> {
    command.argv.iter().skip(1).map(Vec::as_slice).find(|w| w.len() > 1 && w[0] == b'@')
}

/// Guard O2 of the tree form (§5.3): the first entry of the post-image
/// parse's `inputs`, in path order, that is under no post root (nor, after
/// G1, under a subject root), that the pre shard's `inputs` also list, and
/// whose size or digest is not the one listed there. Both lists are sorted
/// by path.
fn drifted<'a>(
    resolver: &mut Resolver<'_>,
    post: &'a [FileRef],
    pre: &[FileRef],
) -> Option<&'a [u8]> {
    post.iter()
        .filter(|f| match pre.binary_search_by(|p| p.path.cmp(&f.path)) {
            Ok(i) => pre[i] != **f,
            Err(_) => false,
        })
        .find(|f| !resolver.under_post(&f.path))
        .map(|f| f.path.as_slice())
}

#[cfg(test)]
mod tests {
    use slip_model::digests::{GroupClass, SCHEME, tu_digest};
    use slip_model::{Command, Language, Origin};

    use super::*;

    fn digests(scheme: u64) -> Digests {
        let groups = vec![Group {
            class: GroupClass::Tu,
            file: Vec::new(),
            count: 0,
            indeterminate: 0,
            digest: Digest([1; 32]),
            units: None,
        }];
        Digests { scheme, tu: tu_digest(&groups), groups }
    }

    // §5.3 C: the reason is `digests: ` and `comparable`'s message, for
    // either image; none when both can be compared. Unit level: no
    // validated shard and no walk gives digests that fail it.
    #[test]
    fn c_digests_not_comparable() {
        let good = digests(SCHEME);
        assert_eq!(not_comparable(&good, &good), None);
        let other = digests(SCHEME + 1);
        let expect = |image: &str| {
            format!(
                "digests: the {image}-image digests have scheme {}; this build implements scheme {SCHEME}",
                SCHEME + 1
            )
        };
        assert_eq!(not_comparable(&other, &good), Some(expect("pre")));
        assert_eq!(not_comparable(&good, &other), Some(expect("post")));
        let mut bad_tu = digests(SCHEME);
        bad_tu.tu = Digest([9; 32]);
        assert_eq!(
            not_comparable(&good, &bad_tu).as_deref(),
            Some("digests: the post-image tu digest is not the digest of its groups")
        );
    }

    // §5.2 step 5: a `CompareError` fails the job, with the observer and
    // its main file; a comparison passes through.
    #[test]
    fn s5_2_compare_error_fails_the_job() {
        let command =
            Command { directory: b"/d".to_vec(), argv: vec![b"c++".to_vec()], env: Vec::new() };
        let observer = Observer {
            id: command.observer_id(),
            command,
            language: Language::Cxx,
            main: FileRef { path: b"/d/a\xff.cpp".to_vec(), size: 0, content: Digest([0; 32]) },
            cost_hint: 1,
            origins: vec![Origin { configuration: 0, entry: 0 }],
        };
        let e = CompareError::MissingUnits {
            class: GroupClass::Declared,
            file: b"/d/h.h".to_vec(),
            post: true,
        };
        let err = compared(Err(e.clone()), &observer).unwrap_err();
        assert_eq!(err, format!("{e} (observer {}, main file /d/a\\xff.cpp)", observer.id));
        let ok = compare_groups(&[], &[], &Exemptions::from_declared(&Declared::none()));
        assert!(compared(ok, &observer).is_ok());
    }
}
