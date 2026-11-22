//! `cargo xtask perf summarize|record|check` (`spec/harness.md` §7): what a parallel phase's run record,
//! its plan and its shards say about a run, and the committed baseline a
//! corpus's counters are gated against.
//!
//! ```text
//! cargo xtask perf summarize <run record>…
//! cargo xtask perf record <corpus> <run record>
//! cargo xtask perf check  <corpus> <run record>
//! ```
//!
//! A run record is named by its path in the store,
//! `<store>/runs/<plan id>/<phase>.<round>.<started>.run` ([`parse_run_path`]);
//! [`load`] reads it, the plan and the run's shards from that store opened
//! shared. Every file is untrusted: anything malformed is an error naming
//! the file, never a panic. Counters and seconds are computed with integer
//! arithmetic, and an overflow is an error.
//!
//! [`render`] holds the three outputs: the summary (§7.2), the baseline and
//! the check (§7.3).

pub mod render;

pub use render::{
    Baseline, MEASURED, Num, check, read_baseline, render_baseline, render_summary, seconds,
};

use crate::corpus::rows;
use slip_exec::store::{self, Access, Store};
use slip_model::{Digest, End, Phase, Plan, Reason, RunRecord, Shard, Status};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "\
usage: cargo xtask perf summarize <run record>…
       cargo xtask perf record <corpus> <run record>
       cargo xtask perf check <corpus> <run record>

A run record is named by its path in the store:
<store>/runs/<plan id>/<phase>.<round>.<started>.run";

/// Where baselines live, relative to the repository root.
pub const BASELINES: &str = "bench/baselines";

/// The parts of a run record's path (§7, first paragraph).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunPath {
    /// The directory three levels above the record (`.` when the path has
    /// no more components than `runs/<plan>/<file>`).
    pub store: PathBuf,
    pub plan: Digest,
    pub phase: Phase,
    pub round: u64,
    pub started_unix_ns: u64,
}

/// A canonical decimal `u64`: `0`, or digits without a leading zero.
fn canonical_u64(text: &str) -> Option<u64> {
    let canonical = text == "0"
        || (!text.is_empty() && !text.starts_with('0') && text.bytes().all(|b| b.is_ascii_digit()));
    if canonical { text.parse().ok() } else { None }
}

/// Splits `path` into the store, the plan, the phase, the round and the
/// start time its name gives: `<store>/runs/<64 hex>/<map|verify>.<round>.<started>.run`,
/// the numbers canonical decimal `u64`. Any other shape is an error naming
/// the path and the part that is wrong.
pub fn parse_run_path(path: &Path) -> Result<RunPath, String> {
    let shown = path.display();
    let expected = "<store>/runs/<plan id>/<phase>.<round>.<started>.run";
    let mut parts: Vec<Component<'_>> = path.components().collect();
    let names: Vec<&str> = parts
        .iter()
        .rev()
        .take(3)
        .map(|c| match c {
            Component::Normal(n) => n.to_str().unwrap_or(""),
            _ => "",
        })
        .collect();
    if names.len() < 3 || names.iter().any(|n| n.is_empty()) {
        return Err(format!("{shown}: not a run record path ({expected})"));
    }
    let (file, plan, runs) = (names[0], names[1], names[2]);
    if runs != "runs" {
        return Err(format!("{shown}: the record is not in a `runs` directory ({expected})"));
    }
    let plan = Digest::from_hex(plan).ok_or_else(|| {
        format!("{shown}: `{plan}` is not a plan id (64 lower-case hexadecimal digits)")
    })?;
    let fields: Vec<&str> = file.split('.').collect();
    let [phase, round, started, "run"] = fields[..] else {
        return Err(format!("{shown}: `{file}` is not `<phase>.<round>.<started>.run`"));
    };
    let phase = match phase {
        "map" => Phase::Map,
        "verify" => Phase::Verify,
        _ => return Err(format!("{shown}: `{phase}` is not a phase (map, verify)")),
    };
    let round = canonical_u64(round)
        .ok_or_else(|| format!("{shown}: round `{round}` is not a canonical decimal u64"))?;
    let started_unix_ns = canonical_u64(started)
        .ok_or_else(|| format!("{shown}: start time `{started}` is not a canonical decimal u64"))?;
    parts.truncate(parts.len() - 3);
    let store: PathBuf = parts.iter().collect();
    let store = if store.as_os_str().is_empty() { PathBuf::from(".") } else { store };
    Ok(RunPath { store, plan, phase, round, started_unix_ns })
}

/// A run record with its plan and the shards of its observers.
#[derive(Debug, Clone)]
pub struct Loaded {
    pub path: RunPath,
    pub record: RunRecord,
    pub plan: Plan,
    /// Every observer of the record (each is in the plan), with its shard
    /// in the store for the run's phase and round; `None` when there is
    /// none (a cancelled run).
    pub shards: BTreeMap<Digest, Option<Shard>>,
}

impl Loaded {
    /// The observers of the record whose shard is missing.
    pub fn missing_shards(&self) -> usize {
        self.shards.values().filter(|s| s.is_none()).count()
    }
}

/// Reads the record at `path`, then its plan and its observers' shards from
/// the store its path names, opened shared (`spec/exec.md` §2).
///
/// Errors, each naming the file: a path of another shape; a record that
/// cannot be read or does not validate, or whose plan, phase or start time
/// is not its path's; a store that cannot be opened; a plan that is missing,
/// does not validate or whose content id is not its name; a record
/// observer that is not in the plan; a shard that does not validate or
/// names another observer, phase or round. A missing shard is not an
/// error here ([`counters`] makes it one).
pub fn load(path: &Path) -> Result<Loaded, String> {
    let rp = parse_run_path(path)?;
    let shown = path.display();
    let bytes = std::fs::read(path).map_err(|e| format!("{shown}: cannot read: {e}"))?;
    let record = RunRecord::from_bytes(&bytes)
        .map_err(|e| format!("{shown}: not a valid run record: {e}"))?;
    let run = &record.run;
    if run.plan != rp.plan {
        return Err(format!(
            "{shown}: the record's plan {} is not the plan {} of its path",
            run.plan, rp.plan
        ));
    }
    if run.phase != rp.phase {
        return Err(format!(
            "{shown}: the record's phase {} is not the phase {} of its path",
            store::phase_name(run.phase),
            store::phase_name(rp.phase)
        ));
    }
    if run.started_unix_ns != rp.started_unix_ns {
        return Err(format!(
            "{shown}: the record's start time {} is not the start time {} of its path",
            run.started_unix_ns, rp.started_unix_ns
        ));
    }
    let st = Store::open(&rp.store, Access::Shared)
        .map_err(|e| format!("{}: cannot open the store: {e}", rp.store.display()))?;
    let plan_path = st.plan_path(&rp.plan);
    let plan_shown = plan_path.display();
    let plan_bytes = st
        .read_plan(&rp.plan)
        .map_err(|e| format!("{plan_shown}: {e}"))?
        .ok_or_else(|| format!("{plan_shown}: the run's plan is not in the store"))?;
    let plan = Plan::from_bytes(&plan_bytes)
        .map_err(|e| format!("{plan_shown}: not a valid plan: {e}"))?;
    let id = slip_model::formats::content_id(&plan_bytes)
        .map_err(|e| format!("{plan_shown}: not a valid plan: {e}"))?;
    if id != rp.plan {
        return Err(format!("{plan_shown}: the plan's content id is {id}, not its name"));
    }
    let in_plan: BTreeSet<Digest> = plan.observers.iter().map(|o| o.id).collect();
    let mut shards = BTreeMap::new();
    for job in &record.jobs {
        if shards.contains_key(&job.observer) {
            continue;
        }
        if !in_plan.contains(&job.observer) {
            return Err(format!(
                "{shown}: observer {} of the record is not in the plan",
                job.observer
            ));
        }
        let shard_path = st.shard_path(&rp.plan, rp.phase, rp.round, &job.observer);
        let ss = shard_path.display();
        let shard = match st
            .read_shard(&rp.plan, rp.phase, rp.round, &job.observer)
            .map_err(|e| format!("{ss}: {e}"))?
        {
            None => None,
            Some(b) => {
                let s =
                    Shard::from_bytes(&b).map_err(|e| format!("{ss}: not a valid shard: {e}"))?;
                if s.observer != job.observer || s.phase != rp.phase || s.round != rp.round {
                    return Err(format!(
                        "{ss}: the shard names observer {}, phase {}, round {}; the run's is \
                         observer {}, phase {}, round {}: the store is not the run's",
                        s.observer,
                        store::phase_name(s.phase),
                        s.round,
                        job.observer,
                        store::phase_name(rp.phase),
                        rp.round
                    ));
                }
                Some(s)
            }
        };
        shards.insert(job.observer, shard);
    }
    Ok(Loaded { path: rp, record, plan, shards })
}

// ---------------------------------------------------------------------------
// Names

/// The reasons of `spec/model.md` §7, in id order.
pub const REASONS: [Reason; 6] = [
    Reason::NotACompile,
    Reason::UnsupportedLanguage,
    Reason::UnsupportedFlag,
    Reason::MissingFile,
    Reason::MalformedEntry,
    Reason::UnsupportedCompiler,
];

/// The statuses of `spec/model.md` §8, in id order.
pub const STATUSES: [Status; 4] = [Status::Ok, Status::Diagnosed, Status::Rejected, Status::Failed];

/// The ends of `spec/model.md` §9, in id order.
pub const ENDS: [End; 5] =
    [End::Completed, End::Crashed, End::KilledMemory, End::KilledTime, End::Cancelled];

pub fn reason_name(r: Reason) -> &'static str {
    match r {
        Reason::NotACompile => "not_a_compile",
        Reason::UnsupportedLanguage => "unsupported_language",
        Reason::UnsupportedFlag => "unsupported_flag",
        Reason::MissingFile => "missing_file",
        Reason::MalformedEntry => "malformed_entry",
        Reason::UnsupportedCompiler => "unsupported_compiler",
    }
}

pub fn status_name(s: Status) -> &'static str {
    match s {
        Status::Ok => "ok",
        Status::Diagnosed => "diagnosed",
        Status::Rejected => "rejected",
        Status::Failed => "failed",
    }
}

pub fn end_name(e: End) -> &'static str {
    match e {
        End::Completed => "completed",
        End::Crashed => "crashed",
        End::KilledMemory => "killed_memory",
        End::KilledTime => "killed_time",
        End::Cancelled => "cancelled",
    }
}

// ---------------------------------------------------------------------------
// §7.1 What is computed

/// `a + b`, or an error naming what overflowed.
pub(crate) fn add(a: u64, b: u64, what: &str) -> Result<u64, String> {
    a.checked_add(b).ok_or_else(|| format!("{what} overflows a 64-bit unsigned integer"))
}

/// The record's observers by the status of their shard (the four §8
/// statuses, in order), over the shards present.
pub fn shard_statuses(loaded: &Loaded) -> [(Status, u64); 4] {
    STATUSES.map(|st| {
        let n = loaded.shards.values().flatten().filter(|s| s.outcome.status == st).count();
        (st, n as u64)
    })
}

/// Every shard counter name with its sum over the shards present.
pub fn shard_counters(loaded: &Loaded) -> Result<BTreeMap<String, u64>, String> {
    let mut sums: BTreeMap<String, u64> = BTreeMap::new();
    for s in loaded.shards.values().flatten() {
        for c in &s.counters {
            let sum = sums.entry(c.name.clone()).or_insert(0);
            *sum = add(*sum, c.value, &format!("counter `{}` summed over the shards", c.name))?;
        }
    }
    Ok(sums)
}

/// The counters of §7.1, by name. An observer of the record without a
/// shard is an error (the run was cancelled, or the store is not the
/// run's), as is a shard counter whose name is one of the counters this
/// function computes itself, or a sum that overflows.
pub fn counters(loaded: &Loaded) -> Result<BTreeMap<String, u64>, String> {
    let missing = loaded.missing_shards();
    if missing > 0 {
        let first = loaded.shards.iter().find(|(_, s)| s.is_none()).map(|(id, _)| *id);
        return Err(format!(
            "{}: {missing} observer(s) of the record have no shard in the store (the first is \
             {}): the run was cancelled, or the store is not the run's",
            loaded.path.store.display(),
            first.map_or_else(String::new, |d| d.to_string())
        ));
    }
    let mut out = BTreeMap::new();
    out.insert("plan.observers".to_owned(), loaded.plan.observers.len() as u64);
    for r in REASONS {
        let n = loaded.plan.excluded.iter().filter(|x| x.reason == r).count();
        out.insert(format!("plan.excluded.{}", reason_name(r)), n as u64);
    }
    out.insert("run.observers".to_owned(), loaded.shards.len() as u64);
    for (st, n) in shard_statuses(loaded) {
        out.insert(format!("shard.{}", status_name(st)), n);
    }
    for (name, sum) in shard_counters(loaded)? {
        if out.contains_key(&name) {
            return Err(format!(
                "{}: shard counter `{name}` has the name of a counter of spec/harness.md §7.1",
                loaded.path.store.display()
            ));
        }
        out.insert(name, sum);
    }
    Ok(out)
}

/// The measured values of §7.1. Times are kept in nanoseconds and rendered
/// as seconds by [`seconds`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Measured {
    /// The run's `wall_ns` (`wall_s`).
    pub wall_ns: u64,
    /// The sum of `cpu_ns` over every job (`cpu_s`).
    pub cpu_ns: u64,
    pub peak_rss_max: u64,
    pub peak_rss_p50: u64,
    pub peak_rss_p90: u64,
    pub peak_rss_p99: u64,
    pub peak_rss_sum: u64,
    pub cores: u64,
    pub workers_max: u64,
    pub memory_budget: u64,
}

/// The value at nearest rank `ceil(p × n / 100)` (1-based) of the
/// ascending list `sorted`; 0 for an empty list. The rank is clamped to
/// 1..=n, so `p` 0 gives the smallest value and `p` above 100 the largest.
pub fn percentile(sorted: &[u64], p: u64) -> u64 {
    let n = sorted.len() as u128;
    if n == 0 {
        return 0;
    }
    let rank = (u128::from(p) * n).div_ceil(100).clamp(1, n);
    sorted[(rank - 1) as usize]
}

/// The measured values of `run` (§7.1): the percentiles over the jobs that
/// ended `completed` with a non-zero `peak_rss`. `Err` if the summed
/// `cpu_ns` overflows.
pub fn measured(run: &RunRecord) -> Result<Measured, String> {
    let mut cpu_ns = 0u64;
    for j in &run.jobs {
        cpu_ns = add(cpu_ns, j.cpu_ns, "cpu_ns summed over the jobs")?;
    }
    let mut rss: Vec<u64> = run
        .jobs
        .iter()
        .filter(|j| j.end == End::Completed && j.peak_rss != 0)
        .map(|j| j.peak_rss)
        .collect();
    rss.sort_unstable();
    let r = &run.run;
    Ok(Measured {
        wall_ns: r.wall_ns,
        cpu_ns,
        peak_rss_max: rss.last().copied().unwrap_or(0),
        peak_rss_p50: percentile(&rss, 50),
        peak_rss_p90: percentile(&rss, 90),
        peak_rss_p99: percentile(&rss, 99),
        peak_rss_sum: r.peak_rss_sum,
        cores: r.cores,
        workers_max: r.workers_max,
        memory_budget: r.memory_budget,
    })
}

// ---------------------------------------------------------------------------
// The command

/// Why a command failed: a usage error (exit status 2) or any other error
/// (exit status 1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    Usage(String),
    Error(String),
}

/// `cargo xtask perf …` over the repository `repo`: [`run`] with stdout.
pub fn main(repo: &Path, args: &[String]) -> ExitCode {
    let mut stdout = std::io::stdout();
    match run(repo, args, &mut stdout) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(Failure::Usage(e)) => {
            eprintln!("xtask perf: {e}\n{USAGE}");
            ExitCode::from(2)
        }
        Err(Failure::Error(e)) => {
            eprintln!("xtask perf: {e}");
            ExitCode::from(1)
        }
    }
}

/// Runs `args` (after `perf`), writing the output to `out`. `Ok(false)` is
/// a `check` that found a difference (exit status 1). The corpus is looked
/// up before any record is read; every record of `summarize` is loaded
/// before anything is printed.
pub fn run(repo: &Path, args: &[String], out: &mut dyn Write) -> Result<bool, Failure> {
    let usage = |s: &str| Failure::Usage(s.to_owned());
    let err = Failure::Error;
    let write = |out: &mut dyn Write, text: &str| {
        out.write_all(text.as_bytes())
            .and_then(|()| out.flush())
            .map_err(|e| Failure::Error(format!("cannot write the output: {e}")))
    };
    match args.first().map(String::as_str) {
        Some("summarize") => {
            let records = &args[1..];
            if records.is_empty() {
                return Err(usage("`summarize` needs at least one run record"));
            }
            let mut texts = Vec::new();
            for given in records {
                let loaded = load(Path::new(given)).map_err(err)?;
                texts.push(render_summary(given, &loaded).map_err(err)?);
            }
            write(out, &texts.join("\n"))?;
            Ok(true)
        }
        Some(cmd @ ("record" | "check")) => {
            let [name, record] = &args[1..] else {
                return Err(usage(&format!("`{cmd}` needs <corpus> <run record>")));
            };
            let corpora = rows::load(repo).map_err(err)?;
            let corpus = rows::find(&corpora, name).map_err(err)?;
            let loaded = load(Path::new(record)).map_err(err)?;
            let rel = format!("{BASELINES}/{}.json", corpus.name);
            let path = repo.join(&rel);
            if cmd == "record" {
                let text = render_baseline(corpus, &loaded).map_err(err)?;
                write_atomic(&path, &text).map_err(err)?;
                write(out, &format!("{}\n", path.display()))?;
                Ok(true)
            } else {
                let text = std::fs::read(&path)
                    .map_err(|e| Failure::Error(format!("{rel}: cannot read: {e}")))?;
                let baseline =
                    read_baseline(&text).map_err(|e| Failure::Error(format!("{rel}: {e}")))?;
                if baseline.corpus != corpus.name {
                    return Err(err(format!(
                        "{rel}: the baseline is of corpus `{}`, not `{}`",
                        baseline.corpus, corpus.name
                    )));
                }
                let (report, ok) = check(&baseline, corpus, &loaded).map_err(err)?;
                write(out, &report)?;
                Ok(ok)
            }
        }
        Some(other) => Err(usage(&format!("unknown command `{other}`"))),
        None => Err(usage("missing command")),
    }
}

/// Writes `text` to `path` through a temporary file in the same directory
/// and one rename, creating the directory; a baseline that exists is
/// replaced.
fn write_atomic(path: &Path, text: &str) -> Result<(), String> {
    let shown = path.display();
    let dir = path.parent().ok_or_else(|| format!("{shown}: has no parent directory"))?;
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: cannot create: {e}", dir.display()))?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    let result = std::fs::write(&tmp, text)
        .and_then(|()| std::fs::rename(&tmp, path))
        .map_err(|e| format!("{shown}: cannot write: {e}"));
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}
