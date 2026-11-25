//! `plan`, `parse`, `dump` and `resource` (`spec/exec.md` §8.1–§8.3,
//! §8.10), each over its parsed options and the two output streams, and
//! the pieces `verify` (`crate::verify_cmd`), `declare` and `units` share
//! with them: the store, the plan, the selection, the pool's options, the
//! worker command and the summary lines.
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
use slip_model::{Digest, End, Observer, Phase, Plan, Reason, Status, formats};

use crate::args::{self, Db, Invocation, ParseArgs, PlanArgs, PlanSource, RunArgs};
use crate::status;

/// Why a command stopped early: the line to print and the status.
#[derive(Debug)]
pub(crate) enum Failure {
    /// Status 2; the usage text follows the line.
    Usage(String),
    /// Status 3; the line is `slip: <what>: <why>`.
    Input(String),
    /// `parse` was interrupted (§7): the signal path ends the process.
    Interrupted,
}

pub(crate) type Result<T> = std::result::Result<T, Failure>;

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
        Invocation::Resource { write } => write_resource(&write),
        Invocation::Verify(a) => return crate::verify_cmd::main(&a, out, err),
        Invocation::Declare(a) => return crate::declare::main(&a, out, err),
        Invocation::Units(a) => return crate::units::main(&a, out, err),
        Invocation::Map(a) => crate::transform::map::main(&a, out, err),
        Invocation::Rename(a) => crate::transform::rename::main(&a, out, err),
        Invocation::Reduce(a) => crate::transform::reduce::main(&a, out, err),
        Invocation::Check(a) => crate::transform::check::main(&a, out, err),
        Invocation::Apply(a) => crate::transform::apply::main(&a, out, err),
        Invocation::Panic => panic!("slip __panic: a deliberate panic"),
        Invocation::Worker(_) => unreachable!("the worker is run by `run`, not dispatched"),
    };
    finish(result, out, err)
}

/// A command's result as its exit status (§8.5): the line of a failure on
/// standard error (with the usage text for status 2).
pub(crate) fn finish(result: Result<u8>, out: &mut dyn Write, err: &mut dyn Write) -> ExitCode {
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

// ----- shared pieces -----------------------------------------------------------

pub(crate) fn tool() -> Tool {
    Tool { slip: env!("CARGO_PKG_VERSION").to_owned(), kiln: slip_kiln::kiln_version().to_owned() }
}

pub(crate) fn open_store(dir: &Path, access: Access) -> Result<Store> {
    Store::open(dir, access).map_err(|e| store_failure(dir, &e))
}

/// `slip: <what>: <why>` for a store error: the errors that name a path
/// already start with it.
pub(crate) fn store_failure(dir: &Path, e: &StoreError) -> Failure {
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
pub(crate) fn build_and_publish(
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
    for reason in Reason::ALL {
        let n = plan.excluded.iter().filter(|x| x.reason == reason).count();
        let _ = writeln!(out, "excluded {} {n}", reason.name());
        if n > 0 && reason != Reason::NotACompile {
            code = status::FINDINGS;
        }
    }
    Ok((plan, id, code))
}

/// The jobserver pool, bounded by `-j` (default: the cores available).
pub(crate) fn token_pool(jobs: Option<usize>) -> Pool {
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
    let store = open_store(&a.run.store, Access::Exclusive)?;
    // From here an interruption kills the workers, returns the tokens,
    // publishes the run record, removes the workers' `tmp/` entries and
    // removes the lock (§7).
    signals::install(Box::new(pool::cleanup_on_signal(&store)));

    let (plan, id, plan_code) = match &a.source {
        PlanSource::Db(dbs) => build_and_publish(dbs, a.run.jobs, &store, out)?,
        PlanSource::Plan(x) => {
            let (plan, id) = load_plan(&store, x)?;
            let _ = writeln!(out, "plan {id}");
            (plan, id, status::OK)
        }
    };

    let selected = select_for_phase(&plan, &a.run, err);
    let tokens = token_pool(a.run.jobs);
    let options = pool_options(&a.run, tokens.jobs(), err);
    let worker = worker_command(&a.run)?;

    let run =
        pool::run(&store, &id, &selected, Phase::Map, 0, &worker, &options, &tokens, &ProcSampler);
    let summary = match run {
        Ok(summary) => summary,
        Err(PoolError::Cancelled) => return Err(Failure::Interrupted),
        Err(PoolError::Store(e)) => return Err(store_failure(store.root(), &e)),
        Err(PoolError::Io(e)) => return Err(Failure::Input(format!("parse: {e}"))),
        Err(PoolError::Refused(what)) => {
            unreachable!("parse runs the map phase with neither field of protocol 2: {what}")
        }
    };

    print_summary(&summary, &selected, out);

    let all_ok = report_not_ok(&summary, &selected, err);
    let parse_code = if all_ok { status::OK } else { status::FINDINGS };
    Ok(plan_code.max(parse_code))
}

/// The selection of a phase (§8.2): [`select`], with the warning when a
/// non-empty plan is left with no observer.
pub(crate) fn select_for_phase(plan: &Plan, a: &RunArgs, err: &mut dyn Write) -> Vec<Observer> {
    let selected = select(&plan.observers, &a.only, a.limit);
    if selected.is_empty() && !plan.observers.is_empty() {
        let _ = writeln!(
            err,
            "slip: warning: the selection keeps none of the plan's {} observer(s); the phase \
             runs over nothing",
            plan.observers.len()
        );
    }
    selected
}

/// The pool's parameters from the options (§6, §8.2).
pub(crate) fn pool_options(a: &RunArgs, jobs: usize, err: &mut dyn Write) -> Options {
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
    for s in Status::ALL {
        let _ = writeln!(out, "shard {} {}", s.name(), summary.statuses[s as usize]);
    }
    for e in End::ALL {
        let _ = writeln!(out, "job {} {}", e.name(), summary.ends[e as usize]);
    }
    let (wall, peak) = wall_and_peak(summary, selected);
    let _ = writeln!(out, "wall {wall}");
    if let Some((bytes, main)) = peak {
        let _ = write!(out, "peak_rss {bytes} ");
        let _ = out.write_all(main);
        let _ = writeln!(out);
    }
    let _ = out.write_all(b"run ");
    let _ = out.write_all(&args::os_bytes(summary.run_path.as_os_str()));
    let _ = writeln!(out);
    let _ = out.flush();
}

/// `wall` (the run record's `wall_ns` in seconds, truncated to
/// milliseconds) and `peak_rss` (the largest `peak_rss` of the run
/// record's jobs, the first such job on a tie, and its observer's main
/// file; `None` when every job's is 0 or there is no job) of §8.2.
pub(crate) fn wall_and_peak<'a>(
    summary: &Summary,
    selected: &'a [Observer],
) -> (String, Option<(u64, &'a [u8])>) {
    let ms = summary.run.run.wall_ns / 1_000_000;
    let wall = format!("{}.{:03}", ms / 1000, ms % 1000);
    let mains: HashMap<Digest, &[u8]> =
        selected.iter().map(|o| (o.id, o.main.path.as_slice())).collect();
    let mut peak: Option<(u64, Digest)> = None;
    for job in &summary.run.jobs {
        if job.peak_rss > 0 && peak.is_none_or(|(p, _)| job.peak_rss > p) {
            peak = Some((job.peak_rss, job.observer));
        }
    }
    let peak = peak.map(|(bytes, o)| (bytes, mains.get(&o).copied().unwrap_or_default()));
    (wall, peak)
}

/// One line on standard error per observer whose shard is not `ok`, in
/// plan order (§8.2), from the pool's outcomes. Returns whether every one
/// is `ok`.
pub(crate) fn report_not_ok(summary: &Summary, selected: &[Observer], err: &mut dyn Write) -> bool {
    debug_assert_eq!(summary.outcomes.len(), selected.len(), "one outcome per observer run");
    let mut all_ok = summary.outcomes.len() == selected.len();
    for (o, outcome) in selected.iter().zip(&summary.outcomes) {
        debug_assert_eq!(o.id, outcome.observer, "outcomes in the order given");
        if outcome.status != Status::Ok {
            all_ok = false;
            let _ = write!(err, "slip: {}: ", outcome.status.name());
            let _ = err.write_all(&o.main.path);
            let _ = writeln!(err, ": {}", outcome.message);
        }
    }
    all_ok
}

/// `--plan <x>` (§8.2): an id prefix of 4 to 64 hexadecimal digits, which
/// must match exactly one plan of the store, or else a plan file, which is
/// read, validated and published.
pub(crate) fn load_plan(store: &Store, x: &OsString) -> Result<(Plan, Digest)> {
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
        // A plan stored under another plan's id (`spec/exec.md` §2.1
        // "Which reads are checked").
        if formats::content_id(&bytes).ok() != Some(id) {
            return Err(Failure::Input(format!(
                "{}: the file's content id is not its name",
                path.display()
            )));
        }
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
pub(crate) fn select(
    observers: &[Observer],
    only: &[Vec<u8>],
    limit: Option<u64>,
) -> Vec<Observer> {
    let limit = limit.map_or(usize::MAX, |n| usize::try_from(n).unwrap_or(usize::MAX));
    observers
        .iter()
        .filter(|o| only.is_empty() || only.iter().any(|s| contains(&o.main.path, s)))
        .take(limit)
        .cloned()
        .collect()
}

pub(crate) fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    needle.is_empty() || haystack.windows(needle.len()).any(|w| w == needle)
}

/// The run's files from `--owned` and `--readonly` (`spec/edits.md` §2.1,
/// §10.1 step 2: "made real paths"; `spec/verify.md` §2.1: ownership is
/// of real paths, which are the subject paths). Each entry is made a real
/// path (`slip_spelling::real_path`: a symbolic link is resolved, the
/// result is canonical and spelled as its directories list it, §2.1 "One
/// spelling"); an entry that does not exist, or one that is
/// both owned and read-only once resolved, is an input error. Sorted, no
/// two equal. `None` when neither option is given; the caller has
/// refused `--readonly` without `--owned` (a usage error).
pub(crate) fn ownership(
    owned: &[PathBuf],
    readonly: &[PathBuf],
) -> Result<Option<slip_model::transform::Ownership>> {
    use std::os::unix::ffi::OsStrExt;
    if owned.is_empty() {
        return Ok(None);
    }
    let real = |list: &[PathBuf]| -> Result<Vec<Vec<u8>>> {
        let mut v = list
            .iter()
            .map(|p| {
                slip_spelling::real_path(p)
                    .map(|r| r.as_os_str().as_bytes().to_vec())
                    .map_err(|e| Failure::Input(format!("{}: {}", p.display(), e.detail())))
            })
            .collect::<Result<Vec<_>>>()?;
        v.sort();
        v.dedup();
        Ok(v)
    };
    let (owned, readonly) = (real(owned)?, real(readonly)?);
    if let Some(both) = readonly.iter().find(|r| owned.binary_search(r).is_ok()) {
        return Err(Failure::Input(format!(
            "{}: given as --owned and as --readonly",
            String::from_utf8_lossy(both)
        )));
    }
    Ok(Some(slip_model::transform::Ownership { owned, readonly, rebuilt: Vec::new() }))
}

/// The check of a record's ownership entries where they are read for a
/// run (`spec/edits.md` §2.1 "One spelling": a `TRFM`'s `ownership`, a
/// `DECL`'s `files`; `docs/wp/WP-151.md` item 1): every entry, owned and
/// read-only, must exist and be its own real path, the spelling
/// [`ownership`] gives a command line's (`slip_spelling::real_path`: a
/// symbolic link resolved, each component spelled as its directory lists
/// it). An entry spelled otherwise covers none of the paths it means,
/// which are real paths, so a read-only one would protect nothing. The
/// one function every reader calls; `Err` is the line of an input error
/// (status 3) or of a failed job, naming the entry and, when it has one,
/// its spelling on disk. An entry that finds nothing is refused, never
/// passed: on a volume that answers lookups inconsistently, "missing" is
/// the answer that would let it carve nothing.
pub fn entries_on_disk(o: &slip_model::transform::Ownership) -> std::result::Result<(), String> {
    use std::os::unix::ffi::OsStrExt;
    for (list, name) in [(&o.owned, "owned"), (&o.readonly, "readonly")] {
        for e in list {
            let entry = String::from_utf8_lossy(e);
            match slip_spelling::real_path(Path::new(std::ffi::OsStr::from_bytes(e))) {
                Ok(real) if real.as_os_str().as_bytes() == e.as_slice() => {}
                Ok(real) => {
                    return Err(format!(
                        "ownership: the {name} entry {entry} is not spelled as on disk ({})",
                        real.display()
                    ));
                }
                Err(err) if err.is_not_found() => {
                    return Err(format!("ownership: the {name} entry {entry} does not exist"));
                }
                Err(err) => {
                    return Err(format!(
                        "ownership: the {name} entry {entry} has no spelling on disk: {}",
                        err.detail()
                    ));
                }
            }
        }
    }
    Ok(())
}

/// The worker command (§8.2): this executable, `__worker` with the same
/// `--worker-jobs` the pool is given and `--worker-stack`, without the
/// coordinator's `KILN_*` variables. `verify` appends its own two options
/// (§8.7).
pub(crate) fn worker_command(a: &RunArgs) -> Result<WorkerCommand> {
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

/// `slip resource --write <dir>` (§8.10): nothing on standard output.
fn write_resource(dir: &Path) -> Result<u8> {
    slip_script::resource::write_to(dir).map_err(|e| Failure::Input(e.to_string()))?;
    Ok(status::OK)
}
