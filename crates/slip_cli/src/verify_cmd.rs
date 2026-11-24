//! `slip verify` (`spec/exec.md` §8.7; `spec/verify.md` §7, §7.1): the
//! verify phase of one round over the observers of a plan, and the run's
//! report. Work package WP-053.
//!
//! The verdict, the states and the exit status are the report's
//! (`slip_verify::run::Report`, trusted base): this module decides which
//! shard is this run's (§7 "This run's shard") and prints what the report
//! says, nothing more.

use std::collections::HashMap;
use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use slip_exec::pool::{self, JobExtra, PoolError};
use slip_exec::process::ProcSampler;
use slip_exec::signals;
use slip_exec::store::{Access, Store};
use slip_exec::worker::Post;
use slip_model::declared::Declared;
use slip_model::result::RunVerdict;
use slip_model::{Digest, Observer, Phase, RunRecord, Shard, Status, json};
use slip_verify::run::{Header, MapShard, Report};

use crate::args::{self, VerifyArgs};
use crate::commands::{self, Failure, Result};
use crate::image::Image;
use crate::status;

/// `slip verify` (§8.7).
pub fn main(args: &VerifyArgs, out: &mut dyn Write, err: &mut dyn Write) -> ExitCode {
    let result = verify(args, out, err);
    commands::finish(result, out, err)
}

/// `slip: verify: <message>` (§8.7's table).
fn input(message: impl std::fmt::Display) -> Failure {
    Failure::Input(format!("verify: {message}"))
}

fn verify(a: &VerifyArgs, out: &mut dyn Write, err: &mut dyn Write) -> Result<u8> {
    // The store, exclusively, and at once the signal handling (§8.2).
    let store = commands::open_store(&a.run.store, Access::Exclusive)?;
    signals::install(Box::new(pool::cleanup_on_signal(&store)));

    let (plan, plan_id) = commands::load_plan(&store, &a.plan)?;

    let (declared_id, declared) = publish_declared(&store, a.declared.as_deref())?;

    let image = Image::from_args(&a.image).map_err(input)?;

    if store
        .latest_run(&plan_id, Phase::Map)
        .map_err(|e| commands::store_failure(store.root(), &e))?
        .is_none()
    {
        return Err(input(format!(
            "plan {plan_id} has no map run in the store {}; run slip parse first",
            store.root().display()
        )));
    }
    if store
        .is_transform_round(&plan_id, a.round)
        .map_err(|e| commands::store_failure(store.root(), &e))?
    {
        return Err(input(format!(
            "round {} of plan {plan_id} is the round of a transform's run; slip verify does not \
             replace its shards",
            a.round
        )));
    }

    let selected = commands::select_for_phase(&plan, &a.run, err);
    let tokens = commands::token_pool(a.run.jobs);
    let options = commands::pool_options(&a.run, tokens.jobs(), err);
    let mut worker = commands::worker_command(&a.run)?;
    worker.args.extend(worker_options(a.findings, a.locate));
    let extra = JobExtra {
        transform: None,
        post: Some(Post { pre: 0, declared: declared_id, image: image.to_post() }),
    };

    let run = pool::run_with(
        &store,
        &plan_id,
        &selected,
        Phase::Verify,
        a.round,
        &extra,
        &worker,
        &options,
        &tokens,
        &ProcSampler,
    );
    let summary = match run {
        Ok(summary) => summary,
        Err(PoolError::Cancelled) => return Err(Failure::Interrupted),
        Err(PoolError::Store(e)) => return Err(commands::store_failure(store.root(), &e)),
        Err(PoolError::Io(e)) => return Err(Failure::Input(format!("verify: {e}"))),
        Err(PoolError::Refused(what)) => {
            unreachable!("verify gives every job a post and no transform: {what}")
        }
    };

    // The report (§7.1), observer by observer in plan order.
    let header = Header {
        plan: plan_id,
        round: a.round,
        declared: declared_id,
        rules: declared.rules_id(),
        image: image.maps().iter().map(|m| (m.subject.clone(), m.post.clone())).collect(),
        limit: a.findings,
        locate: a.locate,
        plan_observers: plan.observers.len() as u64,
        run: args::os_bytes(summary.run_path.as_os_str()).into_owned(),
    };
    let mut report = Report::new(header, &declared);
    if a.strict_names {
        // `--strict-names` (`spec/verify.md` §7.1): before the first `add`.
        report.strict_names();
    }
    let positions: HashMap<Digest, u64> =
        plan.observers.iter().enumerate().map(|(i, o)| (o.id, i as u64)).collect();
    let last = last_shards(&summary.run);
    for o in &selected {
        let verify = this_runs_shard(&store, &plan_id, a.round, o, &last)?;
        let map = match verify.as_ref().map(|s| s.outcome.status) {
            // §7: the states that depend on the map shard. (§8.7 names the
            // map shard for `diagnosed` and `rejected`; `verified` and
            // `mismatch` need it as well, `state` taking either as
            // `failed` without an `ok` one.)
            Some(Status::Ok | Status::Diagnosed | Status::Rejected) => {
                map_status(&store, &plan_id, &o.id)?
            }
            Some(Status::Failed) | None => None,
        };
        let position = positions[&o.id];
        report.add_at(position, &o.id, &o.main.path, verify.as_ref(), map);
    }

    if let Some(path) = &a.report {
        write_report(path, &report.to_json())?;
    }

    print(&report, &summary, &selected, a.report.as_deref(), out, err);
    Ok(match report.verdict() {
        RunVerdict::Verified => status::OK,
        RunVerdict::NotVerified => status::FINDINGS,
        RunVerdict::Incomplete => status::INCOMPLETE,
    })
}

/// The declared changes of `--declared` (or none): read, validated (a
/// `DECL` container whose rule table holds no rule §2.1 refuses) and
/// published to the store; their content id and their decoding.
fn publish_declared(store: &Store, path: Option<&Path>) -> Result<(Digest, Declared)> {
    let bytes = match path {
        Some(path) => {
            let bytes = std::fs::read(path)
                .map_err(|e| Failure::Input(format!("{}: {e}", path.display())))?;
            let declared = Declared::from_bytes(&bytes)
                .map_err(|e| Failure::Input(format!("{}: {e}", path.display())))?;
            // `spec/verify.md` §2.1 (WP-103): a table that holds a rule
            // §2.1 refuses keys nothing; it is an input error before any
            // job is planned, naming the rule (its index in the table)
            // and why, in the form of the container's own rule errors.
            if let Some((index, what)) = declared.rules().refused() {
                return Err(Failure::Input(format!("{}: rule {index}: {what}", path.display())));
            }
            bytes
        }
        None => Declared::none().to_bytes(),
    };
    let id =
        store.publish_declared(&bytes).map_err(|e| commands::store_failure(store.root(), &e))?;
    let declared = Declared::from_bytes(&bytes)
        .unwrap_or_else(|e| unreachable!("the declared changes were validated: {e}"));
    Ok((id, declared))
}

/// Standard output (§8.7): the report's text, the run's `wall`, `peak_rss`
/// and `run` lines, the `report` line, the verdict line; standard error:
/// the report's error lines.
fn print(
    report: &Report,
    summary: &pool::Summary,
    selected: &[Observer],
    path: Option<&Path>,
    out: &mut dyn Write,
    err: &mut dyn Write,
) {
    let _ = out.write_all(report.to_text().as_bytes());
    let (wall, peak) = commands::wall_and_peak(summary, selected);
    let _ = writeln!(out, "wall {wall}");
    if let Some((bytes, main)) = peak {
        let _ = writeln!(out, "peak_rss {bytes} {}", escape(main));
    }
    let _ = writeln!(out, "run {}", escape(&args::os_bytes(summary.run_path.as_os_str())));
    if let Some(path) = path {
        let _ = writeln!(out, "report {}", escape(&args::os_bytes(path.as_os_str())));
    }
    let _ = writeln!(out, "{}", report.verdict_line());
    let _ = out.flush();
    let _ = err.write_all(report.error_lines().as_bytes());
}

/// What `verify` appends to the worker command of `parse` (§8.7): both
/// always passed.
fn worker_options(findings: u64, locate: u64) -> [OsString; 4] {
    ["--findings".into(), findings.to_string().into(), "--locate".into(), locate.to_string().into()]
}

/// The `shard` of each observer's last `Job` entry in the run record (the
/// entries are in completion order; `None`: the last attempt wrote none).
fn last_shards(record: &RunRecord) -> HashMap<Digest, Option<Digest>> {
    record.jobs.iter().map(|j| (j.observer, j.shard)).collect()
}

/// The observer's shard of phase `verify` and `round`, if it is this run's
/// (`spec/verify.md` §7): there is one in the store, it decodes, and its
/// content id is the `shard` of the observer's last entry in the run
/// record. Anything else is no shard (`Report::add` with `None`: the
/// observer is `failed`).
fn this_runs_shard(
    store: &Store,
    plan: &Digest,
    round: u64,
    o: &Observer,
    last: &HashMap<Digest, Option<Digest>>,
) -> Result<Option<Shard>> {
    let Some(Some(expected)) = last.get(&o.id) else { return Ok(None) };
    let bytes = store
        .read_shard(plan, Phase::Verify, round, &o.id)
        .map_err(|e| commands::store_failure(store.root(), &e))?;
    Ok(bytes.and_then(|b| ours(&b, expected)))
}

/// The shard in `bytes`, if they decode and their content id is
/// `expected`.
fn ours(bytes: &[u8], expected: &Digest) -> Option<Shard> {
    match Shard::from_bytes_with_id(bytes) {
        Ok((s, id)) if id == *expected => Some(s),
        _ => None,
    }
}

/// The observer's map shard of round 0 as the run reads it: its status and
/// content id, from one decode (`None`: absent; `MapShard::Unusable`: bytes
/// that are not a valid shard of phase `map`, round 0 and this observer).
/// The id is what a `verified` or `mismatch` verdict must name as its `pre`
/// (`spec/verify.md` §9.3, WP-114).
fn map_status(store: &Store, plan: &Digest, observer: &Digest) -> Result<Option<MapShard>> {
    let bytes = store
        .read_shard(plan, Phase::Map, 0, observer)
        .map_err(|e| commands::store_failure(store.root(), &e))?;
    Ok(bytes.map(|b| match Shard::from_bytes_with_id(&b) {
        Ok((s, id)) if s.observer == *observer && s.phase == Phase::Map && s.round == 0 => {
            MapShard::Read { status: s.outcome.status, id }
        }
        _ => MapShard::Unusable,
    }))
}

/// The JSON report, written to a file beside `path` and renamed over it
/// (§8.7), so that a reader sees no report or a whole one.
fn write_report(path: &Path, report: &json::Value) -> Result<()> {
    let failure = |e: &dyn std::fmt::Display| Failure::Input(format!("{}: {e}", path.display()));
    let mut text = json::write_pretty(report).map_err(|e| failure(&e))?;
    if !text.ends_with('\n') {
        text.push('\n');
    }
    let tmp = beside(path);
    if let Err(e) = std::fs::write(&tmp, text.as_bytes()) {
        let _ = std::fs::remove_file(&tmp);
        return Err(failure(&e));
    }
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(failure(&e));
    }
    Ok(())
}

/// A temporary file in the directory of `path`: `.<name>.<pid>.tmp`.
pub(crate) fn beside(path: &Path) -> PathBuf {
    let name = path.file_name().map_or_else(|| "report".into(), |n| n.to_string_lossy());
    let tmp = format!(".{name}.{}.tmp", std::process::id());
    match path.parent() {
        Some(dir) => dir.join(tmp),
        None => PathBuf::from(tmp),
    }
}

/// A path for the text of §7.1: its characters, except that each byte
/// that is not UTF-8, each byte below 0x20, 0x7f, the backslash and each
/// byte of a C1 control (U+0080 to U+009F) is `\xNN`. The rule of
/// `slip_verify::run`'s own lines, for the lines this command adds.
pub(crate) fn escape(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    for chunk in bytes.utf8_chunks() {
        for c in chunk.valid().chars() {
            let n = c as u32;
            if n < 0x20 || n == 0x7f || c == '\\' || (0x80..=0x9f).contains(&n) {
                let mut buf = [0u8; 4];
                for b in c.encode_utf8(&mut buf).bytes() {
                    let _ = write!(out, "\\x{b:02x}");
                }
            } else {
                out.push(c);
            }
        }
        for b in chunk.invalid() {
            let _ = write!(out, "\\x{b:02x}");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    //! `spec/verify.md` §7 "This run's shard", which the executable cannot
    //! reach (between the pool's return and the reading of the shards
    //! nothing else writes to an exclusive store), and the escape of the
    //! lines this command adds to the text (§7.1).

    use super::*;
    use slip_model::formats::content_id;
    use slip_model::{End, Outcome, Run};

    fn shard(observer: Digest, message: &str) -> Vec<u8> {
        Shard {
            observer,
            phase: Phase::Verify,
            round: 0,
            outcome: Outcome {
                status: Status::Failed,
                errors: 0,
                warnings: 0,
                message: message.to_owned(),
            },
            inputs: None,
            counters: vec![],
            digests: None,
            verdict: None,
            edits: None,
            binding: None,
            rule_use: None,
            spellings: None,
            names: None,
            negatives: None,
            found: None,
        }
        .to_bytes()
    }

    fn record(jobs: Vec<slip_model::Job>) -> RunRecord {
        let run = Run {
            tool_slip: String::new(),
            tool_kiln: String::new(),
            plan: Digest([0; 32]),
            phase: Phase::Verify,
            round: 0,
            started_unix_ns: 0,
            wall_ns: 0,
            cores: 1,
            workers_max: 1,
            memory_budget: 0,
            peak_rss_sum: 0,
        };
        RunRecord { run, jobs }
    }

    fn job(observer: Digest, shard: Option<Digest>) -> slip_model::Job {
        slip_model::Job {
            observer,
            attempt: 1,
            end: End::Completed,
            shard,
            queued_ns: 0,
            wall_ns: 0,
            cpu_ns: 0,
            peak_rss: 0,
            estimate: 0,
        }
    }

    // §7: a shard is this run's only if its content id is the `shard` of
    // the observer's last entry; another run's (or one put there since) is
    // no shard.
    #[test]
    fn s7_this_runs_shard_by_the_last_entry() {
        let o = Digest([1; 32]);
        let mine = shard(o, "this run");
        let other = shard(o, "another run");
        let mine_id = content_id(&mine).unwrap();
        let other_id = content_id(&other).unwrap();
        assert!(ours(&mine, &mine_id).is_some());
        assert!(ours(&other, &mine_id).is_none(), "a stale shard is not this run's");
        assert!(ours(b"not a shard", &mine_id).is_none());

        // The last entry decides: a retry's shard replaces the first's.
        let r = record(vec![job(o, Some(other_id)), job(o, Some(mine_id))]);
        assert_eq!(last_shards(&r)[&o], Some(mine_id));
        let r = record(vec![job(o, Some(mine_id)), job(o, None)]);
        assert_eq!(last_shards(&r)[&o], None, "a last attempt without a shard");
    }

    // §7's map shard: one of the observer, phase `map` and round 0, else
    // unusable (WP-114 review F3: not "absent"). Through the executable the task refuses such a pre shard first
    // (the verify shard is then `failed` and no map shard is read), so the
    // filter is tested here, over a store whose files are put in place.
    #[test]
    fn s7_map_shard_of_the_observer_phase_and_round() {
        let dir = std::env::temp_dir().join(format!("slip-wp053-map-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = Store::open(&dir, Access::Exclusive).unwrap();
        let plan = Digest([9; 32]);
        let (a, b) = (Digest([1; 32]), Digest([2; 32]));
        let at = |observer: &Digest, bytes: &[u8]| {
            let path = store.shard_path(&plan, Phase::Map, 0, observer);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
        };
        let map = |observer: Digest, phase: Phase, round: u64| {
            let mut bytes = Shard::from_bytes(&shard(observer, "")).unwrap();
            bytes.phase = phase;
            bytes.round = round;
            bytes.outcome.status = Status::Ok;
            bytes.to_bytes()
        };
        assert_eq!(map_status(&store, &plan, &a).unwrap(), None, "absent");
        let ok = map(a, Phase::Map, 0);
        at(&a, &ok);
        let want = MapShard::Read { status: Status::Ok, id: content_id(&ok).unwrap() };
        assert_eq!(map_status(&store, &plan, &a).unwrap(), Some(want));
        for (wrong, why) in [
            (map(b, Phase::Map, 0), "another observer's"),
            (map(a, Phase::Verify, 0), "another phase's"),
            (map(a, Phase::Map, 1), "another round's"),
            (b"not a shard".to_vec(), "not a shard"),
        ] {
            at(&a, &wrong);
            assert_eq!(map_status(&store, &plan, &a).unwrap(), Some(MapShard::Unusable), "{why}");
        }
        drop(store);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    // §7.1: the bytes the text escapes.
    #[test]
    fn s7_1_escape() {
        assert_eq!(escape(b"/a/b c.cpp"), "/a/b c.cpp");
        assert_eq!(escape(b"a\nverdict verified"), "a\\x0averdict verified");
        assert_eq!(escape(b"a\\b"), "a\\x5cb");
        assert_eq!(escape(b"\x7f\xff"), "\\x7f\\xff");
        assert_eq!(escape("\u{85}é".as_bytes()), "\\xc2\\x85é");
    }
}
