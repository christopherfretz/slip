//! `slip check` (`spec/exec.md` §8.13, `spec/edits.md` §8, §10.2): from an
//! edit set to a published result. Steps apply, verify, scan, conclude.
//! Work package WP-076; its choices are `spec/landed/edits/WP-076.md`.
//!
//! The verdict and the state are the trusted base's: the run report
//! (`slip_verify::run::Report`) decides each observer's state, including
//! `unchanged` (`Report::add_unchanged_at`), and conclude
//! (`slip_verify::conclude`) the result's. This module decides which
//! observers get a verify job (§8.2) and gathers what they need.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};

use slip_apply::{ApplyError, Blobs as _};
use slip_exec::jobserver::Pool;
use slip_exec::pool::{self, JobExtra, Options, WorkerCommand};
use slip_exec::process::ProcSampler;
use slip_exec::store::{Access, Object, Store};
use slip_exec::worker::{Post, PostImage};
use slip_model::declared::Declared;
use slip_model::editset::{EditSet, SetState};
use slip_model::result::{SCHEME, State, TransformResult};
use slip_model::transform::{Transform, TransformKind};
use slip_model::transform_run::StepName;
use slip_model::{Digest, Overlay, Phase, Plan, Shard, Status};
use slip_verify::conclude::{Facts, conclude};
use slip_verify::run::{Header, Report};

use super::lines::edit_set_summary;
use super::map::{observers_at, pool_error, selection};
use super::report::{Details, details, result_summary};
use super::residue::{extensions, residue};
use super::run_record::{Clock, Record, install_signals, interrupted, run_id};
use super::store::{DiskFiles, StoreBlobs, StoreShards, request_of_round, selected_of_round};
use super::{CommandError, command_error, failure, read_checked, store_error};
use crate::args::{self, CheckArgs, TransformOptions};
use crate::commands::{self, open_store};
use crate::status;
use crate::verify_cmd;

/// What [`check`] runs.
#[derive(Debug)]
pub struct CheckInput<'a> {
    pub store: &'a Store,
    pub plan_id: Digest,
    pub plan: &'a Plan,
    pub round: u64,
    /// The observers the round's map phase selected: indices of the plan,
    /// ascending.
    pub selected: &'a [usize],
    /// Fewer observers selected than the plan has.
    pub partial: bool,
    pub transform_id: Digest,
    pub transform: &'a Transform,
    pub set_id: Digest,
    pub set: &'a EditSet,
    /// `--declared`, published; `None`: derived from the set (§6.2).
    pub declared: Option<(Digest, Declared)>,
    /// `--allow-unparsed`, `--residue-ext`.
    pub options: &'a TransformOptions,
    /// `--report`, `--findings`, `--locate` (`spec/exec.md` §8.7).
    pub report: Option<&'a Path>,
    pub findings: u64,
    pub locate: u64,
    pub pool: &'a Options,
    /// `parse`'s worker command; the verify phase appends `--findings`
    /// and `--locate`.
    pub worker: &'a WorkerCommand,
    pub tokens: &'a Pool,
}

/// What [`check`] made.
#[derive(Debug)]
pub struct Checked {
    pub result_id: Digest,
    pub result: TransformResult,
    pub declared: Option<Declared>,
    pub overlay: Option<Overlay>,
    /// The verify phase's report, if the phase finished.
    pub report: Option<Report>,
    /// The scan step ran.
    pub scanned: bool,
}

/// The declared changes of an accepted set when `--declared` is not given
/// (§10.2): derived from the set and the request's ownership
/// (`slip_reduce::declared`) and published. `None` for a refused set.
pub fn derived_declared(
    store: &Store,
    set: &EditSet,
    transform: &Transform,
) -> Result<Option<(Digest, Declared)>, CommandError> {
    let declared = slip_reduce::declared(set, &transform.ownership)
        .map_err(|e| CommandError::Input(format!("the edit set's declared changes: {e}")))?;
    match declared {
        None => Ok(None),
        Some(d) => {
            let id = store.publish_declared(&d.to_bytes()).map_err(|e| store_error(store, &e))?;
            Ok(Some((id, d)))
        }
    }
}

/// An `ApplyError` of `build` (WP-076): every one is an input error; a set
/// reduce would not have made is not turned into a `refused` result.
fn apply_error(e: ApplyError) -> CommandError {
    let path = |p: &[u8]| String::from_utf8_lossy(p).into_owned();
    CommandError::Input(match e {
        ApplyError::StaleFile(p) => {
            format!("{}: stale file: not the file the edit set names", path(&p))
        }
        ApplyError::TooLarge(p) => format!("{}: the post-image is too large", path(&p)),
        ApplyError::NotAccepted => "the edit set is refused".to_owned(),
        ApplyError::Invalid(why) => format!("the edit set cannot be applied: {why}"),
        ApplyError::Io(p, why) => format!("{}: {why}", path(&p)),
    })
}

/// What one pass over the round's map shards says of the selected
/// observers (WP-076): by selected position, whether the shard is `ok`
/// and whether it read an overlay file (§8.2); the owned paths some `ok`
/// shard read, sorted; and the main files of the others.
#[derive(Debug, Default)]
struct Pass {
    ok: Vec<bool>,
    read_overlay: Vec<bool>,
    read_by_some: Vec<Vec<u8>>,
    unparsed_mains: Vec<Vec<u8>>,
}

/// The observer's map shard of the round, if it is one: present, decoded,
/// of this observer, phase and round (`verify_cmd::read_map_shard`).
fn map_shard(
    store: &Store,
    plan_id: &Digest,
    round: u64,
    observer: &Digest,
) -> Result<Option<Shard>, CommandError> {
    let read =
        verify_cmd::read_map_shard(store, plan_id, round, observer).map_err(command_error)?;
    Ok(match read {
        Some(verify_cmd::MapRead::Shard(s, _)) => Some(*s),
        _ => None,
    })
}

fn map_pass(input: &CheckInput<'_>, overlay: &Overlay) -> Result<Pass, CommandError> {
    let mut pass = Pass::default();
    let mut read = BTreeSet::new();
    for &i in input.selected {
        let o = &input.plan.observers[i];
        let shard = map_shard(input.store, &input.plan_id, input.round, &o.id)?
            .filter(|s| s.outcome.status == Status::Ok);
        let inputs = shard.as_ref().and_then(|s| s.inputs.as_deref()).unwrap_or(&[]);
        pass.ok.push(shard.is_some());
        pass.read_overlay.push(inputs.iter().any(|f| overlay.find(&f.path).is_some()));
        if shard.is_none() {
            pass.unparsed_mains.push(o.main.path.clone());
        }
        for f in inputs {
            if input.transform.ownership.is_owned(&f.path) {
                read.insert(f.path.clone());
            }
        }
    }
    pass.read_by_some = read.into_iter().collect();
    Ok(pass)
}

/// §8.2: an observer whose map shard is `ok` gets a verify job when the
/// declared rules are not empty; with no rules, only one that read an
/// overlay file. The others are `unchanged`, which the report decides.
pub fn gets_job(ok: bool, has_rules: bool, read_overlay: bool) -> bool {
    ok && (has_rules || read_overlay)
}

/// The verify phase over the overlay and its report (§8.2, §8.3); `None`
/// when it was interrupted.
fn verify(
    input: &CheckInput<'_>,
    overlay: (Digest, &Overlay),
    declared: (Digest, &Declared),
    pass: &Pass,
    record: &mut Record,
    err: &mut dyn Write,
) -> Result<Option<Report>, CommandError> {
    let store = input.store;
    let clock = Clock::start(StepName::Verify);
    let has_rules = !declared.1.rules().rules().is_empty();
    let jobbed: Vec<bool> = (0..input.selected.len())
        .map(|k| gets_job(pass.ok[k], has_rules, pass.read_overlay[k]))
        .collect();
    let chosen: Vec<usize> =
        input.selected.iter().zip(&jobbed).filter(|(_, j)| **j).map(|(&i, _)| i).collect();
    let mut worker = input.worker.clone();
    worker.args.extend(verify_cmd::worker_options(input.findings, input.locate));
    let extra = JobExtra {
        transform: None,
        post: Some(Post {
            pre: input.round,
            declared: declared.0,
            image: PostImage::Overlay(overlay.0),
        }),
    };
    let run = pool::run_with(
        store,
        &input.plan_id,
        &observers_at(input.plan, &chosen),
        Phase::Verify,
        input.round,
        &extra,
        &worker,
        input.pool,
        input.tokens,
        &ProcSampler,
    );
    let summary = match run {
        Ok(s) => s,
        Err(e) => {
            record.steps.push(clock.end(None));
            return match pool_error(store, e, "verify") {
                CommandError::Interrupted => Ok(None),
                e => Err(e),
            };
        }
    };
    let header = Header {
        plan: input.plan_id,
        round: input.round,
        declared: declared.0,
        rules: declared.1.rules_id(),
        image: Vec::new(),
        limit: input.findings,
        locate: input.locate,
        plan_observers: input.plan.observers.len() as u64,
        run: args::os_bytes(summary.run_path.as_os_str()).into_owned(),
    };
    let mut report = Report::new(header, declared.1);
    report.overlay(overlay.0, overlay.1);
    report.pre_round(input.round);
    let last = verify_cmd::last_shards(&summary.run);
    for (k, &i) in input.selected.iter().enumerate() {
        if !pass.ok[k] {
            // Unparsed: counted by the result, no state in the report.
            continue;
        }
        let o = &input.plan.observers[i];
        let position = i as u64;
        if jobbed[k] {
            let shard = verify_cmd::this_runs_shard(store, &input.plan_id, input.round, o, &last)
                .map_err(command_error)?;
            // The map shard as the store has it now, read again after the
            // phase: a verdict is believed only against the one whose
            // content id it names as its `pre` (verify.md §9.3, WP-114).
            let map = verify_cmd::map_status(store, &input.plan_id, input.round, &o.id)
                .map_err(command_error)?;
            report.add_at(position, &o.id, &o.main.path, shard.as_ref(), map);
        } else {
            match map_shard(store, &input.plan_id, input.round, &o.id)? {
                Some(map) => report.add_unchanged_at(position, &o.id, &o.main.path, &map),
                None => report.add_at(position, &o.id, &o.main.path, None, None),
            }
        }
    }
    record.steps.push(clock.end(run_id(&summary.run_path)));
    let _ = err.write_all(report.error_lines().as_bytes());
    if let Some(path) = input.report {
        verify_cmd::write_report(path, &report.to_json()).map_err(command_error)?;
    }
    Ok(Some(report))
}

/// The threads the jobserver gives without waiting, at least one (as
/// reduce takes them).
fn with_threads<T>(tokens: &Pool, f: impl FnOnce(usize) -> T) -> T {
    let mut held = Vec::new();
    while held.len() < tokens.jobs() {
        match tokens.try_acquire() {
            Some(t) => held.push(t),
            None => break,
        }
    }
    let value = f(held.len().max(1));
    drop(held);
    value
}

/// Steps apply, verify, scan and conclude (§8.1) over an edit set of the
/// round; publishes the overlay, its blobs, the declared changes when
/// derived, and the result, and prints from the `overlay` line to the
/// `state` line of §10.5. A refused set runs none of apply, verify and
/// scan (§8.1). An interruption (`spec/exec.md` §7) skips verify and scan:
/// the result is concluded without a report, `incomplete` (§10.6).
pub fn check(
    input: &CheckInput<'_>,
    record: &mut Record,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<Checked, CommandError> {
    let store = input.store;
    let set = input.set;
    let declared = match (&input.declared, set.state) {
        (_, SetState::Refused) => None,
        (Some(d), _) => Some(d.clone()),
        (None, _) => derived_declared(store, set, input.transform)?,
    };
    record.declared = declared.as_ref().map(|d| d.0);

    let mut overlay: Option<(Digest, Overlay)> = None;
    let mut report = None;
    let mut found = Vec::new();
    let mut scanned = false;
    if let Some((declared_id, d)) = &declared {
        let clock = Clock::start(StepName::Apply);
        let blobs = StoreBlobs(store);
        let built = slip_apply::build_into(set, input.set_id, &DiskFiles, &mut |_, bytes| {
            blobs.put(bytes).map(|_| ())
        })
        .map_err(apply_error)?;
        let id = store
            .publish_object(Object::Overlay, &built.to_bytes())
            .map_err(|e| store_error(store, &e))?;
        record.overlay = Some(id);
        record.steps.push(clock.end(None));
        let _ = writeln!(
            out,
            "overlay {id}: files {}, spellings {}",
            built.files.len(),
            built.spellings.len()
        );
        let _ = out.flush();
        let pass = map_pass(input, &built)?;
        if !interrupted() {
            report = verify(input, (id, &built), (*declared_id, d), &pass, record, err)?;
        }
        if report.is_some() && input.transform.kind == TransformKind::Rename && !interrupted() {
            let clock = Clock::start(StepName::Scan);
            let mains: &[Vec<u8>] =
                if input.options.allow_unparsed { &pass.unparsed_mains } else { &[] };
            let ext = extensions(input.options.residue_ext.as_deref());
            found = with_threads(input.tokens, |threads| {
                residue(input.transform, set, &pass.read_by_some, mains, &ext, threads)
            })
            .map_err(|e| CommandError::Input(format!("residue: {e}")))?;
            record.steps.push(clock.end(None));
            scanned = true;
        }
        overlay = Some((id, built));
    }

    let clock = Clock::start(StepName::Conclude);
    let facts = Facts {
        set,
        selected: input.selected.len() as u64,
        report: report.as_ref(),
        allow_unparsed: input.options.allow_unparsed,
        residue: &found,
    };
    let c = conclude(&facts, overlay.as_ref().map(|o| &o.1));
    let result = TransformResult {
        scheme: SCHEME,
        plan: input.plan_id,
        round: input.round,
        transform: input.transform_id,
        edits: input.set_id,
        declared: declared.as_ref().map(|d| d.0),
        overlay: overlay.as_ref().map(|o| o.0),
        state: c.state,
        partial: input.partial,
        allow_unparsed: input.options.allow_unparsed,
        verdict: c.verdict,
        tally: c.tally,
        agreement: c.agreement,
        unused: c.unused,
        spellings: c.spellings,
        residue: found,
        exempt: declared.as_ref().map(|d| d.1.changed().to_vec()).unwrap_or_default(),
    };
    let result_id = store
        .publish_object(Object::Result, &result.to_bytes())
        .map_err(|e| store_error(store, &e))?;
    record.result = Some(result_id);
    record.steps.push(clock.end(None));
    result_summary(result_id, &result, scanned, out);
    let _ = out.flush();
    Ok(Checked {
        result_id,
        result,
        declared: declared.map(|d| d.1),
        overlay: overlay.map(|o| o.1),
        report,
        scanned,
    })
}

/// The exit status of `rename` and `check` (§10.5) for a result's state:
/// `wrote` says whether what `--write` or `--patch` asked was done (`true`
/// when nothing was asked).
pub fn status_of(state: State, wrote: bool) -> u8 {
    match state {
        State::Incomplete => status::INCOMPLETE,
        State::Verified if wrote => status::OK,
        State::Verified | State::VerifiedWithResidue | State::NotVerified | State::Refused => {
            status::FINDINGS
        }
    }
}

/// `--edits <id or file>` or `--declared <id or file>` (§10): an id
/// prefix of 4 to 64 hexadecimal digits, which must match exactly one
/// file of its kind in the store, or else a file, which is read, checked
/// and published.
/// Reads a file given for an object, checks and publishes it.
type FromFile<'a, T> = dyn Fn(&[u8]) -> Result<(Digest, T), String> + 'a;

fn id_or_file<T>(
    store: &Store,
    x: &OsString,
    what: &str,
    find: &dyn Fn(&str) -> Result<Vec<Digest>, CommandError>,
    by_id: &dyn Fn(&Digest) -> Result<T, CommandError>,
    by_file: &FromFile<'_, T>,
) -> Result<(Digest, T), CommandError> {
    if let Some(prefix) = x
        .to_str()
        .filter(|t| (4..=64).contains(&t.len()) && t.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        let prefix = prefix.to_ascii_lowercase();
        let ids = find(&prefix)?;
        return match ids.as_slice() {
            [id] => Ok((*id, by_id(id)?)),
            [] => Err(CommandError::Input(format!(
                "{what} {prefix}: none in the store {} has an id starting with it",
                store.root().display()
            ))),
            several => Err(CommandError::Input(format!(
                "{what} {prefix}: {} have an id starting with it",
                several.len()
            ))),
        };
    }
    let path = PathBuf::from(x);
    let bytes = std::fs::read(&path)
        .map_err(|e| CommandError::Input(format!("{}: {e}", path.display())))?;
    by_file(&bytes).map_err(|e| CommandError::Input(format!("{}: {e}", path.display())))
}

fn load_edits(store: &Store, x: &OsString) -> Result<(Digest, EditSet), CommandError> {
    id_or_file(
        store,
        x,
        "edit set",
        &|p| store.find_object(Object::EditSet, p).map_err(|e| store_error(store, &e)),
        &|id| read_checked(store, Object::EditSet, id, "the edit set", EditSet::from_bytes),
        &|bytes| {
            let set = EditSet::from_bytes(bytes).map_err(|e| e.to_string())?;
            let id = store.publish_object(Object::EditSet, bytes).map_err(|e| e.to_string())?;
            Ok((id, set))
        },
    )
}

fn load_declared(store: &Store, x: &OsString) -> Result<(Digest, Declared), CommandError> {
    let decode = |bytes: &[u8]| -> Result<Declared, String> {
        let d = Declared::from_bytes(bytes).map_err(|e| e.to_string())?;
        // As `verify --declared` (`spec/verify.md` §2.1).
        match d.rules().refused() {
            Some((index, why)) => Err(format!("rule {index}: {why}")),
            None => Ok(d),
        }
    };
    id_or_file(
        store,
        x,
        "declared changes",
        &|p| store.find_declared(p).map_err(|e| store_error(store, &e)),
        &|id| {
            let path = store.declared_path(id);
            let bytes =
                store.read_declared(id).map_err(|e| store_error(store, &e))?.ok_or_else(|| {
                    CommandError::Input(format!("{}: not in the store", path.display()))
                })?;
            decode(&bytes).map_err(|e| CommandError::Input(format!("{}: {e}", path.display())))
        },
        &|bytes| {
            let d = decode(bytes)?;
            let id = store.publish_declared(bytes).map_err(|e| e.to_string())?;
            Ok((id, d))
        },
    )
}

/// `slip check` (§10.2, §10.5).
pub(crate) fn main(
    a: &CheckArgs,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> commands::Result<u8> {
    let store = open_store(&a.run.store, Access::Exclusive)?;
    let published = install_signals(&store, || {});
    let mut record = Record::new();
    let (plan, plan_id) = commands::load_plan(&store, &a.plan)?;
    let selected = selected_of_round(&store, plan_id, &plan, a.round).map_err(failure)?;
    let shards = StoreShards::new(&store, plan_id, &plan, a.round, &selected);
    let (transform_id, transform) =
        request_of_round(&store, plan_id, a.round, &shards).map_err(failure)?;
    if (!a.run.only.is_empty() || a.run.limit.is_some())
        && selection(&plan, &a.run, err) != selected
    {
        return Err(commands::Failure::Input(format!(
            "plan {plan_id} round {}: `--only` and `--limit` select other observers than the \
             round's map phase did",
            a.round
        )));
    }
    let (set_id, set) = load_edits(&store, &a.edits).map_err(failure)?;
    let mismatch = [
        ("plan", set.plan != plan_id),
        ("round", set.round != a.round),
        ("transform", set.transform != transform_id),
    ]
    .into_iter()
    .find(|(_, differs)| *differs);
    if let Some((field, _)) = mismatch {
        return Err(commands::Failure::Input(format!(
            "edit set {set_id}: its {field} is not the round's (plan {plan_id}, round {}, \
             transform {transform_id})",
            a.round
        )));
    }
    let declared = match (&a.declared, set.state) {
        (Some(x), SetState::Accepted) => Some(load_declared(&store, x).map_err(failure)?),
        (_, SetState::Accepted) => derived_declared(&store, &set, &transform).map_err(failure)?,
        (_, SetState::Refused) => None,
    };

    let _ = writeln!(out, "transform {transform_id}");
    let _ = writeln!(out, "round {}", a.round);
    edit_set_summary(
        set_id,
        &set,
        declared.as_ref().map(|(id, d)| (*id, d)),
        transform.indivisible,
        out,
    );
    let tokens = commands::token_pool(a.run.jobs);
    let options = commands::pool_options(&a.run, tokens.jobs(), err);
    let worker = commands::worker_command(&a.run)?;
    record.edits = Some(set_id);
    let input = CheckInput {
        store: &store,
        plan_id,
        plan: &plan,
        round: a.round,
        selected: &selected,
        partial: selected.len() < plan.observers.len(),
        transform_id,
        transform: &transform,
        set_id,
        set: &set,
        declared,
        options: &a.options,
        report: a.report.as_deref(),
        findings: a.findings,
        locate: a.locate,
        pool: &options,
        worker: &worker,
        tokens: &tokens,
    };
    let checked = check(&input, &mut record, out, err);
    let published_record = record.publish(&store, plan_id, a.round, transform_id);
    drop(published);
    let checked = checked.map_err(failure)?;
    published_record.map_err(failure)?;
    if interrupted() {
        return Err(commands::Failure::Interrupted);
    }
    details(
        &Details {
            store: &store,
            plan: &plan,
            set: &set,
            set_record: &store.object_path(Object::EditSet, &set_id),
            declared: checked.declared.as_ref(),
            result: &checked.result,
            result_record: &store.object_path(Object::Result, &checked.result_id),
            overlay: checked.overlay.as_ref(),
            report: checked.report.as_ref(),
            report_file: a.report.as_deref(),
            relative_to: None,
        },
        out,
    );
    Ok(status_of(checked.result.state, true))
}
