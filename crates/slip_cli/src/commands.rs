//! `plan`, `parse` and `dump` (`spec/exec.md` §8.1–§8.3), each over its
//! parsed options and the two output streams.
//!
//! Every write to standard output or standard error ignores its error
//! (`let _ = …`): a closed pipe must not become a panic and status 70.

use std::collections::HashMap;
use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use slip_cdb::plan::{self as cdb_plan, PlanError, Source, Tool};
use slip_exec::admission::{self, Config};
use slip_exec::jobserver::Pool;
use slip_exec::pool::{self, Options, PoolError, Summary, WorkerCommand};
use slip_exec::process::ProcSampler;
use slip_exec::signals;
use slip_exec::store::{Access, Store, StoreError};
use slip_kiln::KilnCheck;
use slip_model::{Digest, End, Observer, Phase, Plan, Reason, Shard, Status, formats};

use crate::args::{self, Db, Invocation, ParseArgs, PlanArgs, PlanSource};
use crate::status;

/// Why a command stopped early: the line to print and the status.
#[derive(Debug)]
enum Failure {
    /// Status 2; the usage text follows the line.
    Usage(String),
    /// Status 3; the line is `slip: <what>: <why>`.
    Input(String),
    /// `parse` was interrupted (§7): the signal path ends the process.
    Interrupted,
}

type Result<T> = std::result::Result<T, Failure>;

/// Runs `invocation` (anything but the worker).
pub fn dispatch(invocation: Invocation, out: &mut dyn Write, err: &mut dyn Write) -> ExitCode {
    let result = match invocation {
        Invocation::Version => {
            let _ = writeln!(out, "{}", crate::version_line());
            Ok(status::OK)
        }
        Invocation::Help => {
            let _ = write!(out, "{}", args::USAGE);
            Ok(status::OK)
        }
        Invocation::Plan(a) => plan(&a, out, err),
        Invocation::Parse(a) => parse(&a, out, err),
        Invocation::Dump { file } => dump(&file, out),
        Invocation::Panic => panic!("slip __panic: a deliberate panic"),
        Invocation::Worker(_) => unreachable!("the worker is run by `run`, not dispatched"),
    };
    let _ = out.flush();
    match result {
        Ok(code) => ExitCode::from(code),
        Err(Failure::Usage(line)) => {
            let _ = writeln!(err, "slip: {line}");
            let _ = write!(err, "{}", args::USAGE);
            ExitCode::from(status::USAGE)
        }
        Err(Failure::Input(line)) => {
            let _ = writeln!(err, "slip: {line}");
            ExitCode::from(status::INPUT)
        }
        // `main` returns through `signals::finish`, which waits for the
        // signal thread to end the process by the signal.
        Err(Failure::Interrupted) => ExitCode::FAILURE,
    }
}

// ----- names of the model's enumerations (spec/model.md §7–§9) ----------------

const REASONS: [(Reason, &str); 6] = [
    (Reason::NotACompile, "not_a_compile"),
    (Reason::UnsupportedLanguage, "unsupported_language"),
    (Reason::UnsupportedFlag, "unsupported_flag"),
    (Reason::MissingFile, "missing_file"),
    (Reason::MalformedEntry, "malformed_entry"),
    (Reason::UnsupportedCompiler, "unsupported_compiler"),
];

const STATUSES: [(Status, &str); 4] = [
    (Status::Ok, "ok"),
    (Status::Diagnosed, "diagnosed"),
    (Status::Rejected, "rejected"),
    (Status::Failed, "failed"),
];

const ENDS: [(End, &str); 5] = [
    (End::Completed, "completed"),
    (End::Crashed, "crashed"),
    (End::KilledMemory, "killed_memory"),
    (End::KilledTime, "killed_time"),
    (End::Cancelled, "cancelled"),
];

fn status_name(s: Status) -> &'static str {
    STATUSES.iter().find(|(v, _)| *v == s).map_or("?", |(_, n)| n)
}

// ----- shared pieces -----------------------------------------------------------

fn tool() -> Tool {
    Tool { slip: env!("CARGO_PKG_VERSION").to_owned(), kiln: slip_kiln::kiln_version().to_owned() }
}

fn open_store(dir: &Path, access: Access) -> Result<Store> {
    Store::open(dir, access).map_err(|e| store_failure(dir, &e))
}

/// `slip: <what>: <why>` for a store error: the errors that name a path
/// already start with it.
fn store_failure(dir: &Path, e: &StoreError) -> Failure {
    match e {
        StoreError::Unrecognised(_) | StoreError::Io { .. } => Failure::Input(e.to_string()),
        _ => Failure::Input(format!("{}: {e}", dir.display())),
    }
}

fn sources(dbs: &[Db]) -> Vec<Source> {
    dbs.iter()
        .map(|d| Source {
            name: d.name.clone().unwrap_or_else(|| "default".to_owned()),
            path: d.path.clone(),
        })
        .collect()
}

/// Builds the plan (`spec/cdb.md` §7) with Kiln's front-end check, hashing
/// on the threads whose CPU tokens can be had without waiting (§8.1), and
/// publishes it. Prints the three kinds of line of §8.1; returns the plan,
/// its id and the status of `plan`.
fn build_and_publish(
    dbs: &[Db],
    jobs: Option<usize>,
    store: &Store,
    out: &mut dyn Write,
) -> Result<(Plan, Digest, u8)> {
    let tokens = token_pool(jobs);
    let mut held = Vec::new();
    while held.len() < tokens.jobs() {
        match tokens.try_acquire() {
            Some(t) => held.push(t),
            None => break,
        }
    }
    let threads = held.len().max(1);
    let built = cdb_plan::build(&sources(dbs), &KilnCheck, &tool(), threads);
    drop(held);
    let plan = built.map_err(|e| match e {
        PlanError::Usage(detail) => Failure::Usage(detail),
        e @ PlanError::Database { .. } => Failure::Input(e.to_string()),
    })?;
    let id = store.publish_plan(&plan.to_bytes()).map_err(|e| store_failure(store.root(), &e))?;
    let _ = writeln!(out, "plan {id}");
    let _ = writeln!(out, "observers {}", plan.observers.len());
    let mut code = status::OK;
    for (reason, name) in REASONS {
        let n = plan.excluded.iter().filter(|x| x.reason == reason).count();
        let _ = writeln!(out, "excluded {name} {n}");
        if n > 0 && reason != Reason::NotACompile {
            code = status::FINDINGS;
        }
    }
    Ok((plan, id, code))
}

/// The jobserver pool, bounded by `-j` (default: the cores available).
fn token_pool(jobs: Option<usize>) -> Pool {
    let pool = Pool::from_env();
    match jobs {
        Some(j) => pool.with_jobs(j),
        None => pool,
    }
}

// ----- plan (§8.1) ------------------------------------------------------------

fn plan(a: &PlanArgs, out: &mut dyn Write, _err: &mut dyn Write) -> Result<u8> {
    let store = open_store(&a.store, Access::Shared)?;
    let (_, _, code) = build_and_publish(&a.dbs, a.jobs, &store, out)?;
    Ok(code)
}

// ----- parse (§8.2) -----------------------------------------------------------

fn parse(a: &ParseArgs, out: &mut dyn Write, err: &mut dyn Write) -> Result<u8> {
    let store = open_store(&a.store, Access::Exclusive)?;
    // From here an interruption kills the workers, returns the tokens,
    // publishes the run record and removes the lock (§7). The killed
    // workers' scratch directories go too, while the lock is still held.
    let cleanup = pool::cleanup_on_signal(&store);
    let root = store.root().to_owned();
    signals::install(Box::new(move || {
        remove_dead_entries(&root);
        cleanup();
    }));

    let (plan, id, plan_code) = match &a.source {
        PlanSource::Db(dbs) => build_and_publish(dbs, a.jobs, &store, out)?,
        PlanSource::Plan(x) => {
            let (plan, id) = load_plan(&store, x)?;
            let _ = writeln!(out, "plan {id}");
            (plan, id, status::OK)
        }
    };

    let selected = select(&plan.observers, &a.only, a.limit);
    if selected.is_empty() && !plan.observers.is_empty() {
        let _ = writeln!(
            err,
            "slip: warning: the selection keeps none of the plan's {} observer(s); the phase \
             runs over nothing",
            plan.observers.len()
        );
    }

    let tokens = token_pool(a.jobs);
    let options = pool_options(a, tokens.jobs(), err);
    let worker = worker_command(a)?;

    let run =
        pool::run(&store, &id, &selected, Phase::Map, 0, &worker, &options, &tokens, &ProcSampler);
    // A worker killed for a cap, a time limit or a crash left its scratch
    // directory behind; the lock is still ours.
    remove_dead_entries(store.root());
    let summary = match run {
        Ok(summary) => summary,
        Err(PoolError::Cancelled) => return Err(Failure::Interrupted),
        Err(PoolError::Store(e)) => return Err(store_failure(store.root(), &e)),
        Err(PoolError::Io(e)) => return Err(Failure::Input(format!("parse: {e}"))),
    };

    print_summary(&summary, &selected, out);

    let reported_ok = report_not_ok(&store, &id, &selected, err)?;
    let all_ok = reported_ok && summary.statuses[Status::Ok as usize] == selected.len() as u64;
    let parse_code = if all_ok { status::OK } else { status::FINDINGS };
    Ok(plan_code.max(parse_code))
}

/// Removes the entries of `<root>/tmp/` named `<pid>.…` for a process that
/// no longer exists, directories included, as an exclusive open does
/// (§2 "Cleaning"): what the workers this process killed left behind.
/// Only where `/proc` tells; errors are ignored.
fn remove_dead_entries(root: &Path) {
    if !Path::new("/proc/self").exists() {
        return;
    }
    let tmp = root.join("tmp");
    let Ok(entries) = std::fs::read_dir(&tmp) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(pid) = name
            .to_str()
            .and_then(|n| n.split_once('.'))
            .map(|(pid, _)| pid)
            .filter(|p| {
                !p.is_empty() && !p.starts_with('0') && p.bytes().all(|b| b.is_ascii_digit())
            })
            .and_then(|p| p.parse::<u32>().ok())
        else {
            continue;
        };
        if pid == std::process::id() || Path::new(&format!("/proc/{pid}")).exists() {
            continue;
        }
        let path = entry.path();
        let _ = if path.is_dir() {
            std::fs::remove_dir_all(&path)
        } else {
            std::fs::remove_file(&path)
        };
    }
}

/// The pool's parameters from the options (§6, §8.2).
fn pool_options(a: &ParseArgs, jobs: usize, err: &mut dyn Write) -> Options {
    let budget = if let Some(b) = a.memory {
        b
    } else {
        let (cgroup, total) = admission::read_system();
        let b = admission::budget_from_system(cgroup, total);
        if b == 0 {
            let _ = writeln!(
                err,
                "slip: warning: the memory available is unknown; memory admission and caps \
                 are off (as with --memory 0)"
            );
        }
        b
    };
    let mut config = Config::with_budget(budget);
    config.observer_memory = a.observer_memory;
    config.worker_stack = a.worker_stack;
    if let Some(base) = a.estimate_base {
        config.estimate_base = base;
    }
    if let Some(per_byte) = a.estimate_per_byte {
        config.estimate_per_byte = per_byte;
    }
    let mut options = Options::new(jobs, config);
    let Tool { slip, kiln } = tool();
    options.tool_slip = slip;
    options.tool_kiln = kiln;
    options.worker_jobs = a.worker_jobs;
    if let Some(seconds) = a.observer_timeout {
        options.observer_timeout = (seconds > 0).then(|| Duration::from_secs(seconds));
    }
    options
}

/// The lines of §8.2 after the phase, but `plan`.
fn print_summary(summary: &Summary, selected: &[Observer], out: &mut dyn Write) {
    let _ = writeln!(out, "selected {}", selected.len());
    for (s, name) in STATUSES {
        let _ = writeln!(out, "shard {name} {}", summary.statuses[s as usize]);
    }
    for (e, name) in ENDS {
        let _ = writeln!(out, "job {name} {}", summary.ends[e as usize]);
    }
    let ms = summary.run.run.wall_ns / 1_000_000;
    let _ = writeln!(out, "wall {}.{:03}", ms / 1000, ms % 1000);
    let mains: HashMap<Digest, &[u8]> =
        selected.iter().map(|o| (o.id, o.main.path.as_slice())).collect();
    let mut peak: Option<(u64, Digest)> = None;
    for job in &summary.run.jobs {
        if job.peak_rss > 0 && peak.is_none_or(|(p, _)| job.peak_rss > p) {
            peak = Some((job.peak_rss, job.observer));
        }
    }
    if let Some((bytes, observer)) = peak {
        let _ = write!(out, "peak_rss {bytes} ");
        let _ = out.write_all(mains.get(&observer).copied().unwrap_or_default());
        let _ = writeln!(out);
    }
    let _ = out.write_all(b"run ");
    let _ = out.write_all(&args::os_bytes(summary.run_path.as_os_str()));
    let _ = writeln!(out);
    let _ = out.flush();
}

/// One line on standard error per observer whose shard is not `ok`, in
/// plan order (§8.2). Returns whether every one is `ok`.
fn report_not_ok(
    store: &Store,
    id: &Digest,
    selected: &[Observer],
    err: &mut dyn Write,
) -> Result<bool> {
    let mut all_ok = true;
    for o in selected {
        let shard = store
            .read_shard(id, Phase::Map, 0, &o.id)
            .map_err(|e| store_failure(store.root(), &e))?;
        let Some(bytes) = shard else {
            all_ok = false;
            let _ = err.write_all(b"slip: failed: ");
            let _ = err.write_all(&o.main.path);
            let _ = writeln!(err, ": no shard in the store");
            continue;
        };
        let shard = Shard::from_bytes(&bytes).map_err(|e| {
            Failure::Input(format!("{}: {e}", store.shard_path(id, Phase::Map, 0, &o.id).display()))
        })?;
        if shard.outcome.status != Status::Ok {
            all_ok = false;
            let first = shard.outcome.message.lines().next().unwrap_or_default();
            let _ = write!(err, "slip: {}: ", status_name(shard.outcome.status));
            let _ = err.write_all(&o.main.path);
            let _ = writeln!(err, ": {first}");
        }
    }
    Ok(all_ok)
}

/// `--plan <x>` (§8.2): an id prefix of 4 to 64 hexadecimal digits, which
/// must match exactly one plan of the store, or else a plan file, which is
/// read, validated and published.
fn load_plan(store: &Store, x: &OsString) -> Result<(Plan, Digest)> {
    if let Some(prefix) = x
        .to_str()
        .filter(|t| (4..=64).contains(&t.len()) && t.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        let prefix = prefix.to_ascii_lowercase();
        let ids = store.find_plan(&prefix).map_err(|e| store_failure(store.root(), &e))?;
        let id = match ids.as_slice() {
            [id] => *id,
            [] => {
                return Err(Failure::Input(format!(
                    "plan {prefix}: no plan in the store {} has an id starting with it",
                    store.root().display()
                )));
            }
            several => {
                let names: Vec<String> = several.iter().map(ToString::to_string).collect();
                return Err(Failure::Input(format!(
                    "plan {prefix}: {} plans have an id starting with it: {}",
                    several.len(),
                    names.join(", ")
                )));
            }
        };
        let path = store.plan_path(&id);
        let bytes = store
            .read_plan(&id)
            .map_err(|e| store_failure(store.root(), &e))?
            .ok_or_else(|| Failure::Input(format!("{}: the plan has gone", path.display())))?;
        let plan = Plan::from_bytes(&bytes)
            .map_err(|e| Failure::Input(format!("{}: {e}", path.display())))?;
        return Ok((plan, id));
    }
    let path = PathBuf::from(x);
    let bytes =
        std::fs::read(&path).map_err(|e| Failure::Input(format!("{}: {e}", path.display())))?;
    let plan =
        Plan::from_bytes(&bytes).map_err(|e| Failure::Input(format!("{}: {e}", path.display())))?;
    let id = store.publish_plan(&bytes).map_err(|e| store_failure(store.root(), &e))?;
    Ok((plan, id))
}

/// `--only` (any substring of the main file's path), then `--limit` (§8.2).
fn select(observers: &[Observer], only: &[Vec<u8>], limit: Option<u64>) -> Vec<Observer> {
    let limit = limit.map_or(usize::MAX, |n| usize::try_from(n).unwrap_or(usize::MAX));
    observers
        .iter()
        .filter(|o| only.is_empty() || only.iter().any(|s| contains(&o.main.path, s)))
        .take(limit)
        .cloned()
        .collect()
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    needle.is_empty() || haystack.windows(needle.len()).any(|w| w == needle)
}

/// The worker command (§8.2): this executable, `__worker` with the same
/// `--worker-jobs` the pool is given and `--worker-stack`, without the
/// coordinator's `KILN_*` variables.
fn worker_command(a: &ParseArgs) -> Result<WorkerCommand> {
    let program = std::env::current_exe()
        .map_err(|e| Failure::Input(format!("cannot find the slip executable: {e}")))?;
    let env_remove = std::env::vars_os()
        .map(|(name, _)| name)
        .filter(|name| args::os_bytes(name).starts_with(b"KILN_"))
        .collect();
    Ok(WorkerCommand {
        program,
        args: vec![
            "__worker".into(),
            "--worker-jobs".into(),
            a.worker_jobs.to_string().into(),
            "--worker-stack".into(),
            a.worker_stack.to_string().into(),
        ],
        env_remove,
    })
}

// ----- dump (§8.3) ------------------------------------------------------------

fn dump(file: &Path, out: &mut dyn Write) -> Result<u8> {
    let bytes =
        std::fs::read(file).map_err(|e| Failure::Input(format!("{}: {e}", file.display())))?;
    let text =
        formats::dump(&bytes).map_err(|e| Failure::Input(format!("{}: {e}", file.display())))?;
    let _ = out.write_all(text.as_bytes());
    if !text.ends_with('\n') {
        let _ = writeln!(out);
    }
    Ok(status::OK)
}
