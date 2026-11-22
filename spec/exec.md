# exec — running phases: store, jobserver, workers, memory admission

Status: draft (M0). Implemented by `slip_exec` (machinery; does not link
Kiln) and `slip_cli` (wiring). M0 has two phases: **plan** (`spec/cdb.md`)
and **parse**, which is the map phase with a task that only runs the front
end. Reduce, apply and verify are added to this spec by their milestones.

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
  shards/<plan id>/<phase>.<round>/<observer id>.shard
  runs/<plan id>/<phase>.<round>.<started_unix_ns>.run
  logs/<plan id>/<phase>.<round>/<observer id>.<attempt>.log     worker stderr, kept only for failed attempts
  tmp/                                      files being written
```

Ids are 64 hexadecimal digits (`spec/model.md` §3); `<phase>` is `map` or
`verify`; `<round>` is decimal, 0 for the first.

- **Publishing.** A file is written under `tmp/` with a name unique to the
  writing process and moved to its final name with one rename. A reader never
  sees a partial file. If the final name exists, the rename replaces it.
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
  every exit path it controls. Stale entries (from a killed process) are
  removed when the store is opened exclusively.

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
  a later fifo I/O error: a warning is printed once, acquisition becomes
  free, and `-j` alone bounds parallelism.
- **Inspection.** `status(fifo)` counts idle tokens without blocking (read
  until the fifo would block, write the same bytes back); `refill(fifo, n)`
  tops the pool up to `n` idle tokens and is only correct while no client
  holds tokens.

A token is acquired by a helper thread and handed to the scheduler over a
channel, so the scheduler never blocks in a read and can stop waiting when a
job finishes (the implicit token becoming free) or the run is cancelled. A
byte read after the scheduler stopped wanting it is written straight back.

## 4. Child processes

`slip_exec::process` starts and supervises children without unsafe code:

- **Own process group.** Every child is the leader of a new process group, so
  the whole tree under it can be signalled together.
- **Address-space cap.** A child given a cap is started through
  `sh -c 'ulimit -v <KiB> || exit 125; ulimit -c 0; exec "$@"' slip-mem-limit <program> <args…>`,
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

`Job` carries the plan's `Observer` record, the phase and the round. `Shard`
is the model type of `spec/model.md` §8. `slip_exec::worker::serve(task)` is
the worker's main loop.

### 5.2 Protocol

Coordinator and worker talk over the worker's standard input and output.
Each message is a `u32le` length followed by that many bytes, which are one
record in the encoding of `spec/model.md` §2. The worker's standard error is
captured to the attempt's log file.

| Direction | Message | Fields |
|---|---|---|
| to worker | `Job = 1` | `store: bytes` (path), `plan: digest`, `phase`, `round: uv`, `observer: Observer` |
| to worker | `Exit = 2` | — |
| to coordinator | `Ready = 1` | `protocol: uv` (1), `tool: str` |
| to coordinator | `Done = 2` | `observer: digest`, `shard: digest` (content id), `cpu_ns: uv`, `peak_rss: uv` |
| to coordinator | `Failed = 3` | `observer: digest`, `message: str` |

Each message starts with its tag as a `uv`.

The worker sends `Ready` once at start. For each `Job` it runs the task on a
thread with a large stack (`--worker-stack`, default 1 GiB reserved), catches
a panic of that thread, publishes the shard to the store (§2) and answers
`Done`, or answers `Failed`. Before `Done` it reads its own CPU time and peak
resident size from `/proc/self` (0 if unavailable). On `Exit` or end of
input it exits with status 0.

A worker runs at most `--worker-jobs` jobs (default 1) and is then replaced
by a new process. With the default every observer gets a fresh process, so
`peak_rss` is that observer's alone. With a larger value the worker resets
its peak counter between jobs where the system allows it
(`/proc/self/clear_refs`); where it does not, `peak_rss` is reported as 0.

Anything else is a **protocol violation** and is treated as a crash: a
message that does not decode, a `Done` for another observer, a `Done` whose
shard is not in the store or does not open as a `SHRD` for that observer and
phase, output after `Exit`.

### 5.3 Scheduling

Pending jobs are in plan order (largest first). A job is started when all of
these hold: the number running is below `-j`; a token is held (§3); and
admission accepts it (§6).

### 5.4 What every observer ends with

| What happened | `End` (run record) | Shard |
|---|---|---|
| The worker answered `Done` | `completed` | the worker's |
| The worker answered `Failed`, exited, was killed by a signal, or violated the protocol | `crashed` | after the retry (below), a shard with status `failed` written by the coordinator; `message` is the worker's message or the last 4096 bytes of its log |
| Killed for exceeding its memory cap | `killed_memory` | as above, after the retries of §6 |
| Killed for exceeding `--observer-timeout` (default 3600 s; 0 disables) | `killed_time` | `failed`, no retry |
| The run was interrupted | `cancelled` | none |

A crashed job is retried once. Every attempt is a `Job` entry in the run
record, with its attempt number.

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

## 7. Signals

On `SIGINT` or `SIGTERM` the coordinator: kills every live child's process
group (§4); waits for them to be reaped (at most 10 s); returns every pool
token it holds (§3); removes its `tmp/` files and the store lock; writes the
run record with the remaining jobs as `cancelled` (best effort); prints one
line to standard error; and ends by the signal's default action, so its
parent sees it killed by that signal. A second signal 0.5 s or more after the
first ends the process at once, after killing the children and returning the
tokens. The process ends at the latest 15 s after the first signal. No write
to standard error on these paths may panic or block the exit.

`SIGKILL` cannot be handled: tokens held are lost (restore with
`cargo xtask jobserver refill`) and the store keeps a stale lock and `tmp/`
entries, which the next run clears (§2).

A worker does not handle signals; it dies with its process group.

## 8. Commands (M0)

```
slip plan  --db [<name>=]<compile_commands.json>…  [--store <dir>]
slip parse [--plan <id or file> | --db …]  [--store <dir>] [-j <n>] [--memory <size>]
           [--observer-memory <size>] [--observer-timeout <seconds>]
           [--only <substring>]… [--limit <n>] [--worker-jobs <n>]
slip dump  <file>
slip --version
```

- `plan` builds the plan, publishes it, and prints its id and a summary: the
  number of observers, and of exclusions per reason.
- `parse` runs the map phase with the parse task on every observer of the
  plan (`--only` keeps observers whose main file path contains the
  substring; `--limit` keeps the first `n` in plan order), and prints a
  summary: observers per shard status, jobs per `End`, wall time, the largest
  `peak_rss` and the observer it belongs to.
- `dump` prints the JSON rendering of a container file (`spec/model.md` §10).
- `<size>` is a decimal number with an optional suffix `K`, `M`, `G`, `T`
  (powers of 1024).

Without `--db name=`, a single database is named `default`; several need
names.

### 8.1 Exit status

| Status | Meaning |
|---|---|
| 0 | The command did what was asked and has nothing to report. For `parse`: every observer's shard has status `ok`. |
| 1 | Completed with findings. For `parse`: at least one shard is not `ok`. For `plan`: at least one entry was excluded for a reason other than `not_a_compile`. |
| 2 | Usage error (unknown option, missing argument). |
| 3 | Input error: a database, plan, store or container that cannot be read or is invalid. |
| 70 | Internal error (a bug): a panic in the coordinator. |
| killed by signal | Interrupted (§7). |

## Tests

- `crates/slip_exec/tests/jobserver.rs` — §3 against a fifo made by the
  test: the implicit token, release on every path, fallback, `status` and
  `refill`.
- `crates/slip_exec/tests/process.rs` — §4: process groups, caps, kill
  reasons, the table.
- `crates/slip_exec/tests/signals.rs` — §7 at the level of `signals`:
  children killed, tokens returned, the second signal, the bound.
- `crates/slip_exec/tests/store.rs` — §2.
- `crates/slip_exec/tests/pool.rs` — §5–§7 with a fake worker program whose
  behaviour is chosen per job (succeed, allocate, crash, hang, violate the
  protocol) and a fake sampler: every row of §5.4, every rule of §6, signals.
- `crates/slip_cli/tests/cli.rs` — §8, including each exit status.

## Open questions

- Defaults of `--estimate-base` and `--estimate-per-byte` are guesses until
  M0 measures LLVM; so is `--worker-jobs 1`, which costs a process start and
  later a transform compilation per observer.
- Cache keys for shards (command, input digests, transform, tool version)
  replace the plan-id directory in M5.
- Dropping a `Child` that was not reaped waits for it without a bound after
  `SIGKILL`; a child stuck in the kernel would hang its owner.
- Killing a process group starts a `kill` process, since std has no safe
  `kill(2)`. One process start per kill is acceptable at Slip's rates.
- A stale lock whose process id has been reused by an unrelated process
  reads as busy until the file is removed by hand.
- Whether the coordinator should lower its budget when other processes in
  the same cgroup use memory. Today the budget is static.
