//! TEST FIXTURE, not part of Slip: a worker (`spec/exec.md` §5) whose
//! behaviour for each job is chosen by the job's observer, for the tests of
//! `slip_exec` (`tests/worker.rs`, `tests/pool.rs`), which start it through
//! `CARGO_BIN_EXE_slip-fake-worker`.
//!
//! Usage: `slip-fake-worker [--jobs <n>] [--stack <bytes>] [--scratch 1]`
//! (defaults: 1 job, 16 MiB of stack, no scratch). It runs
//! `slip_exec::worker::serve` with the tool name `slip-fake-worker`.
//!
//! With `--scratch 1`, before each behaviour the worker opens the job's
//! store shared (once; kept for the process's life), makes its scratch
//! directory `tmp/<pid>.d/` with a file `f` in it and a loose file
//! `tmp/<pid>.fake.tmp`, and writes `slip-fake-worker: scratch <dir> <file>`
//! to standard error; a behaviour that returns removes the loose file, and
//! the scratch directory goes with the store when the process ends
//! normally. A worker killed or ending abruptly leaves both behind, for the
//! coordinator to remove (`spec/exec.md` §2 "Cleaning", WP-018).
//!
//! The behaviour is `observer.command.argv[1]`:
//!
//! | `argv[1]` | Behaviour |
//! |---|---|
//! | `ok` | a shard with status `ok`, `inputs = [main]`, counter `fake.jobs = 1` and the echo below; `Done` |
//! | `diagnosed` | a shard with status `diagnosed`, 1 error, message `fake diagnostic`; `Done` |
//! | `fail:<message>` | the task returns `Err(<message>)`; `Failed` with that message |
//! | `panic` | the task panics with `fake worker panic`; `Failed` with `panicked: fake worker panic`, then the worker ends (status 0) |
//! | `exit:<status>` | the process exits with `<status>` without answering |
//! | `abort` | the process dies by `SIGKILL` (sent to itself, so no core file) without answering |
//! | `hang` | sleeps for ever |
//! | `alloc:<MiB>` | allocates `<MiB>` MiB and touches every page, holds it for `argv[2]` milliseconds if present, then as `ok` |
//! | `sleep:<ms>` | sleeps `<ms>` milliseconds, then as `ok` |
//! | `garbage` | writes `ff ff ff ff` and `garbage` to standard output (a frame length above any limit) and exits 0 |
//! | `wrong-observer` | publishes the `ok` shard, answers `Done` naming another observer (the id with its first byte inverted), exits 0 |
//! | `no-shard` | answers `Done` with a shard id that is not in the store, without publishing, exits 0 |
//! | `late-output` | as `ok`; after `serve` returns (on `Exit` or end of input) writes `late output\n` to standard output, exits 0 |
//!
//! The worker speaks protocol 2 (`spec/exec.md` §5.2). Every shard it makes
//! echoes the job's two fields of protocol 2 in counters, each present
//! only when its field is: `fake.transform` (the first byte of the
//! transform's id); `fake.post.pre`, `fake.post.declared` (the first byte
//! of its id), `fake.post.image` (0 tree, 1 overlay), and
//! `fake.post.roots` (the number of root maps) or `fake.post.overlay` (the
//! first byte of the overlay's id).
//!
//! Anything else (or no `argv[1]`) is `Err("unknown fake behaviour …")`.
//! Every behaviour writes one line naming itself to standard error, which
//! the coordinator captures to the attempt's log.

#![forbid(unsafe_code)]

use std::io::Write as _;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use slip_exec::store::{Access, Store};
use slip_exec::worker::{Job, PostImage, ServeOptions, Task, ToCoordinator, serve, write_frame};
use slip_model::hash::content_digest;
use slip_model::{Counter, Outcome, Shard, Status};

/// `late-output` ran: write after `serve` returns.
static LATE: AtomicBool = AtomicBool::new(false);

struct Fake {
    /// `--scratch`: the store, opened on the first job.
    scratch: bool,
    store: Option<Store>,
}

impl Fake {
    /// The `--scratch` entries; returns the loose file.
    fn make_scratch(&mut self, job: &Job) -> Result<std::path::PathBuf, String> {
        if self.store.is_none() {
            let store = Store::open(job.store_path(), Access::Shared)
                .map_err(|e| format!("cannot open the store: {e}"))?;
            self.store = Some(store);
        }
        let Some(store) = &self.store else { unreachable!("the store was just opened") };
        let dir = store.scratch_dir().map_err(|e| format!("cannot make the scratch: {e}"))?;
        std::fs::write(dir.join("f"), b"scratch").map_err(|e| format!("scratch file: {e}"))?;
        let loose = store.root().join("tmp").join(format!("{}.fake.tmp", std::process::id()));
        std::fs::write(&loose, b"loose").map_err(|e| format!("loose file: {e}"))?;
        eprintln!("slip-fake-worker: scratch {} {}", dir.display(), loose.display());
        let _ = std::io::stderr().flush();
        Ok(loose)
    }
}

fn ok_shard(job: &Job) -> Shard {
    Shard {
        observer: job.observer.id,
        phase: job.phase,
        round: job.round,
        outcome: Outcome { status: Status::Ok, errors: 0, warnings: 0, message: String::new() },
        inputs: Some(vec![job.observer.main.clone()]),
        counters: echo(job),
        digests: None,
        verdict: None,
        edits: None,
        binding: None,
        rule_use: None,
        spellings: None,
    }
}

/// `fake.jobs = 1` and the job's protocol 2 fields, ascending by name.
fn echo(job: &Job) -> Vec<Counter> {
    let mut out = vec![("fake.jobs", 1)];
    if let Some(post) = &job.post {
        out.push(("fake.post.declared", u64::from(post.declared.0[0])));
        match &post.image {
            PostImage::Tree(maps) => {
                out.push(("fake.post.image", 0));
                out.push(("fake.post.pre", post.pre));
                out.push(("fake.post.roots", maps.len() as u64));
            }
            PostImage::Overlay(id) => {
                out.push(("fake.post.image", 1));
                out.push(("fake.post.overlay", u64::from(id.0[0])));
                out.push(("fake.post.pre", post.pre));
            }
        }
    }
    if let Some(t) = &job.transform {
        out.push(("fake.transform", u64::from(t.0[0])));
    }
    out.into_iter().map(|(name, value)| Counter { name: name.to_owned(), value }).collect()
}

/// Writes one message straight to standard output and ends the process.
fn answer_and_exit(msg: &ToCoordinator) -> ! {
    let mut out = std::io::stdout();
    let _ = write_frame(&mut out, &msg.encode());
    std::process::exit(0)
}

fn sleep_forever() -> ! {
    loop {
        std::thread::sleep(Duration::from_hours(1));
    }
}

impl Task for Fake {
    fn run(&mut self, job: &Job) -> Result<Shard, String> {
        let loose = if self.scratch { Some(self.make_scratch(job)?) } else { None };
        let result = behave(job);
        if let Some(loose) = loose {
            let _ = std::fs::remove_file(loose);
        }
        result
    }
}

fn behave(job: &Job) -> Result<Shard, String> {
    let argv = &job.observer.command.argv;
    let arg = |i: usize| argv.get(i).map(|a| String::from_utf8_lossy(a).into_owned());
    let behaviour = arg(1).unwrap_or_default();
    eprintln!("slip-fake-worker: {behaviour}");
    let (name, value) = behaviour.split_once(':').unwrap_or((&behaviour, ""));
    match name {
        "ok" => Ok(ok_shard(job)),
        "diagnosed" => {
            let mut s = ok_shard(job);
            s.outcome = Outcome {
                status: Status::Diagnosed,
                errors: 1,
                warnings: 0,
                message: "fake diagnostic".to_owned(),
            };
            Ok(s)
        }
        "fail" => Err(value.to_owned()),
        "panic" => panic!("fake worker panic"),
        "exit" => {
            let status: i32 = value.parse().map_err(|_| format!("exit: bad status {value:?}"))?;
            let _ = std::io::stderr().flush();
            std::process::exit(status)
        }
        "abort" => {
            let _ = std::io::stderr().flush();
            let pid = std::process::id().to_string();
            let _ = std::process::Command::new("kill").args(["-KILL", &pid]).status();
            std::thread::sleep(Duration::from_secs(5));
            std::process::abort()
        }
        "hang" => sleep_forever(),
        "alloc" => {
            let mib: usize = value.parse().map_err(|_| format!("alloc: bad size {value:?}"))?;
            let mut block = vec![0u8; mib << 20];
            for i in (0..block.len()).step_by(4096) {
                block[i] = 1;
            }
            let hold = arg(2).map_or(Ok(0), |ms| ms.parse::<u64>());
            let hold = hold.map_err(|_| "alloc: bad hold time".to_owned())?;
            std::thread::sleep(Duration::from_millis(hold));
            std::hint::black_box(&block);
            Ok(ok_shard(job))
        }
        "sleep" => {
            let ms: u64 = value.parse().map_err(|_| format!("sleep: bad time {value:?}"))?;
            std::thread::sleep(Duration::from_millis(ms));
            Ok(ok_shard(job))
        }
        "garbage" => {
            let mut out = std::io::stdout();
            let _ = out.write_all(b"\xff\xff\xff\xffgarbage");
            let _ = out.flush();
            std::process::exit(0)
        }
        "wrong-observer" => {
            let shard = ok_shard(job);
            let store = Store::open(job.store_path(), Access::Shared)
                .map_err(|e| format!("cannot open the store: {e}"))?;
            let id = store
                .publish_shard(&job.plan, job.phase, job.round, &job.observer.id, &shard.to_bytes())
                .map_err(|e| format!("cannot publish: {e}"))?;
            drop(store);
            let mut other = job.observer.id;
            other.0[0] = !other.0[0];
            answer_and_exit(&ToCoordinator::Done {
                observer: other,
                shard: id,
                cpu_ns: 0,
                peak_rss: 0,
            })
        }
        "no-shard" => answer_and_exit(&ToCoordinator::Done {
            observer: job.observer.id,
            shard: content_digest(b"slip-fake-worker: no such shard"),
            cpu_ns: 0,
            peak_rss: 0,
        }),
        "late-output" => {
            LATE.store(true, Ordering::SeqCst);
            Ok(ok_shard(job))
        }
        _ => Err(format!("unknown fake behaviour {behaviour:?}")),
    }
}

fn usage(message: &str) -> ExitCode {
    eprintln!(
        "slip-fake-worker: {message}\nusage: slip-fake-worker [--jobs <n>] [--stack <bytes>] \
         [--scratch 0|1]"
    );
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let mut options =
        ServeOptions { tool: "slip-fake-worker".to_owned(), stack_bytes: 16 << 20, max_jobs: 1 };
    let mut fake = Fake { scratch: false, store: None };
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let value = args.next();
        let Some(value) = value else { return usage(&format!("`{a}` needs a value")) };
        match a.as_str() {
            "--jobs" => match value.parse() {
                Ok(n) => options.max_jobs = n,
                Err(_) => return usage("bad --jobs"),
            },
            "--stack" => match value.parse() {
                Ok(n) => options.stack_bytes = n,
                Err(_) => return usage("bad --stack"),
            },
            "--scratch" => match value.as_str() {
                "0" => fake.scratch = false,
                "1" => fake.scratch = true,
                _ => return usage("bad --scratch"),
            },
            _ => return usage(&format!("unknown argument `{a}`")),
        }
    }
    // Unlocked handles: a behaviour may write to standard output from the
    // job thread while `serve` is waiting for it.
    let result = serve(fake, &options, std::io::stdin(), std::io::stdout());
    if let Err(e) = result {
        eprintln!("slip-fake-worker: {e}");
        return ExitCode::from(1);
    }
    if LATE.load(Ordering::SeqCst) {
        let mut out = std::io::stdout();
        let _ = out.write_all(b"late output\n");
        let _ = out.flush();
    }
    ExitCode::SUCCESS
}
