# exec — running phases: store, jobserver, workers, memory admission

Status: draft (M0). Implemented by `slip_exec` (machinery; does not link
Kiln) and `slip_cli` (wiring). M0 has two phases: **plan** (`spec/cdb.md`)
and **parse**, which is the map phase with a task that only runs the front
end. M1 makes the map task write digests (`spec/model.md` §8.5) and adds
the **verify** phase with the commands `verify`, `declare` and `units`
(§8.4, §8.7 to §8.9). Reduce and apply are added by their milestones.

## 1. Principles

1. **A phase is a function of the plan and the store.** Workers get their
   input from the plan and write their output to the store; the coordinator
   keeps no state that cannot be rebuilt from the two.
2. **One observer, one process at a time.** An observer's work runs in a
   worker process, so its memory is returned to the system, a crash costs one
   observer, and its peak memory can be measured.
3. **Memory is the limit, not cores.** A job starts only when there is a CPU
   token *and* its memory estimate fits (§6).
4. **A shared machine.** Slip takes CPU tokens from the make jobserver when
   there is one (§3), returns them on every exit path including signals (§7),
   and never exceeds its memory budget on purpose.
5. **No failure is silent.** Every observer in the plan ends the phase with a
   shard, whose status says what happened (§5.4).

## 2. The store

`--store <dir>` (default `.slip` in the current directory).

```
<store>/
  VERSION                                   the text "1\n"
  plans/<plan id>.plan                      PLAN containers, named by content id
  declared/<declared id>.decl               DECL containers (spec/verify.md §2), named by content id
  shards/<plan id>/<phase>.<round>/<observer id>.shard
  runs/<plan id>/<phase>.<round>.<started_unix_ns>.run
  logs/<plan id>/<phase>.<round>/<observer id>.<attempt>.log     worker stderr, kept only for failed attempts
  tmp/                                      files being written
```

Ids are 64 hexadecimal digits (`spec/model.md` §3); `<phase>` is `map` or
`verify`; `<round>` is decimal, 0 for the first.

A run of a transform (M2) adds the directories and files of
`spec/edits.md` §10.4 (`transforms/`, `edits/`, `overlays/`, `results/`,
`blobs/`, the transform run records under `runs/`, and `write.journal`).
Everything in this section holds for them: they are published by rename,
never written in place, and a command that does not know them leaves them
alone. A store that holds `write.journal` has an unfinished write of the
working tree, and a command that opens it exclusively refuses to run
(`spec/edits.md` §7.3).

A run of a transform takes a round of its own for both of its phases
(`spec/edits.md` §8): the least number for which the plan has no shard
directory of either phase and no transform run record. Round 0 is
therefore the round of `slip parse`, and `slip verify` refuses a round
that a transform's run took (§8.7).

- **Publishing.** A file is written under `tmp/` with a name unique to the
  writing process and moved to its final name with one rename. A reader never
  sees a partial file. If the final name exists, the rename replaces it.
  A run record is published only under the plan, phase, round and start
  time it names (`spec/model.md` §9); any other is `Invalid`.
- **Opening.** A store whose `VERSION` is missing is initialised if the
  directory is empty or absent, and otherwise refused (`Input` error): Slip
  does not write into a directory it does not recognise. A `VERSION` other
  than `1` is refused.
- **Locking.** A command that runs a phase opens the store *exclusively*:
  it creates `<store>/lock` exclusively, holding its process id in decimal
  and a newline. If the file exists and its owner is alive (`/proc/<pid>`
  exists; where there is no `/proc` the owner is taken to be alive), the
  store is busy: an `Input` error naming the process and the lock file. A
  lock whose owner no longer exists is taken over. Every other command, and
  every worker, opens the store *shared*: no lock; it only reads and
  publishes, which is safe beside a running phase. The lock is removed on
  every exit path its owner controls (§7).
- **Cleaning.** `tmp/` entries are removed by the process that made them, on
  every exit path it controls. The coordinator, which holds the store
  exclusively, removes the entries of each of its workers, directories
  included, when it reaps that worker (whatever ended it: an exit, a crash,
  a kill for a memory cap, pressure or the time limit, §5.4, §6) and when
  it gives up on it after killing it (§7); so a phase that returns leaves
  no entry of its workers. Other stale entries (from a killed process),
  directories included, are removed when the store is opened exclusively.
- **Scratch.** A process that needs temporary files of its own (a worker,
  for what the front end writes) keeps them in the directory
  `tmp/<process id>.d/`, which is a `tmp/` entry like any other.
- **Path.** The store's path is made absolute when it is opened; that is the
  path a `Job` carries (§5.2), since a worker changes its current directory.

### 2.1 Store rules

- **Root.** `open` makes the directory absolute against the current
  directory (`std::path::absolute`: not canonicalised, symbolic links
  kept), so `root()` and a `Job.store` made from it stay valid after a
  worker changes its current directory.
- **Opening.** `VERSION` must be exactly the bytes `1\n`; anything else
  (including `1` without the newline) is `Version` with the text, lossily
  decoded. A missing `VERSION` in an absent or empty directory initialises
  it, in both access modes: `tmp/`, `plans/`, `declared/`, `shards/`,
  `runs/`, `logs/` are created, then `VERSION` is published (a `tmp/` file and a rename)
  last, so a directory with `VERSION` is complete. A non-empty directory
  without `VERSION`, or a path that is not a directory, is `Unrecognised`.
- **`tmp/` names.** A file being written is `tmp/<pid>.<n>.tmp`, `n` a
  process-wide counter; the scratch directory is `tmp/<pid>.d`. Every entry
  the store makes starts with `<pid>.`, `<pid>` in canonical decimal, which
  is what stale cleaning and `remove_entries_of` read.
- **Publishing validates.** `publish_plan` requires a valid `PLAN` file and
  names it by its content id; `publish_declared` a valid `DECL` file,
  named by its content id (the directory `declared/` is made when the
  store is initialised and, for a store initialised before it existed, by
  the first `publish_declared`); `publish_shard` a valid `SHRD` whose header
  names the observer, phase and round given; `publish_run` a valid `RUNR`
  whose `plan`, `phase`, `round` and `started_unix_ns` are the ones given.
  Anything else is `Invalid` and nothing is written. The `read_*`
  methods and `latest_run` return the bytes unvalidated; `None` when the
  file does not exist. Files are not `fsync`ed.
- **The lock** holds `<pid>\n`. It is written in full under `tmp/` and
  hard-linked to `lock`, so it never exists without its pid. If `lock`
  exists: a content other than a canonical decimal pid and a newline is an
  `Io` error (`InvalidData`) and the lock is left alone; a live owner
  (including this process, for a second exclusive open) is `Busy`; a dead
  owner's lock is renamed into `tmp/` and, if the file moved still names
  that dead pid, removed and the link retried; if it names another process
  (a racing opener got there first) it is linked back and that process is
  reported `Busy`. At most 8 rounds, then `Busy`.
- **Stale cleaning** (exclusive open, after the lock is taken): every
  `tmp/` entry whose name starts with `<pid>.` for a process that does not
  exist is removed, directories recursively. Entries of live processes
  (shared writers beside the phase) and names not of that form are kept.
- **The entries of one process.** `remove_entries_of(pid)` removes every
  `tmp/` entry whose name is `<pid>.…` with `<pid>` the canonical decimal
  of `pid`, directories recursively, in either access mode. The caller
  vouches that the process is gone: `/proc` is not consulted. This
  process's own id is a no-op. No `tmp/`, or an entry that vanishes
  meanwhile, is not an error; any other failure is returned (the first
  one) after the remaining entries have been tried.
- **Closing.** `Drop` and `cleanup_on_signal` run the same cleanup: mark the
  store closed, remove the `tmp/` files it has open, remove its scratch
  directory recursively if `scratch_dir` was called on it, and remove
  `lock` if it holds it *and* the file still holds this process's pid.
  Errors are ignored; it never panics; it is idempotent. Once closed, a
  store refuses to create `tmp/` files or the scratch directory (`Io`,
  `Interrupted`). `tmp/` files are created under the same mutex the cleanup
  takes, so none is created after the cleanup and left behind.
- **`find_plan`** returns, ascending, the ids of `plans/<64 hex>.plan`
  files that start with the prefix. A prefix that is longer than 64 or not
  lower-case hexadecimal matches nothing; the empty prefix matches all.
- **`latest_run`** considers `runs/<plan>/<phase>.<round>.<ns>.run` names
  with canonical decimal `round` and `ns` only; the greatest `ns` wins, then
  the greatest `round`.
- **Logs.** `log_path` creates the parent directories, not the file.
  `remove_log` of an absent log succeeds.

### 2.2 Interface

`slip_exec::store`:

```rust
pub const VERSION_TEXT: &str = "1\n";
pub fn phase_name(phase: Phase) -> &'static str;            // "map", "verify"
pub enum Access { Exclusive, Shared }
pub enum StoreError { Unrecognised(PathBuf), Version(String), Busy { pid: u32, lock: PathBuf },
                      Invalid(String), Io { path: PathBuf, error: io::Error } }   // Display, Error
pub struct Store;   // Drop: §2.1 "Closing"
impl Store {
    pub fn open(dir: &Path, access: Access) -> Result<Store, StoreError>;
    pub fn root(&self) -> &Path;                                  // absolute
    pub fn scratch_dir(&self) -> Result<PathBuf, StoreError>;     // <root>/tmp/<pid>.d, created, idempotent
    pub fn remove_entries_of(&self, pid: u32) -> Result<(), StoreError>;
    pub fn plan_path(&self, id: &Digest) -> PathBuf;
    pub fn shard_path(&self, plan: &Digest, phase: Phase, round: u64, observer: &Digest) -> PathBuf;
    pub fn run_path(&self, plan: &Digest, phase: Phase, round: u64, started_unix_ns: u64) -> PathBuf;
    pub fn publish_plan(&self, bytes: &[u8]) -> Result<Digest, StoreError>;
    pub fn read_plan(&self, id: &Digest) -> Result<Option<Vec<u8>>, StoreError>;
    pub fn find_plan(&self, hex_prefix: &str) -> Result<Vec<Digest>, StoreError>;
    pub fn declared_path(&self, id: &Digest) -> PathBuf;
    pub fn publish_declared(&self, bytes: &[u8]) -> Result<Digest, StoreError>;
    pub fn read_declared(&self, id: &Digest) -> Result<Option<Vec<u8>>, StoreError>;
    pub fn publish_shard(&self, plan: &Digest, phase: Phase, round: u64, observer: &Digest, bytes: &[u8]) -> Result<Digest, StoreError>;
    pub fn read_shard(&self, plan: &Digest, phase: Phase, round: u64, observer: &Digest) -> Result<Option<Vec<u8>>, StoreError>;
    pub fn publish_run(&self, plan: &Digest, phase: Phase, round: u64, started_unix_ns: u64, bytes: &[u8]) -> Result<PathBuf, StoreError>;
    pub fn latest_run(&self, plan: &Digest, phase: Phase) -> Result<Option<Vec<u8>>, StoreError>;
    pub fn log_path(&self, plan: &Digest, phase: Phase, round: u64, observer: &Digest, attempt: u64) -> Result<PathBuf, StoreError>;
    pub fn remove_log(&self, plan: &Digest, phase: Phase, round: u64, observer: &Digest, attempt: u64) -> Result<(), StoreError>;
}
pub fn cleanup_on_signal(store: &Store) -> impl FnOnce() + Send + 'static;
```

Every `StoreError` is an `Input` error of the command (§8.5, status 3).

## 3. Jobserver

`slip_exec::jobserver` is a GNU make jobserver client. Its behaviour is
Kiln's (`kiln/spec/harness.md` "Jobserver"), from which the implementation is
ported, with these names:

- **Honoured**: `MAKEFLAGS` containing `--jobserver-auth=fifo:<path>` (the
  last such word wins). The fifo is opened read+write. A token is one byte
  read from it; the same byte is written back on release.
- **The implicit token.** The process owns one token that is not in the pool:
  one job may run without reading the fifo; every further concurrent job
  holds one pool byte for as long as it runs.
- **`-j N`** is an upper bound on concurrent jobs (default: the number of
  cores available to the process). The pool can only lower it.
- **Release** is by `Drop`, on every path: normal completion, error, panic,
  a job killed by a cap or a timeout.
- **Fallback.** With no `MAKEFLAGS`, the descriptor forms
  (`--jobserver-auth=R,W`), a path that is not a fifo or cannot be opened, or
  a later fifo I/O error: acquisition becomes free, and `-j` alone bounds
  parallelism. A warning is printed for a fifo that was named and cannot be
  used, and for a later fifo I/O error (§3.1); none without `MAKEFLAGS` or
  for the descriptor forms.
- **Inspection.** `status(fifo)` counts idle tokens without blocking (read
  until the fifo would block, write the same bytes back); `refill(fifo, n)`
  tops the pool up to `n` idle tokens and is only correct while no client
  holds tokens.

A token is acquired by a helper thread and handed to the scheduler over a
channel, so the scheduler never blocks in a read and can stop waiting when a
job finishes (the implicit token becoming free) or the run is cancelled. A
byte read after the scheduler stopped wanting it is written straight back.

### 3.1 Requests and release

- **Requests.** `request` queues a waiter served in order. Its token is
  passed to `deliver` exactly once. That happens on the caller's thread if
  the implicit token is free or acquisition is free, on the helper thread
  after a read, or on the thread dropping another token, which hands the
  token straight to the oldest waiter. `deliver` must not block. Dropping
  the `Request` before delivery abandons it, and a byte read for it later
  is written straight back. A token delivered into a channel nobody reads
  any more is released when the channel drops it. `acquire` is a request
  served over a private channel.
- **`try_acquire`** takes the implicit token, a free token when acquisition
  is free, or one byte read with `O_NONBLOCK` from a second descriptor.
  Otherwise it returns `None` and never blocks. Where `O_NONBLOCK`'s value is
  not known for the platform, it only ever gets the implicit or a free token.
- **Fallback warnings** are printed once per pool (a later fifo error), and
  once per `from_makeflags` call that names a fifo it cannot use. Every
  message on these paths ignores write errors.
- **`release_all`** (per pool, or for every pool of the process) writes back
  every byte held by live tokens, including tokens delivered but not
  received. It then serves pending requests free tokens and marks the pool
  released: later drops return nothing and later acquisitions are free. It
  waits up to 200 ms for a helper read in flight to put its byte back.
- The helper thread ends once only it still refers to the pool (checked
  every second while idle), after `release_all`, or on a fifo error.

### 3.2 Interface

`slip_exec::jobserver`:

```rust
pub const DEFAULT_FIFO: &str = "/run/slip/jobserver.fifo";
pub const DEFAULT_POOL_SIZE: usize = 8;
pub fn fifo_from_makeflags(flags: &str) -> Option<PathBuf>;
pub fn default_fifo() -> PathBuf;                 // MAKEFLAGS fifo:, else $SLIP_JOBSERVER_FIFO, else DEFAULT_FIFO
pub fn resolve_fifo(makeflags: Option<&str>, fifo_var: Option<PathBuf>) -> PathBuf;
pub fn default_pool_size() -> Result<usize, String>;   // $SLIP_JOBS, else 8
pub fn resolve_pool_size(jobs_var: Option<&str>) -> Result<usize, String>;
pub fn available_cores() -> usize;

#[derive(Clone)] pub struct Pool;                 // clones share the connection
impl Pool {
    pub fn none() -> Pool;
    pub fn from_env() -> Pool;                    // MAKEFLAGS, with the fallback of §3
    pub fn from_makeflags(flags: &str) -> Pool;
    pub fn from_fifo(path: &Path) -> io::Result<Pool>;
    pub fn with_jobs(self, jobs: usize) -> Pool;  // -j, at least 1; default available_cores()
    pub fn jobs(&self) -> usize;
    pub fn describe(&self) -> Option<String>;     // "fifo:<path>"
    pub fn is_free(&self) -> bool;
    pub fn acquire(&self) -> Token;               // blocking
    pub fn try_acquire(&self) -> Option<Token>;   // never blocks
    pub fn request(&self, deliver: impl FnOnce(Token) + Send + 'static) -> Request;
    pub fn release_all(&self) -> usize;
}
pub struct Token;   pub fn kind(&self) -> Kind;   // released on Drop
pub enum Kind { Free, Implicit, Pool(u8) }
pub struct Request; pub fn is_pending(&self) -> bool;   // Drop abandons
pub fn release_all() -> usize;                    // every pool of the process
pub struct RefillReport { pub idle: usize, pub added: usize }
pub fn status(fifo: &Path) -> io::Result<usize>;
pub fn refill(fifo: &Path, jobs: usize) -> io::Result<RefillReport>;
```

## 4. Child processes

`slip_exec::process` starts and supervises children without unsafe code:

- **Own process group.** Every child is the leader of a new process group, so
  the whole tree under it can be signalled together.
- **Address-space cap.** A child given a cap is started through
  `sh -c 'ulimit -v <KiB> || exit 125; ulimit -c 0 2>/dev/null; exec "$@"' slip-mem-limit <program> <args…>`,
  which applies `RLIMIT_AS` and disables core dumps before `exec`. If the
  cap cannot be applied the wrapper exits with status 125 rather than run
  the program uncapped. If `sh` is not available the child is started
  directly and a warning is printed once.
- **Sampling.** While a child runs, its resident size is read from
  `/proc/<pid>/status` (`VmRSS`, `VmHWM`) at an interval (default 50 ms). The
  process sampled is the child itself (after the wrapper's `exec` that is the
  program); its descendants are not sampled, and workers start none. On a
  system without `/proc` every measurement is 0 ("unknown") and
  resident-size caps are not enforced.
- **Killing.** `SIGKILL` to the process group. A child killed for exceeding a
  cap or a time limit is distinguished from one that died by itself: the
  supervisor records why it sent the signal before sending it.
- **Table.** Every live child is in a process-wide table from spawn to reap,
  which is what signal handling (§7) walks.

### 4.1 Child rules

- **Wrapper.** A child with `address_space = Some(b)` runs under the
  wrapper of §4 with `<KiB>` = `b / 1024`, rounded down and at least 1.
  If `ulimit -v` fails, the wrapper exits 125 (`ULIMIT_FAILED`) instead of
  running uncapped. `Some(0)` is refused with `InvalidInput`. `sh` counts
  as "not available" when it is not on the command's own `PATH` (if it
  sets one) or the process's `PATH`. The child then starts directly with
  its streams, and one warning is printed per process.
- **Kill.** `kill` records the reason, unless one is already recorded (the
  first one wins). It then sends `SIGKILL` to the group (`kill -KILL --
  -<pid>`, else the shell builtin) and to the pid. All of this happens under
  the child's lock and only while it is unreaped, so a reused pid is never
  signalled. A child that ended by itself, even by a signal, has no reason.
- **Supervision.** `check` samples first, then kills if `rss > rss_cap`
  (`Memory`) or if the time since spawn is at least `timeout` (`Time`). A
  sample of 0 never exceeds a cap. `wait_supervised` repeats this every
  `interval` until the child exits.
- **Table.** A child is in the table from spawn to reap. A shutdown records
  `Signal`, kills every group and reaps the children itself, so it does not
  depend on their owners polling. A spawn in flight is counted and killed as
  it registers, and every later spawn fails with `Interrupted`. `wait` polls
  `try_wait` (1 ms rising to 25 ms) instead of blocking in `waitpid`, so a
  shutdown is never locked out of a child. Dropping an unreaped `Child`
  kills its group and reaps it.

### 4.2 Interface

`slip_exec::process`:

```rust
pub const SAMPLE_INTERVAL: Duration;              // 50 ms
pub const ULIMIT_FAILED: i32 = 125;
pub const WRAPPER_NAME: &str = "slip-mem-limit";
pub struct Limits { pub address_space: Option<u64>, pub timeout: Option<Duration>, pub rss_cap: Option<u64> }
pub enum Killed { Memory, Time, Signal }
pub struct Sample { pub rss: u64, pub hwm: u64 }  // bytes; from_proc_status(&str), max
pub trait Sampler: Send + Sync { fn sample(&self, pid: u32) -> Sample; }
pub struct ProcSampler;
pub struct Io { pub stdin: Stdio, pub stdout: Stdio, pub stderr: Stdio }   // null(), piped()
pub struct Exit { pub status: ExitStatus, pub killed: Option<Killed>, pub peak: Sample, pub elapsed: Duration }
pub struct Child;
impl Child {
    pub fn spawn(cmd: &Command, io: Io, limits: &Limits) -> io::Result<Child>;
    pub fn id(&self) -> u32;                      // = its process group id
    pub fn limits(&self) -> &Limits;
    pub fn take_stdin/take_stdout/take_stderr(&mut self);
    pub fn elapsed(&self) -> Duration;
    pub fn kill(&self, reason: Killed);
    pub fn killed(&self) -> Option<Killed>;
    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>>;
    pub fn wait(&mut self) -> io::Result<ExitStatus>;
    pub fn sample(&mut self, sampler: &dyn Sampler) -> Sample;
    pub fn peak(&self) -> Sample;
    pub fn check(&mut self, sampler: &dyn Sampler) -> Option<Killed>;
    pub fn wait_supervised(&mut self, sampler: &dyn Sampler, interval: Duration) -> io::Result<Exit>;
}
pub fn live_children() -> Vec<u32>;
pub fn shutting_down() -> bool;
pub fn kill_live_children(grace: Duration) -> usize;
pub fn kill_live_children_now();
```

## 5. Workers

### 5.1 Roles

The **coordinator** is the `slip` process the user started. A **worker** is a
child process started as `<worker command>` (for `slip`: the same executable
with the hidden subcommand `__worker`). `slip_exec` is given the worker
command and, on the worker side, a `Task`:

```rust
pub trait Task {
    /// Do one observer's work and return the shard to publish.
    /// An `Err` is an internal failure of the worker, not a diagnostic about
    /// the subject: diagnostics are a shard with status `diagnosed`.
    fn run(&mut self, job: &Job) -> Result<Shard, String>;
}
```

`Job` carries the plan's `Observer` record, the phase and the round, and
what else the job of that phase needs (§5.2). `Shard`
is the model type of `spec/model.md` §8. `slip_exec::worker::serve(task)` is
the worker's main loop.

### 5.2 Protocol

Coordinator and worker talk over the worker's standard input and output.
Each message is a `u32le` length followed by that many bytes, which are one
record in the encoding of `spec/model.md` §2. The worker's standard error is
captured to the attempt's log file.

| Direction | Message | Fields |
|---|---|---|
| to worker | `Job = 1` | `store: bytes` (path), `plan: digest`, `phase`, `round: uv`, `observer: Observer`, `transform: opt<digest>`, `post: opt<Post>` |
| to worker | `Exit = 2` | — |
| to coordinator | `Ready = 1` | `protocol: uv` (2), `tool: str` |
| to coordinator | `Done = 2` | `observer: digest`, `shard: digest` (content id), `cpu_ns: uv`, `peak_rss: uv` |
| to coordinator | `Failed = 3` | `observer: digest`, `message: str` |

Each message starts with its tag as a `uv`.

```
Post      := pre:      uv              the round of the map shards the job compares against (spec/verify.md §5.2 step 0)
             declared: digest          content id of a DECL container in the job's store (§2)
             image:    PostImage       spec/verify.md §5.1
PostImage := tree = 0:    list<RootMap>     strictly ascending by `subject`; empty: the identity
           | overlay = 1: digest            content id of an OVLY container in the job's store (spec/edits.md §7.1)
RootMap   := subject: path, post: path
```

**Protocol 2.** The protocol number is 2 since M2, which added the two
last fields of `Job`; there was one change, and protocol 1 (M0, M1 before
the verify command) is `Job` without them and nothing else. The fields
carry what a job needs beyond its observer:

- `transform`, for a map job that runs a transform after the parse and
  the digest (`spec/edits.md` §8.1): the content id of a `TRFM` container
  in the job's store. Absent, the job is the map job of `spec/model.md`
  §8.5.
- `post`, for every verify job: the round's parameters. One pool of
  workers can then serve any round, a worker started for one run is not
  tied to it, and a verify shard records what it was made with
  (`spec/verify.md` §9, `binding`).

A map job with a `post`, a verify job with a `transform` and a verify job
without a `post` are refused by the task: the worker answers `Failed`,
naming which (§8.4). A coordinator refuses a worker whose `Ready` names
another protocol than its own (a protocol violation, §5.6). Nothing is
negotiated: coordinator and worker are one executable.

The worker sends `Ready` once at start. For each `Job` it runs the task on a
thread with a large stack (`--worker-stack`, default 1 GiB reserved), catches
a panic of that thread, publishes the shard to the store (§2) and answers
`Done`, or answers `Failed`. Before `Done` it reads its own CPU time and peak
resident size from `/proc/self` (0 if unavailable). On `Exit` or end of
input it exits with status 0. The coordinator does not send `Exit`: closing
the worker's standard input is its `Exit` (§5.6).

A worker runs at most `--worker-jobs` jobs (default 1) and is then replaced
by a new process. With the default every observer gets a fresh process, so
`peak_rss` is that observer's alone. With a larger value the worker resets
its peak counter between jobs where the system allows it
(`/proc/self/clear_refs`); where it does not, `peak_rss` is reported as 0.

Anything else is a **protocol violation** and is treated as a crash: a
message that does not decode, a `Done` for another observer, a `Done` whose
shard is not in the store or does not open as a `SHRD` for that observer and
phase, output from a worker that holds no job. §5.6 "Messages" lists every
violation.

### 5.3 Scheduling

Pending jobs are in plan order (largest first). A job is started when all of
these hold: the number running is below `-j`; a token is held (§3); and
admission accepts it (§6).

### 5.4 What every observer ends with

| What happened | `End` (run record) | Shard |
|---|---|---|
| The worker answered `Done` | `completed` | the worker's |
| The worker answered `Failed`, exited, was killed by a signal, or violated the protocol | `crashed` | after the retry (below), a shard with status `failed` written by the coordinator; `message` is the worker's message, or one line saying why followed by the end of its log (§5.6 "The coordinator's shard") |
| Killed for exceeding its memory cap | `killed_memory` | as above, after the retries of §6 |
| Killed for exceeding `--observer-timeout` (default 3600 s; 0 disables) | `killed_time` | `failed`, no retry |
| The run was interrupted | `cancelled` | none |

A crashed job is retried once. Every attempt is a `Job` entry in the run
record, with its attempt number.

### 5.5 The worker's side

- **Messages.** The tag is checked first: an unknown tag (0 included) is
  `Malformed` at offset 0. The observer is checked by `decode_observer`
  (`spec/model.md` §5.5), the phase by `decode_phase`. A `PostImage` tag
  other than 0 and 1 is `Malformed` at the tag; root maps that are not
  strictly ascending by `subject` are `Invalid` at the first that is out
  of order. A byte after the last field is `Malformed` at that byte
  (`Decoder::finish`).
- **Frames.** `read_frame` returns `None` only if the input ends before
  the first byte of a length; an end inside the length or the payload is
  `UnexpectedEof`; a length above `max` is `InvalidData`, refused after
  reading the four length bytes and before allocating; the payload buffer
  grows as bytes arrive. `write_frame` refuses a payload of 4 GiB or more
  (`InvalidInput`). Either side accepts frames of at most `FRAME_MAX`
  (64 MiB).
- **`serve`.** Sends `Ready { protocol: PROTOCOL, tool }`. Then, until `max_jobs`
  jobs have been answered: reads a frame (end of input: returns `Ok`);
  `Exit` returns `Ok`; a frame that does not decode, or is over
  `FRAME_MAX`, returns an `InvalidData` error (the worker's `main` then
  exits non-zero, which the coordinator sees as a crash). `max_jobs = 0`
  sends `Ready` and returns without reading.
- **A job** runs on a scoped thread named `slip-job` with `stack_bytes` of
  stack (if the thread cannot be started, `Failed`). Its shard must pass
  `Shard::validate` and name the job's observer, phase and round, or the
  answer is `Failed` with a message saying so. The store named by the job
  is opened `Shared` and the shard published; a failure there is `Failed`.
  Otherwise `Done` with the shard's content id.
- **A panic** of the job thread is `Failed` with the message
  `panicked: <panic message>` (`a panic with a non-text payload` if it has
  none), after which `serve` returns `Ok` even if jobs remain: the task's
  state is suspect, so the next observer gets a fresh process. The panic
  hook's report goes to standard error, that is, the attempt's log.
- **`Failed.message`** is clipped to 4096 bytes at a character boundary
  (`clip_message`) by `serve`, since it only ever becomes a shard message.
- **`cpu_ns`** is user plus system time of the whole process from
  `/proc/self/stat` (fields 14 and 15, in units of `USER_HZ` = 100), as the
  difference since the end of the previous job, or since `serve` started
  for the first job, so the jobs of one worker add up to its CPU time
  after startup. 0 if unavailable.
- **`peak_rss`** is `VmHWM` of `/proc/self/status`, read before `Done`.
  Before every job but the first, `serve` writes `5` to
  `/proc/self/clear_refs`; if that fails, that job's `peak_rss` is 0.

### 5.6 The coordinator

`slip_exec::pool::run` runs one phase over a list of observers.

- **The loop waits** in `recv_timeout` on one channel until the next
  sampling instant. Into that channel go: each worker's messages (a thread
  per worker reads its standard output, one frame at a time, at most
  `FRAME_MAX`, and reports end of output), the end of its standard
  error (a second thread copies it to the current attempt's log), tokens
  (`jobserver::Pool::request` delivers into it), and the cancellation
  (`pool::cleanup_on_signal`, §7.1). A third thread per worker writes its
  standard input, so a worker that does not read cannot block the loop.
  Exited children are reaped with `try_wait` at each wakeup; at each
  sampling instant (`sample_interval`, at least 1 ms) the running workers
  are sampled and the time limit checked. While a worker is ending (its
  output ended, it is reaped but its streams are open, its input is
  closed, or it is being killed) the wait is at most 5 ms. Nothing blocks
  the loop otherwise.
- **Tokens and `-j`.** At most `min(options.jobs, tokens.jobs())` (each at
  least 1) jobs run. A token is requested only when fewer run and
  `Admission::peek` would start a job; at most one request is outstanding.
  A token that arrives when no job can start is dropped (returned). Each
  job holds its token until its attempt is finished (for a killed job:
  until the process is reaped), then drops it. When a job starts, the next
  token is requested at once, so jobs whose tokens are available now start
  before the next sampling instant. `workers_max` counts running jobs.
- **Workers.** Started as `program args…`, `env_remove` applied, all three
  streams piped, under `Admission::address_space_cap` (none with `B = 0`),
  with no `Limits::rss_cap` or `timeout`: caps and the time limit are the
  coordinator's (the time limit runs from the job's start, not the
  process's). The `Job` is sent at once (not after `Ready`). A worker gets
  at most `worker_jobs` jobs; after the last its input is closed (that is
  its `Exit`; the `Exit` message is never sent). An idle worker is reused
  while jobs are pending; once none is pending its input is closed. A
  worker whose input is closed and that has no job is killed if it has not
  exited 5 s later; one whose output ended and that has not exited 1 s
  later is killed. `worker_jobs` must not exceed the worker's own limit in
  `args`, or an idle worker may have exited when it is given a job (that
  job then crashes and is retried).
- **Messages.** The first message must be `Ready` with protocol 2 (its
  `tool` is not checked). Violations, each a crash of the job the worker
  holds (if any) with the message `protocol violation: <what>`: a frame
  that does not decode or cannot be read, a second `Ready`, any other
  message first, `Done` or `Failed` with no job or for another observer,
  a `Done` whose shard is absent from
  `shards/<plan>/<phase>.<round>/<observer>.shard`, unreadable, not valid,
  of another content id, or for another observer, phase or round. The
  worker is killed and never reused; later messages are ignored. Output
  from a worker that holds no job is a violation with no job to fail. A
  job being killed (cap, pressure, time) ignores the worker's later `Done`
  or `Failed`: the decision stands.
- **A worker that answered `Failed`** is not reused (its input is closed).
- **End of an attempt** (§5.4). `Done` and `Failed` finish the attempt when
  they arrive. Otherwise the attempt is finished once its process is reaped
  and both its streams ended, or 1 s after the reap. An attempt whose
  worker ends without answering is a crash: `the worker ended without
  answering (<exit status>)`. A worker that cannot be started is a crash
  of the job: `cannot start the worker <program>: <error>`.
- **Attempts and retries.** A crash (including `Failed`) is retried once,
  at the head of the queue, as attempt + 1. A cap kill with `retry` is
  attempt + 1. A pressure kill keeps the attempt number (it is not an
  attempt): the run record has two entries with that number, the first
  `killed_memory` with no shard. The crash retry and the cap retry are
  independent: a job can have both.
- **The coordinator's shard** (`failed`, no `inputs`, no counters) carries,
  for `Failed`, the worker's message (`the worker failed without a message`
  if empty); otherwise one line saying why (`protocol violation: …`, `the
  worker ended without answering (…)`, `killed: resident size N bytes
  exceeded the cap of M bytes`, `killed: exceeded the observer timeout of
  Ns`), then a newline and as much of the end of the attempt's log as fits
  in 4096 bytes (cut at a character boundary, lossily decoded, trailing
  white space removed). The final attempt's run-record entry carries its
  content id; earlier ones carry none.
- **Logs.** A log is created (truncated) when the attempt starts and
  receives the worker's standard error while it holds that job; bytes the
  worker writes while idle go nowhere. With `worker_jobs > 1` the split
  between consecutive jobs follows the coordinator's assignment, not the
  worker's timing. The log is removed for `completed`, pressure-killed and
  `cancelled` attempts, and kept for every other end.
- **Workers' `tmp/` entries** (§2 "Cleaning"). The coordinator removes a
  worker's entries (`Store::remove_entries_of`) at the moment
  `Child::try_wait` first returns the worker's status (whatever ended it),
  before anything else is started, so the pid cannot have been given to a
  new worker of this run; and, when the run is abandoned (cancellation or
  an error, below), for every worker still known, reaped or not: each has
  been sent `SIGKILL` and runs no more user code. Each worker's entries are
  removed once. Failures are ignored: the next exclusive open removes what
  is left (§2). The worker's process group is not searched for descendants
  (a worker runs the front end in its own process).
- **Run record.** Jobs in the order their attempts finished; `queued_ns`
  is from the start of `run` to the attempt's start; `wall_ns` to its
  finish; `cpu_ns` is the worker's (0 unless `Done`); `peak_rss` is the
  larger of the largest sampled `VmRSS` (and the sampled `VmHWM` for a
  process's first job) and `Done.peak_rss`; `estimate` is the `e` the job
  was admitted with (§6); `cores` is `jobserver::available_cores()`;
  `peak_rss_sum` the largest sum of one sampling round; `memory_budget` is
  `B`.
- **Summary.** `Summary::statuses` counts the final shards by status,
  `ends` the run-record entries by end. `Summary::outcomes` has one element
  per observer given to `run`, in that order (plan order), for the final
  shard: its observer id, its `outcome.status`, and the first line of its
  `outcome.message` as `str::lines` gives it (empty for an empty message).
  For a shard the worker wrote, it comes from the shard the coordinator
  already reads to check a `Done`; for one the coordinator writes, from
  the message it writes (clipped as written). `statuses` counts the same
  statuses.
- **Errors.** A store error reading the previous run record is returned
  before anything starts. A store or I/O error later (a log or shard that
  cannot be written, a child that cannot be waited for) ends the run: the
  workers are killed and reaped (at most 10 s), every job not finished is
  recorded `cancelled`, the run record is published (errors ignored) and
  the error returned.

### 5.7 Interface

`slip_exec::worker`:

```rust
pub const PROTOCOL: u64 = 2;
pub const FRAME_MAX: usize = 64 << 20;
pub struct Job { pub store: Vec<u8>, pub plan: Digest, pub phase: Phase, pub round: u64, pub observer: Observer,
                 pub transform: Option<Digest>, pub post: Option<Post> }
pub struct Post { pub pre: u64, pub declared: Digest, pub image: PostImage }
pub enum PostImage { Tree(Vec<(Vec<u8>, Vec<u8>)>), Overlay(Digest) }   // (subject, post), ascending
impl Job { pub fn store_path(&self) -> &Path; }
pub trait Task { fn run(&mut self, job: &Job) -> Result<Shard, String>; }   // §5.1
pub enum ToWorker { Job(Box<Job>), Exit }
pub enum ToCoordinator { Ready { protocol: u64, tool: String },
                         Done { observer: Digest, shard: Digest, cpu_ns: u64, peak_rss: u64 },
                         Failed { observer: Digest, message: String } }
// encode(&self) -> Vec<u8>; decode(&[u8]) -> Result<Self, wire::Error>, for both
pub fn write_frame(w: &mut impl Write, payload: &[u8]) -> io::Result<()>;
pub fn read_frame(r: &mut impl Read, max: usize) -> io::Result<Option<Vec<u8>>>;
pub struct ServeOptions { pub tool: String, pub stack_bytes: usize, pub max_jobs: u64 }
pub fn serve(task: impl Task + Send, options: &ServeOptions, input: impl Read, output: impl Write) -> io::Result<()>;
pub fn cpu_ns_from_stat(text: &str) -> Option<u64>;
```

`slip_exec::pool` (the coordinator, §5.3–§7):

```rust
pub const DEFAULT_OBSERVER_TIMEOUT: Duration;                     // 1 h
pub struct WorkerCommand { pub program: PathBuf, pub args: Vec<OsString>, pub env_remove: Vec<OsString> }
pub struct Options { pub jobs: usize, pub admission: admission::Config, pub observer_timeout: Option<Duration>,
                     pub worker_jobs: u64, pub sample_interval: Duration, pub tool_slip: String, pub tool_kiln: String }
impl Options { pub fn new(jobs: usize, admission: admission::Config) -> Options; }   // the others at their defaults
pub struct Summary { pub run: RunRecord, pub run_path: PathBuf, pub statuses: [u64; 4], pub ends: [u64; 5],
                     pub outcomes: Vec<ObserverOutcome> }
pub struct ObserverOutcome { pub observer: Digest, pub status: Status, pub message: String }
pub enum PoolError { Store(StoreError), Io(io::Error), Cancelled }   // Display, Error, From<StoreError>, From<io::Error>
pub fn run(store: &Store, plan_id: &Digest, observers: &[Observer], phase: Phase, round: u64,
           worker: &WorkerCommand, options: &Options, tokens: &jobserver::Pool, sampler: &dyn process::Sampler)
           -> Result<Summary, PoolError>;
/// `run` with the two fields of protocol 2, the same for every job of the run. `run` is this with neither.
pub struct JobExtra { pub transform: Option<Digest>, pub post: Option<Post> }
pub fn run_with(store: &Store, plan_id: &Digest, observers: &[Observer], phase: Phase, round: u64, extra: &JobExtra,
                worker: &WorkerCommand, options: &Options, tokens: &jobserver::Pool, sampler: &dyn process::Sampler)
                -> Result<Summary, PoolError>;
pub fn cleanup_on_signal(store: &Store) -> impl FnOnce() + Send + 'static;   // §7.1
```

## 6. Memory admission

**Budget.** `B` is `--memory <size>` if given. Otherwise it is the smaller of
the cgroup memory limit of the process (cgroup v2 `memory.max`, when it is a
number) and the machine's total memory, minus a reserve of the larger of
2 GiB and one tenth. `--memory 0` disables admission and caps.

**Estimate.** Each job has an estimate `e`, in bytes:

1. the peak recorded for the same observer by the most recent run record in
   the store for this phase, times 1.25; else
2. `--estimate-base` (default 512 MiB) plus `--estimate-per-byte` (default
   64) times the main file's size.

**Reservation.** A running job reserves `r = max(e, its sampled resident
size)`. `R` is the sum of the reservations of running jobs.

**Admission.** A pending job is admitted if `R + e ≤ B`, or if nothing is
running. The scheduler takes the first pending job in plan order that is
admitted (so a smaller job may start while a larger one waits). A job that
has been passed over 64 times blocks all later jobs until it is admitted.

**Caps.** A job's resident-size cap is `--observer-memory` (default `B`); a
job whose sampled resident size exceeds it is killed (`killed_memory`). Its
address-space cap (§4) is twice the resident-size cap plus the worker stack
plus 1 GiB.

**Pressure.** If the sampled resident sizes of all running jobs sum to more
than `B` while more than one job is running, the job that started most
recently is killed and put back at the head of the queue with
`e = 2 ×` its last sampled size. This is not an attempt and not a failure;
it is recorded in the run record with `End = killed_memory` and no shard. A
job requeued this way three times runs alone from then on.

**Retry.** A job killed at its cap while other jobs were running is retried
once, alone (nothing else is admitted while it runs). Killed at its cap
alone, it fails.

All sampling is at the interval of §4. Decisions use sampled values only, so
a test can drive the scheduler with a fake sampler.

### 6.1 Admission rules

`slip_exec::admission` is pure: no clock, no processes, no I/O. It owns the
queue of pending jobs, so a requeue at the head is its own act.

- **Budget.** `budget_from_system` takes `m`, the smaller of the cgroup
  limit and the total (either alone when the other is unknown: `None`, or
  a total of 0) and returns `m − max(2 GiB, m / 10)`. When that reserve is
  not smaller than `m` (a machine of 2 GiB or less) the reserve is `m / 10`
  only. Unknown memory (`None`, 0) gives 0, which disables admission.
  `read_system` takes the smallest `memory.max` number along the process's
  cgroup v2 directory (`/proc/self/cgroup` `0::` line, under
  `/sys/fs/cgroup`) and its ancestors, and `MemTotal` of `/proc/meminfo`.
- **Estimate.** Rule 1 uses the largest `peak_rss` of any job of the
  observer in the most recent run record of the phase, any round
  (`Store::latest_run`); 0 counts as unknown and falls to rule 2. `× 1.25`
  is `p + p / 4`. A record that does not decode is ignored. All arithmetic
  saturates.
- **Pick.** With `B = 0`: the head of the queue (no admission, no caps, no
  decisions). Otherwise nothing while a running job runs alone; then the
  queue is scanned in order: a job that must run alone is admitted only
  when nothing runs, and until then it **blocks the jobs after it**; a job
  is admitted if nothing runs or `R + e ≤ B`; a job not admitted that has
  been passed over `SKIP_LIMIT` times blocks the jobs after it. When a job
  is started, every job before it in the queue is passed over once more.
- **Reservation.** A running job's sample is its last one (0 before the
  first); its reservation `max(e, sample)` lasts until `finished`, also
  while it is being killed.
- **Caps** (`on_samples`, `B > 0`): `rss > cap` (strictly; 0 never) is
  `KillAtCap`. `retry` is true when more than one job is running and the
  job has not had its cap retry. The retry is at the head of the queue,
  alone, once; a job retried for its cap and killed at its cap again (it
  then runs alone) fails.
- **Pressure** (after the caps of the same round): the jobs neither being
  killed nor `stopping` are summed; if the sum exceeds `B` and more than
  one of them runs, the one started last (start order, not plan order) is
  `KillForPressure`, at most one per round. Its new `e` is `2 ×` its last
  sample, or its old `e` when that sample is 0. It goes back at the head
  of the queue when it `finished`; from its third such requeue on it runs
  alone, for every later run (a crash retry included).
- **`finished(job, retry)`**: a decision taken by `on_samples` wins over
  `retry` (a job failed at its cap is never requeued); otherwise `retry`
  puts the job at the head of the queue. Unknown or not running: nothing.
- **`stopping(job)`**: the coordinator is ending the job for a reason of
  its own (a time limit, a protocol violation); it keeps its reservation
  until `finished` but is no longer decided on.

### 6.2 Interface

`slip_exec::admission`:

```rust
pub const GIB: u64; pub const DEFAULT_ESTIMATE_BASE: u64 /* 512 MiB */;
pub const DEFAULT_ESTIMATE_PER_BYTE: u64 /* 64 */; pub const DEFAULT_WORKER_STACK: u64 /* 1 GiB */;
pub const SKIP_LIMIT: u32 /* 64 */; pub const ALONE_AFTER_REQUEUES: u32 /* 3 */;
pub struct Config { pub budget: u64, pub estimate_base: u64, pub estimate_per_byte: u64,
                    pub observer_memory: Option<u64>, pub worker_stack: u64 }
impl Config { pub fn with_budget(budget: u64) -> Config; }        // the other fields at their defaults
pub fn budget_from_system(cgroup_max: Option<u64>, total: u64) -> u64;
pub fn read_system() -> (Option<u64>, u64);
pub fn meminfo_total(text: &str) -> Option<u64>;                  // parsers behind read_system
pub fn cgroup_memory_max(text: &str) -> Option<u64>;
pub fn cgroup_v2_path(text: &str) -> Option<&str>;
pub fn estimate<S: BuildHasher>(config: &Config, previous: &HashMap<Digest, u64, S>, observer: &Observer) -> u64;
pub type JobKey = usize;                                          // index into the observers given
pub enum Decision { KillAtCap { job: JobKey, retry: bool }, KillForPressure { job: JobKey, estimate: u64 } }
pub struct Pick { pub job: JobKey, pub estimate: u64, pub alone: bool }
pub struct Admission;
impl Admission {
    pub fn new<S: BuildHasher>(config: Config, previous: &HashMap<Digest, u64, S>, observers: &[Observer]) -> Admission;
    pub fn config(&self) -> &Config;
    pub fn estimate(&self, job: JobKey) -> u64;                   // current e
    pub fn rss_cap(&self) -> Option<u64>;  pub fn address_space_cap(&self) -> Option<u64>;
    pub fn reserved(&self) -> u64;                                // R
    pub fn pending(&self) -> impl Iterator<Item = JobKey>;  pub fn pending_len(&self) -> usize;
    pub fn running(&self) -> impl Iterator<Item = JobKey>;  pub fn running_len(&self) -> usize;
    pub fn skips(&self, job: JobKey) -> u32;  pub fn requeues(&self, job: JobKey) -> u32;
    pub fn runs_alone(&self, job: JobKey) -> bool;
    pub fn peek(&self) -> Option<JobKey>;                         // what pick would start; no change
    pub fn pick(&mut self) -> Option<Pick>;                       // selects AND marks running
    pub fn on_samples(&mut self, samples: &[(JobKey, u64)]) -> Vec<Decision>;
    pub fn finished(&mut self, job: JobKey, retry: bool);
    pub fn stopping(&mut self, job: JobKey);  pub fn is_killing(&self, job: JobKey) -> bool;
}
```

## 7. Signals

On `SIGINT` or `SIGTERM` the coordinator: kills every live child's process
group (§4); waits for them to be reaped (at most 10 s); returns every pool
token it holds (§3); writes the run record with the remaining jobs as
`cancelled` (best effort); removes its workers' `tmp/` entries (§2
"Cleaning"), its own `tmp/` files and the store lock; prints one line to
standard error; and ends by the signal's default action, so its parent sees
it killed by that signal. A second signal 0.5 s or more after the
first ends the process at once, after killing the children and returning the
tokens. The process ends at the latest 15 s after the first signal. No write
to standard error on these paths may panic or block the exit.

`SIGKILL` cannot be handled: tokens held are lost (restore with
`cargo xtask jobserver refill`) and the store keeps a stale lock and `tmp/`
entries (its own and its workers'), which the next run clears (§2).

A worker does not handle signals; it dies with its process group.

### 7.1 The signal path

- `signals::install` is idempotent: only the first call installs anything.
  Later calls drop their callback and return `false`. It also installs a
  panic hook: a thread other than the signal threads that panics during an
  interruption waits for the process to end.
- **Cleanup order** (cleanup thread): `kill_live_children(reap)`, then
  `jobserver::release_all()`, then the callback (a panic in it is caught),
  then the single line
  `slip: signal N: killed K child process(es), returned T jobserver token(s)`.
  `; the cleanup callback panicked` is appended if it did. The process then
  ends by the signal's default action, or by `_exit(128 + N)` if that cannot
  be emulated.
- **Bound or second signal.** Both kill every group without waiting and
  return the tokens. Their line is
  `slip: signal N: cleanup still running after <bound>s; exiting anyway`
  (`<bound>` in seconds with one decimal) or
  `slip: signal N: exiting without waiting for cleanup`, written from a
  thread the signal thread waits on for at most 100 ms.
- `finish(code)` returns `code` unless the process is interrupted. If it is,
  `finish` waits like `exit_interrupted`, which ends the process itself 5 s
  after the bound if the signal thread has not.
- **The coordinator.** `pool::run` is cancelled when
  `signals::interrupted()` is set or `pool::cleanup_on_signal`'s callback
  runs. It then does what its error path does (§5.6 "Errors") and returns
  `PoolError::Cancelled`: the running jobs and every pending one (attempt
  number of its next run, `queued_ns` the time of the cancellation,
  `wall_ns` 0) are `cancelled`, with no shard. A worker found dead after
  the interruption is never counted as a crash and no `failed` shard is
  written for it. Tokens are dropped with the coordinator; after
  `jobserver::release_all` that returns nothing.
- **`pool::cleanup_on_signal(store)`** is the callback for
  `signals::install` in a process that runs phases. Each run in progress
  registers the pids of the workers it started whose `tmp/` entries it has
  not removed yet. The callback tells every run in progress that it is
  cancelled, waits at most 3 s for each to publish its run record, removes
  the `tmp/` entries of every pid still registered (the signal path has
  killed every child by then), and then runs `store::cleanup_on_signal`
  (§2.1 "Closing"). A run that finishes its cancellation in time has
  emptied its registry, so the removal here matters only for a run that
  does not. Whichever of the coordinator and the signal path kills and
  reaps the workers first, the other finds nothing left, so the counts in
  the signal line vary; the fifo is whole either way.

### 7.2 Interface

`slip_exec::signals`:

```rust
pub struct Timing { pub reap: Duration, pub bound: Duration, pub duplicate: Duration }  // default 10 s, 15 s, 0.5 s
pub const SIGNAL_THREAD: &str; pub const CLEANUP_THREAD: &str;
pub fn install(on_cleanup: Box<dyn FnOnce() + Send>) -> bool;
pub fn install_with(on_cleanup: Box<dyn FnOnce() + Send>, timing: Timing) -> bool;
pub fn interrupted() -> Option<i32>;
pub fn finish(code: ExitCode) -> ExitCode;
pub fn exit_interrupted() -> !;
```

## 8. Commands

```
slip plan  --db [<name>=]<compile_commands.json>…  [--store <dir>] [-j <n>]
slip parse (--plan <id or file> | --db [<name>=]<compile_commands.json>…)
           [--store <dir>] [-j <n>] [--only <substring>]… [--limit <n>]
           [--memory <size>] [--observer-memory <size>] [--observer-timeout <seconds>]
           [--estimate-base <size>] [--estimate-per-byte <n>]
           [--worker-jobs <n>] [--worker-stack <size>]
slip verify --plan <id or file> [--round <n>] [--declared <file>]
           [--root <dir>]… [--post <dir>]… [--findings <n>] [--locate <n>] [--report <file>]
           [--store <dir>] [-j <n>] [--only <substring>]… [--limit <n>]
           [--memory <size>] [--observer-memory <size>] [--observer-timeout <seconds>]
           [--estimate-base <size>] [--estimate-per-byte <n>]
           [--worker-jobs <n>] [--worker-stack <size>]
slip declare --plan <id or file> --from <file> --out <file>
           [--root <dir>]… [--post <dir>]… [--store <dir>] [--only <substring>]… [--limit <n>]
slip units --plan <id or file> [--post-image] [--root <dir>]… [--post <dir>]…
           [--store <dir>] [--only <substring>]… [--limit <n>] [--match <substring>]…
slip map   (--plan <id or file> | --db [<name>=]<compile_commands.json>…)
           (--from <qualified name> --to <identifier> [--kind function|variable]
            | --transform <file.cpp> [--transform-arg <option>]… [--indivisible])
           --owned <path>… [--readonly <path>]…
           [the store, selection, memory, timeout and worker options of `parse`]
slip rename --from <qualified name> --to <identifier> [--kind function|variable]
           (--plan <id or file> | --db [<name>=]<compile_commands.json>…)
           --owned <path>… [--readonly <path>]…
           [--write | --patch <file>] [--relative-to <dir>] [--report <file>]
           [--allow-residue] [--allow-unparsed] [--allow-partial] [--allow-errors]
           [--residue-ext <list>] [--findings <n>] [--locate <n>]
           [the store, selection, memory, timeout and worker options of `parse`]
slip reduce --plan <id or file> --round <n> [--store <dir>] [-j <n>]
slip check --plan <id or file> --round <n> --edits <id or file> [--declared <id or file>]
           [--report <file>] [--allow-unparsed] [--residue-ext <list>] [--findings <n>] [--locate <n>]
           [the store, selection, memory, timeout and worker options of `parse`]
slip apply --result <id> (--write | --patch <file>) [--relative-to <dir>]
           [--allow-residue] [--allow-partial] [--allow-errors] [--store <dir>]
slip apply --recover [--store <dir>]
slip dump  <file>
slip resource --write <dir>
slip --version
slip --help
```

`plan`, `parse` and `dump` are M0's; `verify`, `declare` and `units` are
M1's (§8.7 to §8.9); `map`, `rename`, `reduce`, `check` and `apply` are
M2's (§8.11 to §8.14). The M2 commands run a transform. They follow every
rule of this section for options, numbers, sizes and usage errors, and
`map`, `rename` and `check` run their phases as `parse` and `verify` run
theirs (§8.2, §8.7); what they do is specified in `spec/edits.md` §10.

`slip --help` (or `-h`) prints the usage text on standard output and
exits with status 0. `slip --version` prints `slip <version> (kiln
<version>)`; the same string is the `tool` of the worker's `Ready`.

An option's value is the next argument, or follows `=` for a long option
(`--store=x`) and directly for `-j` (`-j4`). An option given twice is a
usage error unless it is marked `…` above. `--` ends the options. `<size>`
is a decimal number with an optional suffix `K`, `M`, `G`, `T` (powers of
1024). Paths and `--only` values are byte strings, not required to be
UTF-8.

- **Options** are those above, each command accepting only its own;
  `__worker` accepts the options of §8.4; `resource` accepts `--write`
  and requires it; `dump` accepts
  none (`dump -- <file>` reads a file whose name starts with `-`). An
  argument that starts with `-` (other than `-` and `--`) in option
  position is an option; an option's value is taken whatever it starts
  with. A positional argument is refused except `dump`'s one file. After
  `--` everything is positional. `-j=4` has the value `=4` (malformed).
- **Numbers** are decimal digits only (no sign, no white space, leading
  zeros allowed), at most `u64::MAX`. **Sizes** are a number and an
  optional upper-case suffix; a product above `u64::MAX` is malformed.
  Lower-case suffixes are malformed.
- **Ranges.** `-j`, `--worker-jobs`, `--worker-stack` and
  `--observer-memory` must be at least 1; `--limit`, `--memory`,
  `--observer-timeout`, `--estimate-base`, `--estimate-per-byte`,
  `--round`, `--findings` and `--locate` may be 0.
- **Images.** The k-th `--root` and the k-th `--post` are one root map
  (`spec/verify.md` §5.1); a different number of the two is a usage
  error. None of either is the identity image.
- **Usage errors** are `slip: <message>` and then the usage text, on
  standard error. No command, and `--help`, `-h` or `--version` with an
  argument, are usage errors.

Without `--db name=`, a single database is named `default`; several need
names. A `--db` value is split at its first `=` only if what precedes it is
a non-empty name of ASCII letters, digits, `_`, `-` and `.`; otherwise the
whole value is the path. With several `--db`, an unnamed one or a name
given twice is a usage error, reported before anything is read.

`main` passes a locked standard output and an *unlocked* standard error:
the signal thread, the jobserver and the pool write warnings to standard
error from other threads, and a lock held by the main thread would block
them (the signal path for up to its 15 s bound).

### 8.1 `plan`

Builds the plan (`spec/cdb.md` §7) with Kiln's front-end check, publishes it
to the store (opened shared) and prints, on standard output:

```
plan <plan id>
observers <n>
excluded <reason> <n>          one line per reason of spec/model.md §7, in its order, zeros included
```

Main files are hashed on as many threads as CPU tokens can be had without
waiting (§3: the implicit token, then pool tokens while one is idle), at
most `-j`: `Pool::from_env()` (with `-j` if given), then `try_acquire`
until it returns none or `jobs()` tokens are held; the plan is built on
that many threads and the tokens are dropped when the build returns,
before the plan is published.

### 8.2 `parse`

Runs the map phase with the map task (`spec/model.md` §8.5) on the
observers of a plan: each `ok` shard carries the observer's `digests`.

- `--plan <x>`: if `x` is 4 to 64 hexadecimal digits (either case; it is
  lower-cased) it is the prefix of a plan id in the store, which must
  match exactly one plan (none, or several: `Input` error naming them);
  otherwise `x` is the path of a plan file, which is read, validated and
  published to the store (`Store::publish_plan`, which validates it again).
- `--db …`: the plan is built and published as by `plan`, with the same
  output, and then parsed. Exactly one of `--plan` and `--db` is given.
- `--only` keeps the observers whose main file path contains the substring
  (any of them, if repeated); `--limit` then keeps the first `n` in plan
  order. If a non-empty plan is left with no observer, a warning is printed
  and the phase runs over nothing:
  `slip: warning: the selection keeps none of the plan's <n> observer(s); the phase runs over nothing`.

The store is opened exclusively (§2) before any database or plan is read,
and the signal handling is installed at once
(`signals::install(pool::cleanup_on_signal(store))`, §7.1), so an
interruption while the plan is built also removes the lock. The worker
command is the running executable (`std::env::current_exe`) with the
arguments `__worker`, `--worker-jobs <n>` and `--worker-stack <bytes>`
(both always passed), and with every variable of the coordinator's
environment whose name starts with `KILN_` removed. The round is 0. The
pool is given `-j` (`Pool::jobs`); the tools of the plan; `--worker-jobs`;
`--observer-timeout` (default `DEFAULT_OBSERVER_TIMEOUT`, 0: none); and
`admission::Config::with_budget(B)` with `--observer-memory`,
`--worker-stack` and the two estimate parameters when given. The sampler
is `ProcSampler`. When `B` comes from the system and is 0, a warning is
printed:
`slip: warning: the memory available is unknown; memory admission and caps are off (as with --memory 0)`.
On standard output, after the phase:

```
plan <plan id>                 (not repeated after --db)
selected <n>                   observers the phase ran over
shard <status> <n>             one line per Status of spec/model.md §8, in its order, zeros included
job <end> <n>                  one line per End of spec/model.md §9, in its order, zeros included
wall <seconds, three decimals>
peak_rss <bytes> <main file of the observer it belongs to>      (omitted if no job reported one)
run <path of the run record>
```

`wall` is the run record's `wall_ns`, truncated to milliseconds.
`peak_rss` is the largest `peak_rss` of the run record's jobs (the first
such job on a tie) and its observer's main file; it is omitted when every
job's is 0, or there is no job. Paths are written as their bytes.

After that summary, on standard error, one line per observer whose shard is
not `ok`, in plan order: `slip: <status>: <main file>: <first line of the
shard's message>`, from the pool's outcomes (`Summary::outcomes`, §5.6);
the shards are not read again.

`PoolError::Cancelled` returns through `signals::finish` (§7.1), printing
nothing more.

### 8.3 `dump`

Prints the JSON rendering of a container file (`spec/model.md` §10) on
standard output.

### 8.4 The worker

```
slip __worker [--worker-jobs <n>] [--worker-stack <size>] [--findings <n>] [--locate <n>]
```

is `serve` (§5.2) over standard input and output. It is not listed in the
usage text. Nothing but protocol messages may be written to its standard
output.

- `serve` runs with `tool` = the `--version` line, `stack_bytes` =
  `--worker-stack` and `max_jobs` = `--worker-jobs`, over unlocked
  standard input and output. A `serve` error is `slip __worker: <error>` on
  standard error and status 1.
- **The task** is chosen by the job's phase: `map` is the map task
  (`spec/model.md` §8.5), `verify` the verify task (`spec/verify.md` §5).
  A map job with a `transform` (§5.2) is the map task followed, in the
  same parse, by the transform (`spec/edits.md` §8.1): M2. Any task
  opens the job's store (`Job::store_path`, absolute) shared on its
  first job and keeps it for its lifetime; a job naming another store
  replaces it. None writes a file other than through the store, so
  none asks it for a scratch directory.
- **A round's parameters** reach the verify task in the job: `Job.post`
  (§5.2) names the round of the pre shards, the `Declared` and the
  image. A tree image's roots are taken as given (the coordinator made
  them real paths and checked the image). `--findings` (default 64) is
  `limit` and `--locate` (default 16) is `locate` of `spec/verify.md`
  §5; they are the same for every job a worker serves and are the
  worker's own options. The map task ignores them.
- **The verify task**, per job: reads `declared/<id>.decl` from the job's
  store and checks that it is a valid `DECL` whose content id is
  `post.declared`; makes the image from `post.image` (for the overlay
  form: reads `overlays/<id>.ovly` and checks it the same way,
  `spec/verify.md` §5.1); then does `spec/verify.md` §5.2. A job
  without `post`, a container that is absent or invalid or has another
  id, and an image that breaks `spec/verify.md` §5.1 are each `Err` (the
  worker answers `Failed`), naming which. The `Declared`, its
  `RuleTable` and an overlay are kept from one job to the next while
  the ids are the same.
- **A job that does not fit its phase** (a map job with a `post`, a
  verify job with a `transform`) is `Err`, naming the field.
- The front end runs under Kiln's stack guard (`spec/model.md` §8.4):
  nesting deeper than the stack ends in Kiln's fatal diagnostic, a
  `diagnosed` shard, instead of aborting the worker.
- The worker keeps Rust's default panic hook: its report goes to the
  attempt's log.

### 8.5 Exit status

| Status | Meaning |
|---|---|
| 0 | The command did what was asked and has nothing to report. For `parse`: every selected observer's shard has status `ok`. For `verify`: the run is verified. |
| 1 | Completed with findings. For `parse`: at least one shard is not `ok`. For `plan`: at least one entry was excluded for a reason other than `not_a_compile`. For `parse --db`: either. For `verify`: the run is not verified. For `declare`: a line of the text is in error. |
| 2 | Usage error (unknown command or option, missing or malformed argument, duplicate or missing configuration names). One line on standard error, then the usage text. |
| 3 | Input error: a database, plan, store or container that cannot be read or is invalid, an image that is refused, or a busy store. One line on standard error: `slip: <what>: <why>`. |
| 4 | For `verify`: the run is incomplete (`spec/verify.md` §7): nothing was found to differ, and not everything could be checked. For `rename` and `check`: the result's state is `incomplete` (`spec/edits.md` §8.5). |
| 70 | Internal error (a bug): a panic in the coordinator. One line on standard error, `slip: internal error: <message> (<file>:<line>)`, after every live child has been killed and every pool token returned. |
| killed by signal | Interrupted (§7). |

For `map`, `rename`, `reduce`, `check` and `apply` the meanings of
statuses 0, 1, 3 and 4 are spelled out in `spec/edits.md` §10.5; they are
instances of the rows above, and 2, 70 and a signal are as above.

- **Status 1 for `parse`** also when the pool's outcomes are not one per
  selected observer.
- **Input errors.** A store error names the store (the `Unrecognised` and
  `Io` errors already start with a path and are printed as they are); a
  database error is `slip: <path as given>: <detail>`
  (`PlanError::Database`); a plan or container file that cannot be read or
  is invalid is `slip: <path>: <error>`; `--plan` prefixes are
  `slip: plan <prefix>: no plan in the store <root> has an id starting with it`
  and `slip: plan <prefix>: <n> plans have an id starting with it: <ids>`.
  `pool::run` returning `PoolError::Store` or `PoolError::Io` is also
  status 3 (`slip: parse: <error>` for the latter).
- **Status 70.** `run` (every command but `__worker`) installs, once per
  process, a panic hook that prints nothing and records the first panic's
  `<message> (<file>:<line>)`. `run` wraps the command in `catch_unwind`;
  on a panic it calls `process::kill_live_children_now` and
  `jobserver::release_all`, then prints `slip: internal error: <that>` and
  returns 70. A panic on a thread that does not reach `run` (none is known)
  prints nothing. `signals::install`, which `parse` calls later, wraps this
  hook. The hidden `slip __panic` panics on purpose, for the test of this
  status.

### 8.6 Interface

`slip_cli`:

```rust
// lib.rs
pub mod args; pub mod commands; pub mod worker;
pub mod status { pub const OK: u8 = 0; FINDINGS = 1; USAGE = 2; INPUT = 3; INTERNAL = 70; }
pub fn version_line() -> String;                 // "slip <v> (kiln <v>)"
pub fn run(args: &[OsString], out: &mut dyn Write, err: &mut dyn Write) -> ExitCode;

// args.rs
pub const USAGE: &str; pub const DEFAULT_STORE: &str /* ".slip" */;
pub const DEFAULT_WORKER_JOBS: u64 /* 1 */; pub const DEFAULT_WORKER_STACK: u64 /* 1 GiB */;
pub enum Invocation { Version, Help, Plan(PlanArgs), Parse(ParseArgs), Dump { file: PathBuf },
                      Resource { write: PathBuf }, Worker(WorkerArgs), Panic }
pub struct Db { pub name: Option<String>, pub path: PathBuf }
pub struct PlanArgs { pub dbs: Vec<Db>, pub store: PathBuf, pub jobs: Option<usize> }
pub enum PlanSource { Plan(OsString), Db(Vec<Db>) }
pub struct ParseArgs { pub source: PlanSource, pub store: PathBuf, pub jobs: Option<usize>,
                       pub only: Vec<Vec<u8>>, pub limit: Option<u64>, pub memory: Option<u64>,
                       pub observer_memory: Option<u64>, pub observer_timeout: Option<u64>,
                       pub estimate_base: Option<u64>, pub estimate_per_byte: Option<u64>,
                       pub worker_jobs: u64, pub worker_stack: u64 }
pub struct WorkerArgs { pub worker_jobs: u64, pub worker_stack: u64 }
pub struct UsageError(pub String);
pub fn parse(args: &[OsString]) -> Result<Invocation, UsageError>;
pub fn parse_size(text: &str) -> Option<u64>;

// commands.rs
pub fn dispatch(invocation: Invocation, out: &mut dyn Write, err: &mut dyn Write) -> ExitCode;

// worker.rs
pub struct ParseTask;  impl ParseTask { pub fn new() -> ParseTask; }  impl Task for ParseTask;
pub fn main(args: &WorkerArgs) -> ExitCode;
```

The commands of M1 add (§8.7 to §8.9):

```rust
// lib.rs
pub mod declare; pub mod image; pub mod locate; pub mod units; pub mod verify_cmd; pub mod verify_job;
pub mod status { pub const INCOMPLETE: u8 = 4; }

// args.rs
pub const DEFAULT_FINDINGS: u64 /* 64 */; pub const DEFAULT_LOCATE: u64 /* 16 */;
pub enum Invocation { /* … */ Verify(VerifyArgs), Declare(DeclareArgs), Units(UnitsArgs) }
pub struct ImageArgs { pub roots: Vec<PathBuf>, pub posts: Vec<PathBuf> }       // equal lengths
pub struct RunArgs { /* the options `parse` and `verify` share: */
                     pub store: PathBuf, pub jobs: Option<usize>, pub only: Vec<Vec<u8>>, pub limit: Option<u64>,
                     pub memory: Option<u64>, pub observer_memory: Option<u64>, pub observer_timeout: Option<u64>,
                     pub estimate_base: Option<u64>, pub estimate_per_byte: Option<u64>,
                     pub worker_jobs: u64, pub worker_stack: u64 }
pub struct VerifyArgs { pub plan: OsString, pub round: u64, pub declared: Option<PathBuf>, pub image: ImageArgs,
                        pub findings: u64, pub locate: u64, pub report: Option<PathBuf>, pub run: RunArgs }
pub struct DeclareArgs { pub plan: OsString, pub from: PathBuf, pub out: PathBuf, pub image: ImageArgs,
                         pub store: PathBuf, pub only: Vec<Vec<u8>>, pub limit: Option<u64> }
pub struct UnitsArgs { pub plan: OsString, pub post_image: bool, pub image: ImageArgs, pub store: PathBuf,
                       pub only: Vec<Vec<u8>>, pub limit: Option<u64>, pub matches: Vec<Vec<u8>> }
pub struct WorkerArgs { pub worker_jobs: u64, pub worker_stack: u64, pub findings: u64, pub locate: u64 }

// image.rs — spec/verify.md §5.1
pub struct RootMap { pub subject: Vec<u8>, pub post: Vec<u8> }
pub struct Image { /* root maps, sorted */ }
pub enum ImageError { NotAbsolute(Vec<u8>), IsRoot, Nested { inner: Vec<u8>, outer: Vec<u8> }, Duplicate(Vec<u8>),
                      NotUtf8(Vec<u8>), EqualsInPost(Vec<u8>) }
impl Image {
    pub fn identity() -> Image;
    pub fn new(maps: Vec<RootMap>) -> Result<Image, ImageError>;     // the checks of §5.1 that need no file system
    pub fn from_args(args: &ImageArgs) -> Result<Image, String>;     // each directory made a real path, then `new`
    pub fn maps(&self) -> &[RootMap];
    pub fn is_identity(&self) -> bool;
    pub fn command(&self, command: &Command) -> Command;             // the post command
    pub fn to_subject(&self, real: &[u8]) -> Vec<u8>;                // the subject path of a real path
    pub fn under_subject(&self, real: &[u8]) -> bool;                // guard G1
    /// One parse of an observer's image with the walk's input: `Pre` is the observer's command and
    /// real paths, `Post` the post command and subject paths. `f` runs on the front end's thread when
    /// the unit has no error (`slip_digest::map::with_input`, whose `Err` is this function's). The
    /// shard is the parse task's, with the plan's observer id on either side.
    pub fn parse<R: Send>(&self, observer: &Observer, side: Side, phase: Phase, round: u64,
                          f: impl FnOnce(&slip_digest::Input<'_>) -> R + Send) -> Result<(Shard, Option<R>), String>;
}
pub enum Side { Pre, Post }

// worker.rs
pub struct MapTask;     impl Task for MapTask;                       // spec/model.md §8.5
pub struct PhaseTask;   impl Task for PhaseTask;                     // §8.4: by `job.phase`
// verify_job.rs
pub struct Params { pub declared: Declared, pub image: Image, pub limit: usize, pub locate: usize }
pub fn verify(store: &Store, job: &Job, params: &Params) -> Result<Shard, String>;   // spec/verify.md §5.2
// locate.rs — spec/verify.md §6.1 "Interface"
// declare.rs — spec/verify.md §2.3 "Interface", and the command:
pub fn main(args: &DeclareArgs, out: &mut dyn Write, err: &mut dyn Write) -> ExitCode;   // §8.8
// units.rs
pub fn main(args: &UnitsArgs, out: &mut dyn Write, err: &mut dyn Write) -> ExitCode;     // §8.9
// verify_cmd.rs
pub fn main(args: &VerifyArgs, out: &mut dyn Write, err: &mut dyn Write) -> ExitCode;    // §8.7
```

`ParseTask` is replaced by `MapTask`; the worker serves `PhaseTask`.
`ParseArgs` becomes `{ source: PlanSource, run: RunArgs }`.

`worker.rs` has one function that makes the `Params` of a verify job,
from `job.post`, the job's store and the worker's `findings` and
`locate`. `verify_job::verify` and everything under it take their
parameters from `Params` and read neither the job's `post` nor the
worker's arguments.

M2 adds (§8.11 to §8.14):

```rust
// lib.rs
pub mod transform_cmd; pub mod stages;
// args.rs — spec/edits.md §10
pub enum Invocation { /* … */ Map(MapArgs), Rename(RenameArgs), Reduce(ReduceArgs), Check(CheckArgs), Apply(ApplyArgs) }
// image.rs — spec/verify.md §5.1, the overlay form
pub type Blobs = std::sync::Arc<dyn Fn(&Digest) -> std::io::Result<Vec<u8>> + Send + Sync>;
impl Image {
    pub fn overlay(id: Digest, overlay: slip_model::Overlay, blobs: Blobs) -> Image;
    pub fn overlay_id(&self) -> Option<&Digest>;
    pub fn from_post(store: &Store, post: &PostImage) -> Result<Image, String>;   // a job's image; a tree goes through `new`
    pub fn to_post(&self) -> PostImage;                                           // what the coordinator puts in the job
}
// worker.rs
pub struct TransformTask;   impl Task for TransformTask;             // a map job with `transform`; PhaseTask dispatches to it
```

For an image of the overlay form `maps()` is empty, `is_identity()` is
false, `command` returns the command as it is, `to_subject` is the
identity and `under_subject` is false: no path is rewritten and guard G1
has nothing to refuse. `Image::parse` with `Side::Post` gives the front
end the overlay's bytes (`spec/verify.md` §5.1).

### 8.7 `verify`

Runs the verify phase (`spec/verify.md` §5) for one round over the
observers of a plan, and prints the run's report (`spec/verify.md` §7).

- `--plan` as for `parse` (§8.2). There is no `--db`: the plan is the one
  the map phase ran over.
- `--round <n>` (default 0) names the round. The pre shards are always
  those of phase `map`, round 0. A round that was run before is run again
  and its shards replaced, unless it is the round of a transform's run
  (§2), which is refused.
- `--declared <file>` is a `DECL` container (`slip declare` writes one);
  without it the declared changes are `Declared::none()`.
- `--root`, `--post`: the image (§8). Each directory is made a real path
  (`std::fs::canonicalize`); the image is then checked
  (`spec/verify.md` §5.1).
- `--findings`, `--locate`: `limit` and `locate` of `spec/verify.md` §5.
- `--report <file>`: the JSON report is written there (to a file beside
  it, then renamed).
- The other options are those of `parse`, with the same meaning.

In this order: the store is opened exclusively and the signal handling
installed, as for `parse`; the plan is resolved; the declared file is
read, validated and published (`Store::publish_declared`; the container
of `Declared::none()` when there is no `--declared`); the image is made;
the store is asked for the plan's latest map run (`Store::latest_run`);
the selection is made as for `parse`, with the same warning; the phase is
run (`pool::run`, phase `verify`); the report is made and printed.

Input errors (status 3), each `slip: verify: <message>` unless a form of
§8.5 applies:

| When | Message |
|---|---|
| the declared file cannot be read or is not a valid `DECL` | `slip: <path>: <error>` |
| a `--root` or `--post` cannot be made a real path | `image: <path as given>: <error>` |
| the image is refused | `image: <ImageError>` |
| the store has no run record of phase `map` for the plan | `plan <id> has no map run in the store <root>; run slip parse first` |
| the store has a transform run record of the plan and this round | `round <n> of plan <id> is the round of a transform's run; slip verify does not replace its shards` |

The worker command is that of `parse` followed by `--findings <n>` and
`--locate <n>`, both always passed. Every job of the run carries the same
`post` (§5.2): `pre` 0, the id of the published `Declared`, and the image
as a tree (`Image::to_post`).

**The report.** For each selected observer in plan order the command
reads its shard of phase `verify` and this round, decodes it, and takes
it as this run's only if its content id is the `shard` of the observer's
last `Job` entry in the run record the pool returned
(`spec/verify.md` §7). For an observer whose verify shard is `diagnosed`
or `rejected` it reads the map shard for its status. Each goes to
`slip_verify::run::Report::add`. Standard output and standard error are
those of `spec/verify.md` §7.1; `wall` and `peak_rss` are computed as for
`parse`.

Exit status: 0 for a verified run, 1 for one not verified, 4 for an
incomplete one (§8.5). A run over an empty selection is incomplete.

### 8.8 `declare`

Makes a `Declared` from the text of `spec/verify.md` §2.3 and writes it
as a `DECL` container to `--out`.

- `--plan`, `--only`, `--limit`, `--root`, `--post` as for `verify`. The
  selection must keep at least one observer and at most 64 (an input
  error otherwise, naming the number): every selected observer is parsed
  here, in this process, one after another.
- The store is opened shared; no shard is read or written.
- For each selected observer, in plan order: the pre-image is parsed and
  its unit records taken (`slip_digest::records`), with no rules; if the
  text has a `relocate` with a parent, or a `created` line, the
  post-image is parsed with no rules; if it has a `created` line, the
  post-image is parsed again with the rules the `relocate` lines gave.
  An image of an observer that does not parse `ok` contributes no record
  and is a warning: `slip: warning: declare: <main file>: the <pre|post>-image is <status>`.
- The records of all selected observers together are the sets of
  `spec/verify.md` §2.3. Records of one `KeyId` from several observers
  are one record.

Standard output:

```
declared <id of the container>
rules <rules_id> <n>
changed <n>
created <n>
removed <n>
```

An error of §2.3 is `slip: <text file>:<line>: <error>: <detail>` on
standard error, status 1, and nothing is written. An unreadable text
file, a text that is not UTF-8 and an unwritable `--out` are input
errors.

### 8.9 `units`

Prints the unit records of the selected observers, for a person writing a
declared file or reading a report.

- `--plan`, `--only`, `--limit`, `--root`, `--post` as for `declare`,
  with the same bound on the selection and the same in-process parses.
- Without `--post-image` the pre-image's records are printed; with it,
  the post-image's (no rules).
- `--match` keeps the records whose name contains the substring (any of
  them, if repeated).

One line per record, sorted by `(name, key)`, records of one `KeyId` from
several observers once:

```
<key, 16 hexadecimal digits> <kind> <declared|derived|tu> <flags> <name>
```

`<flags>` is `-`, or `i` for an indeterminate unit and `f` for a fallback
key (both: `if`). Status 0; an observer that does not parse is the
warning of §8.8.

### 8.10 `resource`

`slip resource --write <dir>` writes the resource files embedded in this
build (`spec/api.md` §2.1, `slip_script::resource`) under `<dir>`:
`<dir>/include/slip/meta.h` and `<dir>/include/slip/__intrinsics.h`, byte
for byte. Directories that do not exist are made. A file already there is
replaced: each is written to a temporary file beside it and renamed over
it, so a reader sees the old bytes or the new ones. Nothing else under
`<dir>` is touched. Nothing is printed on standard output; the status is 0.

- A directory that cannot be made or a file that cannot be written is
  status 3, `slip: <path>: <error>`, where `<path>` is the one that
  failed. Files written before the failure stay.
- `resource` without `--write`, `--write` twice, or a positional argument
  is a usage error.

The directory written is a resource directory: pointing
`$SLIP_RESOURCE_DIR` at it makes it the one the running Slip uses
(`spec/api.md` §2.1 rule 1).

### 8.11 `map` and `rename`

`slip map` runs the map phase of a transform over a plan, in a new round
(§2): each job carries the transform request's id (§5.2), and each `ok`
shard carries `digests` and `edits` (`spec/edits.md` §4). It builds the
request from its options, publishes it, and prints the request's id and
the round. `--from`/`--to` name the native rename (`spec/edits.md` §9);
`--transform` names a transform unit of `spec/api.md` (M3,
`docs/wp/WP-092.md`). Normative text: `spec/edits.md` §10.1.

`slip rename` is the whole run of a native rename: `map`, `reduce`,
`check` and, when asked and permitted, `apply` (`spec/edits.md` §8,
§10.1).

### 8.12 `reduce`

Reads the map shards of a transform's round and publishes the edit set
and its declared changes (`spec/edits.md` §5, §6, §10.2). It parses
nothing and starts no worker.

### 8.13 `check`

Takes an edit set of a round, builds the overlay (`spec/edits.md` §7.1),
runs the verify phase of that round with jobs whose `post` names the
round itself as `pre`, the edit set's declared changes and the overlay
(§5.2), scans for residue and publishes the result (`spec/edits.md` §8,
§10.2). Its verify phase is `verify`'s (§8.7) in everything but where
the image and the pre shards come from, and its report is
`spec/verify.md` §7.1 with the additions of §9 and §10 there.

### 8.14 `apply`

Writes the post-image of a published result to the working tree, or as a
patch; `--recover` finishes an interrupted write (`spec/edits.md` §7.2,
§7.3, §10.2).

## Tests

- `crates/slip_exec/tests/jobserver.rs` — §3, §3.1 against fifos made by
  the tests (never the host pool): `MAKEFLAGS` parsing, the defaults,
  fallback, `-j`, pool + implicit bound, a freed implicit token waking a
  waiter, release on drop, error and panic, `try_acquire`, channel
  requests, abandoned requests, `release_all` exactly once, `status` and
  `refill`.
- `crates/slip_exec/tests/process.rs` — §4, §4.1: the process group, a kill
  reaching grandchildren, kill reasons, timeout, the rss cap on fake
  samples, `/proc` sampling, the wrapper's transparency and the cap, a zero
  cap, no `sh`, and the table from spawn to reap. The unit tests of
  `crates/slip_exec/src/process.rs`: a shutdown that kills, reaps and
  refuses spawns, including a spawn that was in flight, and the immediate
  kill, both on a private table.
- `crates/slip_exec/tests/signals.rs` — §7, §7.1 against a re-executed probe
  holding the implicit token, a 4-token pool and 5 children, with a sixth
  job waiting: `SIGTERM` and `SIGINT`, a dead stderr reader, signals
  between spawns, a panicking callback, a second signal and the bound.
- `crates/slip_exec/tests/store.rs` — §2, §2.1: initialising absent and
  empty directories, refusing unrecognised and other-version ones, an
  absolute root, every path of the layout, validation on publish, atomic
  replacement under a concurrent reader, `find_plan`, `latest_run`, `Busy`
  against a re-executed lock holder and within one process, the takeover of
  a dead pid's lock with stale files and scratch directories removed, an
  unreadable lock, `Shared` neither locking nor cleaning,
  `remove_entries_of`, `Drop` and `cleanup_on_signal`. The lock holder's
  output is read on a thread and every wait for it, and for its exit, is
  bounded (1 min).
- `crates/slip_exec/tests/worker.rs` — §5.1, §5.2, §5.5: hand-written bytes
  and round trips of every message, rejections, frames, `serve` in process
  (validation of the task's shard, clipping, a panic ending the worker,
  `max_jobs`, `Exit`, bad input), and the fake worker as a child process
  for every behaviour of its table.
- `crates/slip_exec/tests/admission.rs` — §6, §6.1 with numbers only: the
  budget formula and its edge cases, the readings' parsers; both estimate
  rules; the reservation; admission when it fits (equality included), when
  nothing runs, backfill, the 64-skip guard; caps and their values; a cap
  kill alone (fails) and beside others (one alone retry, then fails; once
  only); pressure (newest by start order, `2 ×` the sample, head of the
  queue, not with one job, alone after three, caps first); `B = 0`;
  `finished` with retry and `stopping`.
- `crates/slip_exec/tests/pool.rs` — §5–§7 with `slip-fake-worker` under a
  fake sampler (real `/proc` values, or a constant): every row of §5.4
  (`ok`, `diagnosed`, `fail:`, `panic`, `exit:`, `abort`, `garbage`,
  `wrong-observer`, `no-shard`, `hang` with a 3 s timeout, `late-output`)
  with the retry, the two records, messages and logs; a `Ready` for another
  protocol, a silent exit, a missing program; `-j` and the implicit token
  against a test fifo (all tokens back, after failures too); plan order;
  `worker_jobs = 3`; cap alone, cap retry alone, pressure, `B = 0`, rule 1
  of the estimate across two runs, the address-space cap and `env_remove`
  seen in `/proc`; the workers' `tmp/` entries removed after a crash, a
  protocol violation, a timeout, a cap kill and a pressure kill
  (`s2_cleaning_*`); `SIGINT` and `SIGTERM` to a re-executed probe running
  the pool (dies by the signal, workers gone, fifo whole, lock and `tmp/`
  clean, a run record of six `cancelled` jobs, no logs left). Every run is
  bounded (2 min) and checks that the record validates, accounts for every
  attempt and that the outcomes match the shards (`check_run`).
- `crates/slip_exec/src/bin/slip-fake-worker.rs` — the test fixture behind
  `worker.rs` and `pool.rs`, not part of Slip: `serve` with tool
  `slip-fake-worker`, `--jobs <n>` (default 1), `--stack <bytes>` (default
  16 MiB) and `--scratch 0|1`. Its behaviours (`observer.command.argv[1]`)
  are documented at the top of the file; `abort` sends itself `SIGKILL`
  (through `kill`, so no core file is written); `garbage`,
  `wrong-observer` and `no-shard` exit 0 right after writing. With
  `--scratch 1`, each job first makes `tmp/<pid>.d/f` and a loose file
  `tmp/<pid>.fake.tmp`, names both on standard error, and removes the
  loose file only if the behaviour returns.
- `crates/slip_cli/tests/args.rs` — §8: the command line, parsed without
  starting a process; the options of `verify`, `declare`, `units` and of
  the worker, unpaired `--root` and `--post`.
- `crates/slip_cli/tests/cli.rs` — §8 through the executable, including
  each exit status of §8.5, and §8.10: the files written, a replaced
  file, an error naming its path.
- `crates/slip_exec/tests/store.rs` — also §2 `declared/`: publishing a
  valid and an invalid `DECL`, the name by content id, reading, a store
  initialised without the directory.
- `crates/slip_cli/tests/verify.rs` — §8.7 to §8.9 through the
  executable (`spec/verify.md` "Tests"): every input error of §8.7, the
  `post` of a job as a worker sees it, statuses 0, 1 and 4, a shard of another
  run not read as this run's, `declare` with each error of
  `spec/verify.md` §2.3 and its output, `units` with and without
  `--post-image` and `--match`.
- `crates/slip_cli/tests/image.rs`, `crates/slip_cli/tests/verify_job.rs`,
  `crates/slip_cli/tests/locate.rs`, `crates/slip_cli/tests/declare.rs` —
  `spec/verify.md` "Tests".
- `crates/slip_exec/tests/worker.rs` — §5.2, protocol 2: `Job` with
  neither field, with `transform`, with `post` of each image form; a
  `PostImage` tag that is not known; root maps out of order; a `Ready`
  of protocol 1 refused by the coordinator.
- §8.11 to §8.14 — `spec/edits.md` "Tests".

## Open questions

- A verify job's first estimate is rule 2 of §6, not the observer's peak
  in the map phase: rule 1 reads records of the same phase only. Reading
  the latest map record for a verify job that has no verify record is a
  small change to `admission`, proposed in `spec/verify.md`.
- `--findings` and `--locate` still travel on the worker's command line
  (§8.4), and no shard records them. A report states them
  (`spec/verify.md` §7.1); a shard read without its report does not say
  how many findings it was cut to, except by `total` against the length
  of `findings`.
- `pool::run_with` gives every job of a run the same `transform` and
  `post` (§5.7). Jobs of different rounds in one pool run (repair, M4)
  need them per observer.
- `declare` and `units` parse in the calling process (§8.8), outside the
  pool: no admission, no cap, no retry, one observer at a time. Enough
  for a handful of observers; a relocation whose names no small selection
  sees needs them run as a phase.
- Defaults of `--estimate-base` and `--estimate-per-byte` are guesses until
  M0 measures LLVM; so is `--worker-jobs 1`, which costs a process start and
  later a transform compilation per observer.
- Cache keys for shards (command, input digests, transform, tool version)
  replace the plan-id directory in M5.
- The wrapper of §4 applies the address-space cap after the child has
  started, so a child runs for a moment without it, and a sample taken
  then reads its limit as unlimited.
- Dropping a `Child` that was not reaped waits for it without a bound after
  `SIGKILL`; a child stuck in the kernel would hang its owner.
- Killing a process group starts a `kill` process, since std has no safe
  `kill(2)`. One process start per kill is acceptable at Slip's rates.
- A stale lock whose process id has been reused by an unrelated process
  reads as busy until the file is removed by hand.
- `parse` prints nothing while it runs. A progress line (observers done,
  running, memory reserved) belongs on standard error when it is a terminal;
  the pool needs a way to report it.
- Removal of a worker's `tmp/` entries (§2 "Cleaning") is best effort in
  three places: the signal path's own removal, for a run that misses its
  wait, has no test that isolates it; a worker whose stream thread fails
  to start is dropped without removal; a worker killed and not yet reaped
  when the run is abandoned can write one more entry after the removal.
  The next exclusive open removes what is left.
- Store corner cases: two openers initialising one empty directory at the
  same moment (one can be refused as unrecognised); entries under `tmp/`
  that the store did not name are never removed; without `/proc` nothing
  is ever stale; a worker given a store that has been removed creates a
  new one; two `Store`s of one process share a scratch directory.
- `jobserver::resolve_pool_size` accepts `0` and a padded number for
  `$SLIP_JOBS`; `cargo xtask jobserver` refuses both itself
  (`spec/harness.md` §5). One rule, in `slip_exec`.
- Nothing is `fsync`ed. A crash of the machine can leave a published file
  empty; the container trailer makes that detectable, not impossible.
- Whether the coordinator should lower its budget when other processes in
  the same cgroup use memory. Today the budget is static.
- Coordinator corner cases: a store or I/O error while a job is being
  started or finished leaves that job out of the `cancelled` run record; a
  worker that has exited but whose attempt is not finished still counts
  towards pressure with its last sample, so a crash in that window can be
  recorded as a pressure requeue; a `Done` not delivered within 1 s of the
  worker being reaped is a crash; the estimate's rule 1 reads the most
  recent run record even when that run was cancelled; idle workers kept
  for reuse hold memory that admission does not count; while a worker is
  ending the loop polls every 5 ms.
- A panic of the front end is caught by Kiln's entry and returned to the
  parse task (`spec/model.md` §8.4), so the job answers `Failed` and the
  worker goes on serving: the next observer runs in the same process, and
  the coordinator retries the failed one once, although a front-end panic
  on a given unit is deterministic. Answering "do not retry, and end this
  worker" needs a field in the protocol's `Failed` (§5.2); until then a
  panic costs one retry and no fresh process.
- A job that must run alone blocks the jobs behind it instead of letting
  them start around it. Simple and never over budget; costs throughput when
  a large job waits for the running ones to drain.
