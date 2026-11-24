//! The verify task (`spec/verify.md` §5): one observer, one round, an
//! image, the declared changes; one shard of phase `verify` with a
//! verdict. Work package WP-052.
//!
//! [`verify`] does steps 0 to 6 of §5.2 with every guard of §5.3, writes
//! the post shard of §5.4 and the counters of §7.2; the shard's `binding`
//! of §9.1 in every shard (work package WP-114; under a rule table that is
//! not empty since WP-121), and its `rule_use` in an `ok` shard under a
//! rule table that is not empty (WP-121). It takes everything
//! it is parameterised by from [`Params`]; it reads neither the job's
//! `post` nor the worker's arguments (`spec/exec.md` §8.6).
//!
//! This decides verdicts. A verdict `verified` is made in two places
//! only: at the sums (step 3, `compare::verified`) and from a comparison
//! without findings (step 5, `compare::verdict`). Every other path ends
//! in an `unverifiable` verdict, a shard without a verdict (the
//! post-image does not parse), or the job's `Err`.

use std::ffi::OsStr;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::Path;

use slip_digest::map::{counters as walk_counters, with_input};
use slip_digest::spelling::{Spelling, check_spellings};
use slip_digest::{
    Detail, NamesUnit, Options, Output, ReadOnly, ReadOnlyFiles, digest, digest_kept,
};
use slip_exec::store::Store;
use slip_exec::worker::Job;
use slip_kiln::Lookups;
use slip_model::agreement::{Binding, RuleUse};
use slip_model::declared::Declared;
use slip_model::digests::{Digests, Group, GroupClass, GroupSum};
use slip_model::formats::content_id;
use slip_model::hash::content_digest;
use slip_model::keys::RuleTable;
use slip_model::names::{NameNote, Names, NamesEntry};
use slip_model::overlay::Overlay;
use slip_model::spellings::{Failed, Span, Spellings};
use slip_model::verdict::{FindingKind, Verdict, VerdictKind};
use slip_model::{Command, Counter, Digest, FileRef, Observer, Phase, Shard, Status};
use slip_verify::compare::{
    AtSums, CompareError, Comparison, Exemptions, at_sums, comparable, compare_groups,
    names_changed, unverifiable, verdict, verified,
};
use slip_verify::guards::{OverlayGuard, overlay_inputs};

use crate::image::{Image, Resolver, Side, show_path};
use crate::worker::post::{Loaded, Read, reads};
use crate::{locate, negatives};

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
    // The overlay form (§5.1, WP-074) loads "the pre shard's inputs (every
    // file of the overlay if the pre shard is not usable)" first.
    let first = pre_digests.and(pre.as_ref()).and_then(|p| p.shard.inputs.as_deref());
    let (mut shard, output, mut through) =
        step_1(store, params, observer, job.round, detail, first)?;
    if shard.outcome.status != Status::Ok {
        // `not_parsed`: the parse task's shard, no digests, no verdict.
        return Ok(shard);
    }
    let Some((output, lookups)) = output else {
        return Err("the post-image parsed without error and the walk did not run".to_owned());
    };
    let mut post = Post::new(&mut shard, output, rules_id, !rules.is_empty());
    post.spellings = through.as_mut().and_then(|t| t.spellings.take());
    let unverifiable_now = |post: Post, mut shard: Shard, reason: &str| -> Result<Shard, String> {
        Ok(post.finish(&mut shard, unverifiable(reason, pre_id, rules_id)))
    };

    // Step 2: P1 to P3, G1, L1, G4, G5, G6, O2, N1, C.
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
    let guards = step_2_guards(params, observer, &pre, post_inputs, &lookups, through.as_ref());
    if let Some(reason) = guards {
        return unverifiable_now(post, shard, &reason);
    }
    if let Some(reason) = not_comparable(pre_digests, &post.digests) {
        return unverifiable_now(post, shard, &reason);
    }
    // §4.4 (WP-112): the notes, on both paths that compare (a declared
    // rename leaves the sums equal). A map shard with digests has names
    // (`spec/model.md` §8.7), so this guard cannot fire on a shard the
    // store gave back; it is kept so that no note is ever "none" by default.
    let Some(pre_names) = pre.shard.names.as_ref() else {
        return unverifiable_now(post, shard, "pre-image: the map shard has no names section");
    };
    let notes = names_changed(&pre_names.units, &post.names.units);
    post.note(&notes);

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
    let mut comparison = compared(comparison, observer)?;
    comparison.notes = notes;
    for f in &comparison.findings {
        post.by_kind[kind_index(f.kind)] += 1;
    }
    let mut v = verdict(comparison, pre.id, rules_id, params.limit);

    // Step 6: names, from a second parse of the post-image; G3.
    let locate_n = params.locate;
    if v.total > 0 && locate_n > 0 {
        let keys = locate::wanted(&v.findings, locate_n);
        let take = |input: &slip_digest::Input<'_>| locate::take(input, rules, files, &keys);
        let again = post_again(params, through.as_ref(), observer, job.round, take);
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
/// §9.1: the shard has `binding`, whatever the parse's outcome and the
/// rule table (a `failed` one would be the coordinator's, which has none;
/// WP-114 removed §9.3's exception for a tree image and no rule); under a
/// rule table that is not empty the walk counts the rule use.
fn post_walk(
    params: &Params,
    observer: &Observer,
    round: u64,
    detail: Detail<'_>,
) -> Result<(Shard, Option<(Output, Lookups)>), String> {
    let (rules, files) = (params.declared.rules(), params.declared.files());
    let kept = params.declared.kept();
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
            // `Declared.kept` decides a namespace rule's `direct` and
            // `kept_via` (§9.2, WP-111).
            digest_kept(input, &options, kept)
        })
        .map(|(mut shard, output)| {
            if shard.outcome.status != Status::Failed {
                shard.binding = Some(binding(params));
            }
            (shard, output)
        })
}

/// The `binding` of the job's shards (`spec/verify.md` §9.1), from its
/// parameters, which are `Job.post`'s: the content id of the image's
/// `OVLY` (absent for a tree), the declared changes' content id, the round
/// of the pre shard.
fn binding(params: &Params) -> Binding {
    Binding { overlay: params.image.overlay_id(), declared: params.declared_id, pre: params.pre }
}

/// Step 1's shard, the walk's output with the lookups when it ran, and
/// with an overlay image what [`Through`] adds.
type Step1<'o> = (Shard, Option<(Output, Lookups)>, Option<Through<'o>>);

/// Step 1 (§5.2): [`post_walk`] for a tree; for an overlay image,
/// [`overlay_walk`] with the blobs this observer reads ([`Loaded`], made
/// here: one job's), which the shard's [`Through`] keeps for step 6.
fn step_1<'o>(
    store: &Store,
    params: &'o Params,
    observer: &Observer,
    round: u64,
    detail: Detail<'_>,
    first: Option<&[FileRef]>,
) -> Result<Step1<'o>, String> {
    match params.image.overlay_of() {
        None => {
            let (shard, output) = post_walk(params, observer, round, detail)?;
            Ok((shard, output, None))
        }
        Some((id, overlay)) => {
            let loaded = Loaded::new(overlay, id);
            overlay_walk(store, params, loaded, observer, round, detail, first)
        }
    }
}

/// What step 1's parse of an overlay image gives beside M1's: the blobs
/// given, how the unit read each input (for guard O1) and section 26.
struct Through<'o> {
    loaded: Loaded<'o>,
    read: Vec<Read>,
    spellings: Option<(Spellings, Vec<Counter>)>,
}

/// The walk's result in the loading loop's last parse: the walk's output
/// and, with obligations, the checks of §10.2 with each obligation's index.
type Checked = (Output, Option<Result<(Vec<Spelling>, Vec<u64>), String>>);

/// What the closure of one parse of the loading loop gives back: how each
/// input was read, the overlay files to give next, and (in the last parse,
/// when the unit has no error) the walk with the lookups.
type LoopParse = (Vec<Read>, Vec<usize>, Option<(Result<Checked, String>, Option<Lookups>)>);

/// Step 1 (§5.2) of an overlay image: the loading loop of §5.1 "Loading",
/// whose last parse is step 1's. `first` is the pre shard's `inputs` when
/// it is usable: the overlay files among them are given first; without, every
/// file of the overlay is. Each parse that read, from the file system, an
/// overlay file not given (by path or identity, `Loaded::missing`; the unit
/// with errors or without) gives those and parses again. The walk runs only
/// in the last parse: the one that read no such file, or the one the test
/// hook (`Image::with_loading_limit`) stops at. In it, after the walk and on
/// the same input, the checks of §10.2 over the obligations of the overlay
/// files the unit read, with the declared rules and the walk's read-only
/// files. The number of parses is in nothing returned.
///
/// The parse is the observer's own command with the overlay given, Kiln's
/// written-reference tables when the overlay has obligations, and the
/// lookup log (for G5's `#line` part). The shard has `binding` (its
/// `overlay` the image's) unless the parse failed.
///
/// # Errors
///
/// [`Loaded::load`]'s (a blob absent or invalid: the job fails); the
/// parse's; the walk's.
fn overlay_walk<'o>(
    store: &Store,
    params: &Params,
    mut loaded: Loaded<'o>,
    observer: &Observer,
    round: u64,
    detail: Detail<'_>,
    first: Option<&[FileRef]>,
) -> Result<Step1<'o>, String> {
    // One `stat` per entry: two entries of one file fail the job; an entry
    // with no file now is never given (`Loaded::index`).
    loaded.index()?;
    match first {
        Some(inputs) => loaded.load(store, &mut inputs.iter().map(|f| f.path.as_slice()))?,
        None => loaded.load_all(store)?,
    };
    let (rules, files, kept) =
        (params.declared.rules(), params.declared.files(), params.declared.kept());
    let limit = params.image.loading_limit();
    let mut parses = 0usize;
    loop {
        parses += 1;
        let stop = limit.is_some_and(|n| parses >= n);
        let l = &loaded;
        let mut options = l.parse_options();
        options.lookups = true;
        let (mut shard, ran): (Shard, Option<LoopParse>) = params.image.parse_unit(
            observer,
            Side::Post,
            Phase::Verify,
            round,
            options,
            |unit, subject| {
                let read = reads(unit);
                let missing = l.missing(unit.inputs(), &read);
                if (!missing.is_empty() && !stop) || unit.errors() != 0 {
                    return (read, missing, None);
                }
                let walked = with_input(unit, subject, |input| {
                    let read_only = ReadOnlyFiles::new(files, input.sm, input.subject_path);
                    let options = Options {
                        rules,
                        read_only: ReadOnly::files(&read_only),
                        detail,
                        names: false,
                        rule_use: !rules.is_empty(),
                    };
                    let output = digest_kept(input, &options, kept);
                    let checked = (!l.overlay().spellings.is_empty()).then(|| {
                        let (obligations, indices) = l.obligations(unit.inputs())?;
                        let ro = ReadOnly::files(&read_only);
                        Ok((check_spellings(unit, input, rules, ro, &obligations), indices))
                    });
                    (output, checked)
                });
                (read, missing, Some((walked, unit.lookups().cloned())))
            },
        )?;
        if shard.outcome.status != Status::Failed {
            shard.binding = Some(binding(params));
        }
        let Some((read, missing, walked)) = ran else {
            // No parse happened (a command the front end rejects).
            return Ok((shard, None, None));
        };
        if !missing.is_empty() && !stop {
            if loaded.load_indices(store, &mut missing.into_iter())? == 0 {
                unreachable!("the files missing are files not given");
            }
            continue;
        }
        if shard.outcome.status != Status::Ok {
            return Ok((shard, None, None));
        }
        let Some((walked, lookups)) = walked else {
            return Err("the post-image parsed without error and the walk did not run".to_owned());
        };
        let (output, checked) = walked?;
        let Some(lookups) = lookups else {
            return Err("the front end recorded no lookup log".to_owned());
        };
        let spellings = match checked {
            None => None,
            Some(checked) => {
                let (results, indices) = checked?;
                Some(spellings_section(&results, &indices)?)
            }
        };
        let through = Through { loaded, read, spellings };
        return Ok((shard, Some((output, lookups)), Some(through)));
    }
}

/// Step 2's guards on what the post-image parse read: [`image_guards`] for
/// a tree, [`overlay_guards`] with the loaded blobs and how the unit read
/// each input for an overlay image.
fn step_2_guards(
    params: &Params,
    observer: &Observer,
    pre: &Pre,
    post_inputs: &[FileRef],
    lookups: &Lookups,
    through: Option<&Through<'_>>,
) -> Option<String> {
    match through {
        Some(t) => overlay_guards(params, observer, pre, post_inputs, &t.loaded, &t.read),
        None => image_guards(params, observer, pre, post_inputs, lookups),
    }
}

/// Step 6's second parse (§5.2): [`Image::parse`] of the post-image for a
/// tree, [`overlay_again`] for an overlay image (G3 through the overlay:
/// the same entries as step 1's last parse).
///
/// # Errors
///
/// The parse's.
fn post_again<R: Send>(
    params: &Params,
    through: Option<&Through<'_>>,
    observer: &Observer,
    round: u64,
    f: impl FnOnce(&slip_digest::Input<'_>) -> R + Send,
) -> Result<(Shard, Option<R>), String> {
    match through {
        None => params.image.parse(observer, Side::Post, Phase::Verify, round, f),
        Some(t) => overlay_again(params, &t.loaded, observer, round, f),
    }
}

/// Step 6's second parse (§5.2) of an overlay image: the post-image through
/// the same entries as step 1's last parse (guard G3 then compares the two),
/// `f` on the input when the unit has no error and is `ok`.
///
/// # Errors
///
/// The parse's; `with_input`'s.
fn overlay_again<R: Send>(
    params: &Params,
    loaded: &Loaded<'_>,
    observer: &Observer,
    round: u64,
    f: impl FnOnce(&slip_digest::Input<'_>) -> R + Send,
) -> Result<(Shard, Option<R>), String> {
    let (shard, ran) = params.image.parse_unit(
        observer,
        Side::Post,
        Phase::Verify,
        round,
        loaded.parse_options(),
        |unit, subject| (unit.errors() == 0).then(|| with_input(unit, subject, f)),
    )?;
    let value = ran.flatten().transpose()?;
    let ok = shard.outcome.status == Status::Ok;
    Ok((shard, value.filter(|_| ok)))
}

/// Section 26 (§10.4) from `check_spellings`'s answers, parallel to
/// `indices` (each obligation's index in the overlay's `spellings`,
/// ascending), and its counters `verify.spelling.*`.
///
/// # Errors
///
/// The answers are not one per obligation, or the indices not ascending: a
/// defect, never a section that says less.
fn spellings_section(
    results: &[Spelling],
    indices: &[u64],
) -> Result<(Spellings, Vec<Counter>), String> {
    if results.len() != indices.len() || !indices.windows(2).all(|w| w[0] < w[1]) {
        return Err("the spelling checks are not one per obligation, in order".to_owned());
    }
    let mut held: Vec<Span> = Vec::new();
    let mut failed: Vec<Failed> = Vec::new();
    for (result, &index) in results.iter().zip(indices) {
        match result {
            Spelling::Holds => match held.last_mut() {
                Some(span) if span.first + span.count == index => span.count += 1,
                _ => held.push(Span { first: index, count: 1 }),
            },
            Spelling::NotSeen => {}
            Spelling::Fails { found, found_name } => {
                failed.push(Failed { index, found: *found, name: found_name.clone() });
            }
        }
    }
    let seen = indices.len() as u64;
    let holds: u64 = held.iter().map(|s| s.count).sum();
    let fails = failed.len() as u64;
    let counters = [
        ("verify.spelling.fails", fails),
        ("verify.spelling.holds", holds),
        ("verify.spelling.not_seen", seen - holds - fails),
        ("verify.spelling.obligations", seen),
    ]
    .iter()
    .map(|&(name, value)| Counter { name: name.to_owned(), value })
    .collect();
    Ok((Spellings { seen, held, failed }, counters))
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
    /// Step 1's names digests, and from step 2 the notes (§4.4, WP-112).
    names: Names,
    /// Step 1's names digests with each key's file and name, by key.
    name_strings: Vec<NamesUnit>,
    /// `verify.traces.unstable` (§7.2), present only when not 0.
    unstable: u64,
    /// `verify.traces.panicked` (§7.2): the images whose second writing
    /// panicked; present only when not 0.
    panicked: u64,
    /// Step 1's checks of §10.2 (section 26) and their counters of §10.4,
    /// with an overlay image that has obligations.
    spellings: Option<(Spellings, Vec<Counter>)>,
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
            rule_use: counted.then_some(RuleUse {
                rules: rules_id,
                uses: output.rule_use,
                kept: output.kept,
            }),
            digests: output.digests,
            differing: 0,
            reparsed: 0,
            by_kind: [0; 6],
            names: Names {
                units: output
                    .name_strings
                    .iter()
                    .map(|n| NamesEntry { key: n.key, names: n.names })
                    .collect(),
                notes: Vec::new(),
            },
            name_strings: output.name_strings,
            unstable: 0,
            panicked: 0,
            spellings: None,
        }
    }

    /// The notes of `keys` (ascending, each with a names digest in step
    /// 1), with the file and name of the key's unit (§4.4).
    fn note(&mut self, keys: &[Digest]) {
        let strings = &self.name_strings;
        self.names.notes = keys
            .iter()
            .filter_map(|k| {
                let i = strings.binary_search_by(|n| n.key.cmp(k)).ok()?;
                let n = &strings[i];
                Some(NameNote { key: n.key, file: n.file.clone(), name: n.name.clone() })
            })
            .collect();
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
            mut names,
            name_strings: _,
            unstable,
            panicked,
            spellings,
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
        let spellings = spellings.map(|(section, spelling_counters)| {
            counters.extend(spelling_counters);
            section
        });
        counters.sort_by(|a, b| a.name.cmp(&b.name));
        for grp in &mut digests.groups {
            grp.units = None;
        }
        let mut out = shard.clone();
        out.counters = counters;
        out.digests = Some(digests);
        // Notes only beside a comparison (`spec/model.md` §8.7).
        if !matches!(v.kind, VerdictKind::Verified | VerdictKind::Mismatch) {
            names.notes.clear();
        }
        out.names = Some(names);
        out.verdict = Some(v);
        out.rule_use = rule_use;
        out.spellings = spellings;
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
/// this order (§5.3): [`post_parse_guards`], then O2 (both parts), N1 and N2 of the tree
/// form (not with the identity, where every change since the map phase is
/// the edit compared). The reason of the first that fails. They run before
/// the comparison, so whatever it finds.
fn image_guards(
    params: &Params,
    observer: &Observer,
    pre: &Pre,
    post_inputs: &[FileRef],
    lookups: &Lookups,
) -> Option<String> {
    let mut resolver = params.image.resolver();
    if let Some(reason) =
        post_parse_guards(&params.image, &mut resolver, &observer.command, post_inputs, lookups)
    {
        return Some(reason);
    }
    if params.image.is_identity() {
        return None;
    }
    let pre_inputs = pre.shard.inputs.as_deref().unwrap_or_default();
    if let Some(path) = drifted(&mut resolver, post_inputs, pre_inputs) {
        return Some(format!("pre-image: {} changed since the map phase", show_path(path)));
    }
    if let Some((path, how)) = left_behind(&mut resolver, post_inputs, pre_inputs) {
        return Some(how.reason(path));
    }
    negatives_guards(&mut resolver, pre, pre_inputs)
}

/// Guards N1 then N2 (§5.3) of the pre shard; the reason of the first that
/// fails. With a resolver of no root (the overlay form) every negative
/// input and every place is checked.
fn negatives_guards(
    resolver: &mut Resolver<'_>,
    pre: &Pre,
    pre_inputs: &[FileRef],
) -> Option<String> {
    // N1: no file has appeared, outside the subject roots, where the map
    // parse looked for one and found none. Without the record, fail closed.
    let Some(negatives) = pre.shard.negatives.as_deref() else {
        return Some(
            "pre-image: the map shard has no negative inputs (run `slip parse` again)".to_owned(),
        );
    };
    if let Some(path) = negatives::appeared(resolver, negatives) {
        return Some(format!("pre-image: {} appeared since the map phase", show_path(path)));
    }
    // N2: every found lookup of the map parse leads where it led, and the
    // inputs that were one file are one file. Without the record, fail
    // closed.
    let Some(found) = pre.shard.found.as_ref() else {
        return Some(
            "pre-image: the map shard has no record of where its lookups led (run `slip parse` again)"
                .to_owned(),
        );
    };
    negatives::found_moved(resolver, found, pre_inputs)
}

/// Step 2's guards of the overlay form (§5.1 "What guards it", §5.3), over
/// step 1's last parse, in this order; the reason of the first that fails:
///
/// - G4, on the observer's command (the post command);
/// - O1, the part only the unit answers ([`Loaded::not_overlaid`]): no
///   input that is an overlay file, by path or identity, was read from the
///   file system;
/// - O1 then O2 by path (`slip_verify::guards::overlay_inputs`): an input
///   the overlay names has the overlay's post content, one it does not and
///   the pre shard lists has the content listed there;
/// - the subject's sums (WP-074): each input of the pre shard that the
///   overlay names was the overlay's `pre` (the overlay was made from the
///   files the map phase read), and each that the post-image parse does not
///   list is still on disk as listed ([`left_behind`], with no subject
///   root: every such input is checked);
/// - N1 over every negative input of the pre shard, then N2 (with no
///   subject root, every place is checked).
///
/// G1, L1 and G6 guard the tree form's rewrite and do not run; G5 has
/// nothing to check (no map pair).
fn overlay_guards(
    params: &Params,
    observer: &Observer,
    pre: &Pre,
    post_inputs: &[FileRef],
    loaded: &Loaded<'_>,
    read: &[Read],
) -> Option<String> {
    if let Some(word) = response_file(&observer.command) {
        return Some(format!(
            "post-image: the command names the response file {}, whose words slip cannot examine",
            show_path(word)
        ));
    }
    if read.len() != post_inputs.len() {
        return Some("post-image: the parse's inputs are not those read".to_owned());
    }
    if let Some(path) = loaded.not_overlaid(post_inputs, read) {
        return Some(OverlayGuard::O1(path.to_vec()).reason());
    }
    let overlay: &Overlay = loaded.overlay();
    let pre_inputs = pre.shard.inputs.as_deref().unwrap_or_default();
    match overlay_inputs(pre_inputs, post_inputs, overlay) {
        Ok(()) => {}
        // An input that is an overlay file by identity (another name of
        // it): its bytes are the overlay's, not drift (WP-074 review F3).
        Err(OverlayGuard::O2(path)) => {
            return Some(match loaded.alias_of(&path) {
                Some(entry) => format!(
                    "pre-image: {} is another name of the overlay's {}",
                    show_path(&path),
                    show_path(entry)
                ),
                None => OverlayGuard::O2(path).reason(),
            });
        }
        Err(guard) => return Some(guard.reason()),
    }
    if let Some(f) = pre_inputs.iter().find(|f| {
        overlay.find(&f.path).is_some_and(|i| {
            let o = &overlay.files[i].pre;
            (o.size, o.content) != (f.size, f.content)
        })
    }) {
        return Some(format!(
            "pre-image: the overlay was made from another {} than the map phase read",
            show_path(&f.path)
        ));
    }
    let mut resolver = params.image.resolver();
    if let Some((path, how)) = left_behind(&mut resolver, post_inputs, pre_inputs) {
        return Some(how.reason(path));
    }
    negatives_guards(&mut resolver, pre, pre_inputs)
}

/// The guards of §5.3 that need only the post-image parse, in this order:
/// G1, L1, G4, G5, then G6 of the tree form (not with the identity). The
/// reason of the first that fails. `slip declare` and `slip units
/// --post-image` check them too (`spec/exec.md` §8.8).
pub(crate) fn post_parse_guards(
    image: &Image,
    resolver: &mut Resolver<'_>,
    command: &Command,
    post_inputs: &[FileRef],
    lookups: &Lookups,
) -> Option<String> {
    if let Some(read) = post_inputs.iter().find(|f| resolver.under_subject(&f.path)) {
        return Some(format!("post-image: read {}, a file of the subject", show_path(&read.path)));
    }
    let post = image.post_command(command);
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
    if image.is_identity() {
        return None;
    }
    g6(resolver, &post.command.directory, post_inputs, lookups)
}

/// Guard G6 (§5.3): its three parts in order, (i) over every file read
/// before (ii), then (iii).
fn g6(
    resolver: &mut Resolver<'_>,
    directory: &[u8],
    inputs: &[FileRef],
    lookups: &Lookups,
) -> Option<String> {
    resolver
        .link_differs(directory, lookups)
        .or_else(|| inputs.iter().find_map(|f| resolver.subject_route_link(&f.path)))
        .or_else(|| resolver.leaves_elsewhere(directory, lookups))
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

/// What became of a file the map parse read and the post-image parse did
/// not (O2's second part).
#[derive(Clone, Debug, PartialEq, Eq)]
enum Left {
    /// No regular file is at the path now.
    Gone,
    /// The path leads, through a link made since, to this other real
    /// path: not the file the map parse read there (two inputs made one by
    /// a link, or a directory on the way replaced by a link).
    Moved(Vec<u8>),
    /// The file there has another size or content, or cannot be read.
    Changed,
}

impl Left {
    /// The reason of the `unverifiable` verdict for the input at `path`.
    fn reason(&self, path: &[u8]) -> String {
        let path = show_path(path);
        match self {
            Left::Gone => {
                format!("pre-image: {path} is gone since the map phase (run `slip parse` again)")
            }
            Left::Moved(real) => format!(
                "pre-image: {path} now leads to {}, not the file the map parse read (run `slip parse` again)",
                show_path(real)
            ),
            Left::Changed => {
                format!("pre-image: {path} changed since the map phase (run `slip parse` again)")
            }
        }
    }
}

/// Guard O2's second part, of the tree form (§5.3): the first entry of the
/// pre shard's `inputs`, in path order, whose path the post-image parse's
/// `inputs` do not list (the same path identity as [`drifted`]: the bytes
/// of the path), that is not a file of the subject as recorded, and at
/// which there is no longer the file the map parse read: a regular file
/// whose real path is the recorded path, of the size and content digest
/// listed. Both lists are sorted by path. A file the map parse read and
/// the post parse did not is no file the comparison looks at: gone,
/// changed or reached elsewhere, the pre shard no longer describes the
/// pre-image (a header deleted, so that the search now finds another, the
/// post-image's edit compensating).
///
/// "A file of the subject as recorded": under a subject root
/// (`Resolver::under_subject`) judged on the recorded path only while that
/// path is still as recorded ([`as_recorded`]); through a link made since
/// (a directory of the dependency replaced by a link into the subject),
/// the entry is checked, not skipped.
fn left_behind<'a>(
    resolver: &mut Resolver<'_>,
    post: &[FileRef],
    pre: &'a [FileRef],
) -> Option<(&'a [u8], Left)> {
    pre.iter()
        .filter(|p| post.binary_search_by(|f| f.path.cmp(&p.path)).is_err())
        .filter(|p| !(as_recorded(&p.path) && resolver.under_subject(&p.path)))
        .find_map(|p| still_there(p).err().map(|how| (p.path.as_slice(), how)))
}

/// Whether `path`, a real path when the map parse recorded it, is still
/// one: the path, or the nearest ancestor of it that exists, is its own
/// real path now (no symbolic link has been made on the way since).
pub(crate) fn as_recorded(path: &[u8]) -> bool {
    let mut at = Path::new(OsStr::from_bytes(path));
    loop {
        if at.symlink_metadata().is_ok() {
            return std::fs::canonicalize(at).is_ok_and(|real| real == at);
        }
        match at.parent() {
            Some(up) => at = up,
            None => return true,
        }
    }
}

/// Whether the file the map parse read at `f.path` is still there: a
/// regular file (links followed now) whose real path is `f.path`, of `f`'s
/// size and content digest.
fn still_there(f: &FileRef) -> Result<(), Left> {
    let path = Path::new(OsStr::from_bytes(&f.path));
    match std::fs::metadata(path) {
        Ok(m) if m.is_file() => {
            match std::fs::canonicalize(path) {
                Ok(real) if real == path => {}
                Ok(real) => return Err(Left::Moved(real.into_os_string().into_vec())),
                Err(_) => return Err(Left::Gone),
            }
            if m.len() != f.size {
                return Err(Left::Changed);
            }
        }
        _ => return Err(Left::Gone),
    }
    match std::fs::read(path) {
        Ok(bytes) if bytes.len() as u64 == f.size && content_digest(&bytes) == f.content => Ok(()),
        _ => Err(Left::Changed),
    }
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
