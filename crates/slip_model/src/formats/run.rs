//! The run record, kind `RUNR` (`spec/model.md` §9).

use crate::json::Value;
use crate::wire::{Decoder, Encoder};
use crate::{Digest, Job, Run, RunRecord};

use super::common::{
    JResult, Result, dec_enum, dec_list, dec_opt, enc_enum, json_digest, json_enum, json_uv,
    object, read_digest, read_enum, read_list, read_record, read_str, read_uv,
};
use super::{At, Extra, OwnedExtras, SectionName, open, read_container, render_container, seal};

const RUN: u64 = 1;
const JOBS: u64 = 2;

/// The run record has no invariants beyond its encodings (§9).
#[allow(clippy::unnecessary_wraps)]
pub(super) fn check(_: &RunRecord) -> std::result::Result<(), (String, &'static str)> {
    Ok(())
}

// ---------------------------------------------------------------------------
// Binary

pub(super) fn encode(r: &RunRecord, extras: &[Extra<'_>]) -> Vec<u8> {
    let x = &r.run;
    let mut run = Encoder::new();
    run.str(&x.tool_slip).str(&x.tool_kiln).digest(&x.plan);
    enc_enum(&mut run, x.phase);
    run.uv(x.started_unix_ns)
        .uv(x.wall_ns)
        .uv(x.cores)
        .uv(x.workers_max)
        .uv(x.memory_budget)
        .uv(x.peak_rss_sum);
    let mut jobs = Encoder::new();
    jobs.count(r.jobs.len());
    for j in &r.jobs {
        jobs.digest(&j.observer).uv(j.attempt);
        enc_enum(&mut jobs, j.end);
        match &j.shard {
            None => jobs.u8(0),
            Some(d) => jobs.u8(1).digest(d),
        };
        jobs.uv(j.queued_ns).uv(j.wall_ns).uv(j.cpu_ns).uv(j.peak_rss).uv(j.estimate);
    }
    seal(
        RunRecord::KIND,
        RunRecord::VERSION,
        &[(RUN, run.as_bytes()), (JOBS, jobs.as_bytes())],
        extras,
    )
}

/// `opt<digest>` (§2): `Malformed` at the tag byte unless it is 0 or 1.
fn dec_opt_digest(d: &mut Decoder<'_>) -> Result<Option<Digest>> {
    dec_opt(d, Decoder::digest)
}

/// The run record, its unknown ignorable sections and its content id.
pub(super) fn decode(bytes: &[u8]) -> Result<(RunRecord, Vec<Extra<'_>>, Digest)> {
    let opened = open(bytes, RunRecord::KIND, RunRecord::VERSION, [RUN, JOBS])?;
    let end = opened.end;
    let [run, jobs] = opened.sections;

    let mut d = super::required(run, end, "run record has no run section")?;
    let run = Run {
        tool_slip: d.str()?.to_owned(),
        tool_kiln: d.str()?.to_owned(),
        plan: d.digest()?,
        phase: dec_enum(&mut d)?,
        started_unix_ns: d.uv()?,
        wall_ns: d.uv()?,
        cores: d.uv()?,
        workers_max: d.uv()?,
        memory_budget: d.uv()?,
        peak_rss_sum: d.uv()?,
    };
    d.finish()?;

    let mut d = super::required(jobs, end, "run record has no jobs section")?;
    let jobs = dec_list(&mut d, None, |d| {
        Ok(Job {
            observer: d.digest()?,
            attempt: d.uv()?,
            end: dec_enum(d)?,
            shard: dec_opt_digest(d)?,
            queued_ns: d.uv()?,
            wall_ns: d.uv()?,
            cpu_ns: d.uv()?,
            peak_rss: d.uv()?,
            estimate: d.uv()?,
        })
    })?;
    d.finish()?;

    Ok((RunRecord { run, jobs }, opened.extras, opened.content_id))
}

// ---------------------------------------------------------------------------
// JSON

const NAMES: [SectionName; 2] = [
    SectionName { id: RUN, name: "run", required: true },
    SectionName { id: JOBS, name: "jobs", required: true },
];

const RUN_FIELDS: [&str; 10] = [
    "tool_slip",
    "tool_kiln",
    "plan",
    "phase",
    "started_unix_ns",
    "wall_ns",
    "cores",
    "workers_max",
    "memory_budget",
    "peak_rss_sum",
];

const JOB_FIELDS: [&str; 9] = [
    "observer",
    "attempt",
    "end",
    "shard",
    "queued_ns",
    "wall_ns",
    "cpu_ns",
    "peak_rss",
    "estimate",
];

pub(super) fn render(r: &RunRecord, extras: &[Extra<'_>]) -> Value {
    let x = &r.run;
    let run = object(
        RUN_FIELDS
            .into_iter()
            .zip([
                Value::Str(x.tool_slip.clone()),
                Value::Str(x.tool_kiln.clone()),
                json_digest(&x.plan),
                json_enum(x.phase),
                json_uv(x.started_unix_ns),
                json_uv(x.wall_ns),
                json_uv(x.cores),
                json_uv(x.workers_max),
                json_uv(x.memory_budget),
                json_uv(x.peak_rss_sum),
            ])
            .collect(),
    );
    let jobs = r
        .jobs
        .iter()
        .map(|j| {
            object(
                JOB_FIELDS
                    .into_iter()
                    .zip([
                        json_digest(&j.observer),
                        json_uv(j.attempt),
                        json_enum(j.end),
                        j.shard.as_ref().map_or(Value::Null, json_digest),
                        json_uv(j.queued_ns),
                        json_uv(j.wall_ns),
                        json_uv(j.cpu_ns),
                        json_uv(j.peak_rss),
                        json_uv(j.estimate),
                    ])
                    .collect(),
            )
        })
        .collect();
    render_container(
        RunRecord::KIND,
        RunRecord::VERSION,
        vec![("run", run), ("jobs", Value::Array(jobs))],
        extras,
    )
}

/// The run record and the unknown ignorable sections of a JSON rendering.
pub(super) fn read(v: &Value) -> JResult<(RunRecord, OwnedExtras)> {
    let s = read_container(v, RunRecord::KIND, RunRecord::VERSION, &NAMES)?;
    let [Some(run), Some(jobs)] = s.known else {
        unreachable!("read_container returns every required section")
    };
    let root = At::Root;
    let sections = root.key("sections");

    let at = sections.key("run");
    let [tool_slip, tool_kiln, plan, phase, started, wall, cores, workers, budget, peak] =
        read_record(run, &at, RUN_FIELDS)?;
    let run = Run {
        tool_slip: read_str(tool_slip, &at.key(RUN_FIELDS[0]))?.to_owned(),
        tool_kiln: read_str(tool_kiln, &at.key(RUN_FIELDS[1]))?.to_owned(),
        plan: read_digest(plan, &at.key(RUN_FIELDS[2]))?,
        phase: read_enum(phase, &at.key(RUN_FIELDS[3]))?,
        started_unix_ns: read_uv(started, &at.key(RUN_FIELDS[4]))?,
        wall_ns: read_uv(wall, &at.key(RUN_FIELDS[5]))?,
        cores: read_uv(cores, &at.key(RUN_FIELDS[6]))?,
        workers_max: read_uv(workers, &at.key(RUN_FIELDS[7]))?,
        memory_budget: read_uv(budget, &at.key(RUN_FIELDS[8]))?,
        peak_rss_sum: read_uv(peak, &at.key(RUN_FIELDS[9]))?,
    };

    let jobs = read_list(jobs, &sections.key("jobs"), |v, at| {
        let [observer, attempt, end, shard, queued, wall, cpu, peak, estimate] =
            read_record(v, at, JOB_FIELDS)?;
        let shard = match shard {
            Value::Null => None,
            v => Some(read_digest(v, &at.key("shard"))?),
        };
        Ok(Job {
            observer: read_digest(observer, &at.key(JOB_FIELDS[0]))?,
            attempt: read_uv(attempt, &at.key(JOB_FIELDS[1]))?,
            end: read_enum(end, &at.key(JOB_FIELDS[2]))?,
            shard,
            queued_ns: read_uv(queued, &at.key(JOB_FIELDS[4]))?,
            wall_ns: read_uv(wall, &at.key(JOB_FIELDS[5]))?,
            cpu_ns: read_uv(cpu, &at.key(JOB_FIELDS[6]))?,
            peak_rss: read_uv(peak, &at.key(JOB_FIELDS[7]))?,
            estimate: read_uv(estimate, &at.key(JOB_FIELDS[8]))?,
        })
    })?;

    Ok((RunRecord { run, jobs }, s.extras))
}
