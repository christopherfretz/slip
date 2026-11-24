//! `slip map` (`spec/exec.md` §8.11, `spec/edits.md` §10.1): the request
//! built and published, a new round, the map phase with jobs that carry
//! the request's id, the transform run record, and the first three lines
//! of §10.5. Work package WP-076.

use std::collections::HashMap;
use std::io::Write;

use slip_exec::jobserver::Pool;
use slip_exec::pool::{self, JobExtra, Options, PoolError, WorkerCommand};
use slip_exec::process::ProcSampler;
use slip_exec::store::{Access, Object, Store};
use slip_model::transform::{Ownership, Rename, TargetKind, Transform, TransformKind};
use slip_model::transform_run::{Step, StepName};
use slip_model::{Digest, Observer, Phase, Plan, Shard, Status};

use super::run_record::{Clock, Record, install_signals, run_id};
use super::{CommandError, command_error, failure, store_error};
use crate::args::{self, MapArgs, MapTransform, PlanSource, RunArgs, TransformOptions};
use crate::commands::{self, Failure, open_store};
use crate::status;

/// What [`map`] runs.
#[derive(Debug)]
pub struct MapInput<'a> {
    pub store: &'a Store,
    pub plan_id: Digest,
    pub plan: &'a Plan,
    /// Indices of `plan.observers`, ascending.
    pub selected: &'a [usize],
    pub transform_id: Digest,
    pub transform: &'a Transform,
    /// The round, taken by the caller (`Store::next_round`) so that its
    /// run record can name it whatever happens to the phase.
    pub round: u64,
    pub pool: &'a Options,
    pub worker: &'a WorkerCommand,
    pub tokens: &'a Pool,
}

/// The counts of §10.5's `map:` line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapCounts {
    pub selected: u64,
    pub ok: u64,
    pub not_ok: u64,
    /// The `ok` shards whose `edits` have a `failure` (§4.6).
    pub failed_runs: u64,
}

/// A pool's error as the command's: `what` names the phase.
pub(crate) fn pool_error(store: &Store, e: PoolError, what: &str) -> CommandError {
    match e {
        PoolError::Cancelled => CommandError::Interrupted,
        PoolError::Store(e) => store_error(store, &e),
        PoolError::Io(e) => CommandError::Input(format!("{what}: {e}")),
        PoolError::Refused(why) => unreachable!("{what}: the task refuses the jobs: {why}"),
    }
}

/// The observers at `selected` (indices of the plan).
pub(crate) fn observers_at(plan: &Plan, selected: &[usize]) -> Vec<Observer> {
    selected.iter().map(|&i| plan.observers[i].clone()).collect()
}

/// Prints `transform <id>` and `round <n>`, runs the map phase of round
/// `input.round` and prints the `map:` line; one `map` step. The lines of
/// the observers whose shard is not `ok` go to `err` (`spec/exec.md` §8.2).
pub fn map(
    input: &MapInput<'_>,
    record: &mut Vec<Step>,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<(u64, MapCounts), CommandError> {
    let store = input.store;
    let _ = writeln!(out, "transform {}", input.transform_id);
    let _ = writeln!(out, "round {}", input.round);
    let _ = out.flush();
    let observers = observers_at(input.plan, input.selected);
    let clock = Clock::start(StepName::Map);
    let extra = JobExtra { transform: Some(input.transform_id), post: None };
    let run = pool::run_with(
        store,
        &input.plan_id,
        &observers,
        Phase::Map,
        input.round,
        &extra,
        input.worker,
        input.pool,
        input.tokens,
        &ProcSampler,
    );
    let summary = match run {
        Ok(s) => s,
        Err(e) => {
            record.push(clock.end(None));
            return Err(pool_error(store, e, "map"));
        }
    };
    record.push(clock.end(run_id(&summary.run_path)));
    let mut counts =
        MapCounts { selected: observers.len() as u64, ok: 0, not_ok: 0, failed_runs: 0 };
    for o in &summary.outcomes {
        if o.status != Status::Ok {
            counts.not_ok += 1;
            continue;
        }
        counts.ok += 1;
        let bytes = store
            .read_shard(&input.plan_id, Phase::Map, input.round, &o.observer)
            .map_err(|e| store_error(store, &e))?;
        let failed = bytes
            .and_then(|b| Shard::from_bytes(&b).ok())
            .and_then(|s| s.edits)
            .is_some_and(|e| e.failure.is_some());
        counts.failed_runs += u64::from(failed);
    }
    // An observer the pool reported nothing for is not `ok` either.
    counts.not_ok = counts.selected - counts.ok;
    commands::report_not_ok(&summary, &observers, err);
    let _ = writeln!(
        out,
        "map: selected {}, ok {}, not ok {}, failed runs {}",
        counts.selected, counts.ok, counts.not_ok, counts.failed_runs
    );
    let _ = out.flush();
    Ok((input.round, counts))
}

/// §3: the request of a rename from `--from`, `--to` and `--kind`, with the
/// ownership of `--owned` and `--readonly` made real paths. `--from` is
/// split at `::`, a leading `::` ignored (§10.1). A name §3 refuses is a
/// usage error naming the option, decided before any path is resolved.
pub fn rename_request(
    from: &[u8],
    to: &[u8],
    kind: Option<TargetKind>,
    options: &TransformOptions,
) -> Result<Transform, CommandError> {
    let from_rest = from.strip_prefix(b"::").unwrap_or(from);
    let target: Vec<Vec<u8>> =
        if from_rest.is_empty() { Vec::new() } else { split_qualified(from_rest) };
    let mut request = Transform {
        kind: TransformKind::Rename,
        // A placeholder, for checking the names alone.
        ownership: Ownership { owned: vec![b"/".to_vec()], readonly: Vec::new() },
        indivisible: true,
        rename: Some(Rename {
            target,
            kind: kind.unwrap_or(TargetKind::Any),
            new_name: to.to_vec(),
        }),
        script: None,
    };
    if let Err(e) = request.validate() {
        let why = e.split_once(": ").map_or(e.as_str(), |(_, w)| w);
        let (option, value) = if e.contains("new_name") { ("--to", to) } else { ("--from", from) };
        return Err(CommandError::Usage(format!(
            "`{option}`: `{}`: {why}",
            String::from_utf8_lossy(value)
        )));
    }
    request.ownership = commands::ownership(&options.owned, &options.readonly)
        .map_err(command_error)?
        .ok_or_else(|| CommandError::Usage("a transform needs `--owned`".to_owned()))?;
    request.validate().map_err(|e| CommandError::Input(format!("the request: {e}")))?;
    Ok(request)
}

/// `a::b::c` as its components; an empty component stays (and is refused
/// as no identifier).
fn split_qualified(name: &[u8]) -> Vec<Vec<u8>> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i + 1 < name.len() {
        if &name[i..i + 2] == b"::" {
            parts.push(name[start..i].to_vec());
            i += 2;
            start = i;
        } else {
            i += 1;
        }
    }
    parts.push(name[start..].to_vec());
    parts
}

/// The plan of `--plan` or `--db` (§10.1 step 1): the plan lines of
/// `parse` and a `plan` step when it is built.
pub(crate) fn plan_of(
    source: &PlanSource,
    run: &RunArgs,
    store: &Store,
    steps: &mut Vec<Step>,
    out: &mut dyn Write,
) -> commands::Result<(Plan, Digest, u8)> {
    match source {
        PlanSource::Db(dbs) => {
            let clock = Clock::start(StepName::Plan);
            let built = commands::build_and_publish(dbs, run.jobs, store, out)?;
            steps.push(clock.end(None));
            Ok(built)
        }
        PlanSource::Plan(x) => {
            let (plan, id) = commands::load_plan(store, x)?;
            Ok((plan, id, status::OK))
        }
    }
}

/// The selection of `--only` and `--limit` (`spec/exec.md` §8.2) as
/// indices of the plan, ascending.
pub(crate) fn selection(plan: &Plan, run: &RunArgs, err: &mut dyn Write) -> Vec<usize> {
    let index: HashMap<Digest, usize> =
        plan.observers.iter().enumerate().map(|(i, o)| (o.id, i)).collect();
    commands::select_for_phase(plan, run, err).iter().map(|o| index[&o.id]).collect()
}

/// The request of `map` or `rename`, checked before the store is opened.
pub(crate) fn request_of(
    target: &args::RenameTarget,
    options: &TransformOptions,
) -> commands::Result<Transform> {
    rename_request(&args::os_bytes(&target.from), &args::os_bytes(&target.to), target.kind, options)
        .map_err(failure)
}

/// The plan, the request published and a new round: what `map` and
/// `rename` do before the map phase (§10.1 steps 1 to 3), on a store
/// opened exclusively.
pub(crate) struct Started {
    pub plan: Plan,
    pub plan_id: Digest,
    pub plan_code: u8,
    pub selected: Vec<usize>,
    pub transform_id: Digest,
    pub round: u64,
}

pub(crate) fn start(
    store: &Store,
    source: &PlanSource,
    run: &RunArgs,
    request: &Transform,
    record: &mut Record,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> commands::Result<Started> {
    let (plan, plan_id, plan_code) = plan_of(source, run, store, &mut record.steps, out)?;
    let selected = selection(&plan, run, err);
    let transform_id = store
        .publish_object(Object::Transform, &request.to_bytes())
        .map_err(|e| commands::store_failure(store.root(), &e))?;
    let round =
        store.next_round(&plan_id).map_err(|e| commands::store_failure(store.root(), &e))?;
    Ok(Started { plan, plan_id, plan_code, selected, transform_id, round })
}

/// `slip map` (§10.1, §10.5): status 0 when every selected observer's
/// shard is `ok` and no run failed, else 1 (with the plan's own status).
pub(crate) fn main(a: &MapArgs, out: &mut dyn Write, err: &mut dyn Write) -> commands::Result<u8> {
    let target = match &a.transform {
        MapTransform::Rename(t) => t,
        MapTransform::Script { .. } => {
            return Err(Failure::Usage(
                "map: --transform needs a slip built with slip_script".to_owned(),
            ));
        }
    };
    let request = request_of(target, &a.options)?;
    let mut record = Record::new();
    let store = open_store(&a.run.store, Access::Exclusive)?;
    let published = install_signals(&store, || {});
    let s = start(&store, &a.source, &a.run, &request, &mut record, out, err)?;
    let tokens = commands::token_pool(a.run.jobs);
    let options = commands::pool_options(&a.run, tokens.jobs(), err);
    let worker = commands::worker_command(&a.run)?;
    let input = MapInput {
        store: &store,
        plan_id: s.plan_id,
        plan: &s.plan,
        selected: &s.selected,
        transform_id: s.transform_id,
        transform: &request,
        round: s.round,
        pool: &options,
        worker: &worker,
        tokens: &tokens,
    };
    let mapped = map(&input, &mut record.steps, out, err);
    let published_record = record.publish(&store, s.plan_id, s.round, s.transform_id);
    drop(published);
    let (_, counts) = mapped.map_err(failure)?;
    published_record.map_err(failure)?;
    let code =
        if counts.not_ok == 0 && counts.failed_runs == 0 { status::OK } else { status::FINDINGS };
    Ok(code.max(s.plan_code))
}
