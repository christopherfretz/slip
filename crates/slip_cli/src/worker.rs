//! The hidden `slip __worker` (`spec/exec.md` §8.4): `serve` over standard
//! input and output with the parse task.
//!
//! The worker changes its current directory during a parse
//! (`slip_kiln::parse_observer`), so every path it uses is absolute: the
//! job's store path is (§2 "Path"). Its standard output carries protocol
//! messages only: nothing here prints there.

use std::io::Write;
use std::process::ExitCode;

use slip_exec::store::{Access, Store};
use slip_exec::worker::{Job, ServeOptions, Task, serve};
use slip_kiln::parse_observer;
use slip_model::Shard;

use crate::args::WorkerArgs;

/// The parse task: one observer through Kiln's front end, its shard
/// published in the job's store.
///
/// The store is opened shared on the first job and kept for the task's
/// lifetime; a job naming another store replaces it. The parse itself
/// writes no file (`spec/model.md` §8.4).
#[derive(Debug, Default)]
pub struct ParseTask {
    store: Option<Store>,
}

impl ParseTask {
    pub fn new() -> Self {
        Self::default()
    }

    fn open_store(&mut self, job: &Job) -> Result<(), String> {
        let path = job.store_path();
        if self.store.as_ref().is_some_and(|store| store.root() == path) {
            return Ok(());
        }
        self.store = None;
        let store = Store::open(path, Access::Shared)
            .map_err(|e| format!("cannot open the store {}: {e}", path.display()))?;
        self.store = Some(store);
        Ok(())
    }
}

impl Task for ParseTask {
    fn run(&mut self, job: &Job) -> Result<Shard, String> {
        self.open_store(job)?;
        parse_observer(&job.observer, job.phase, job.round)
    }
}

/// `slip __worker`: serves until the input ends, `--worker-jobs` jobs have
/// run, or a job panicked. Status 0 then; 1 if the protocol failed (the
/// coordinator sees a crash).
pub fn main(args: &WorkerArgs) -> ExitCode {
    let options = ServeOptions {
        tool: crate::version_line(),
        stack_bytes: usize::try_from(args.worker_stack).unwrap_or(usize::MAX),
        max_jobs: args.worker_jobs,
    };
    match serve(ParseTask::new(), &options, std::io::stdin(), std::io::stdout()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            let _ = writeln!(std::io::stderr(), "slip __worker: {e}");
            ExitCode::FAILURE
        }
    }
}
