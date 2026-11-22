# harness — `cargo xtask`

Status: draft (M0). Implemented by `xtask`. The harness is specified to the
same standard as the tool: a re-implementation of Slip needs a
re-implementable definition of "passes".

`xtask` is a workspace member run through the alias `cargo xtask`. It is
`#![forbid(unsafe_code)]`, links none of the crates that link Kiln, and
reaches `slip` only as a built executable. It may depend on `slip_model` and
`slip_exec`.

M0 defines the gate (§2), the lints (§3), the work-package table (§4), the
jobserver commands (§5) and performance records (§7). Transform tests (§6)
are outlined here and specified with M2.

## 1. Principles

1. **One definition of the gate.** `cargo xtask ci` is what "the gate" means
   locally and in any CI system; CI configuration only chooses slices.
2. **Lints run first** and need no build of anything but `xtask`.
3. **Nothing is skipped silently.** A test that cannot run for lack of a
   capability (a tool, a corpus) is reported as `SKIP` with the reason, and
   is an `ERROR` when the environment variable `SLIP_REQUIRE_ALL=1` is set.
4. **Tight gates on deterministic numbers, loose on time.** Counters are
   compared exactly; time and memory are recorded and reviewed as trends
   (§7).
5. **Shared machine.** Every process the harness starts that is not itself
   a jobserver client goes through `slip_exec::process` and holds a
   jobserver token while it runs (`spec/exec.md` §3, §4). `cargo` and `slip`
   are jobserver clients: they are started directly and take their own
   tokens through `MAKEFLAGS`.

## 2. The gate

```
cargo xtask ci [--quick] [--step <name>]…
```

Steps, in order. A failing step stops the gate unless `--keep-going`.

| Step | What | `--quick` |
|---|---|---|
| `lint` | §3 | yes |
| `fmt` | `cargo fmt --all --check` | yes |
| `clippy` | `cargo clippy --all-targets -- -D warnings` over the selected crates | yes |
| `test` | `cargo test` over the selected crates | yes |
| `kiln` | the same clippy and test over the crates that link Kiln | no |
| `transform` | transform tests (§6) | no |

The *selected crates* are the workspace's default members (the crates that
do not link Kiln). `--quick` is the implementer's bar before handback for
work in those crates; work in a crate that links Kiln runs the full gate.
`--step` runs only the named steps, in gate order.

Each step prints `==> <name>` when it starts and `<name>: ok (<seconds>s)` or
`<name>: FAILED` when it ends. The gate's exit status is 0 only if every
step that ran passed; the last line is `ci: ok` or `ci: FAILED (<steps>)`.

Three gating tiers use this command (`docs/design/06-engineering-plan.md`
§3): *commit* is `ci --quick`; *merge* is `ci` plus the corpus slice of §8;
*release candidate* adds full corpora, oracles and performance budgets, and
is specified with M5.

## 3. Lints

`cargo xtask lint` runs all of these and reports every violation, each as
`<path>[:<line>]: <lint>: <message>`. Exit status 1 if there is any.

| Lint | Rule |
|---|---|
| `kiln-boundary` | Only `crates/slip_kiln/Cargo.toml` names a `kiln_*` dependency. Only `slip_kiln`, `slip_digest`, `slip_script` and `slip_cli` depend on `slip_kiln`, directly or indirectly. |
| `forbid-unsafe` | Every crate root (`src/lib.rs`, `src/main.rs`, each file under `src/bin/`) of every workspace member contains `#![forbid(unsafe_code)]`. |
| `dependencies` | Every third-party crate in `Cargo.lock` is in the allow list in this spec (§3.1). |
| `spec-index` | `spec/README.md`'s index: every `spec/*.md` other than `README.md` has a row; every row whose status is not `not written` names an existing file; the status is `not written`, `draft` or `stable`. |
| `spec-shape` | Every `spec/*.md` other than `README.md` contains the headings `## Tests` and `## Open questions`. |
| `landed` | Every file under `spec/landed/` is `<spec>/WP-NNN.md` for an existing `spec/<spec>.md`, starts with `# WP-NNN — `, and `docs/wp/WP-NNN.md` exists. |
| `wp` | Every `docs/wp/WP-*.md` follows `docs/wp/README.md`: the title line with the same id as the file name; `Status`, `Milestone`, `Track`, `Needs`, `Size` lines with allowed values; `Needs` names existing files. |
| `local-paths` | No committed text file contains an absolute path into a home directory or this host's workspace (`/home/`, `/Users/`, `/root/`, `/Volumes/`). |
| `doc-links` | Every relative Markdown link in `docs/`, `spec/` and `README.md` points at an existing file. |

Lints read the files tracked by git (`git ls-files`), so build output and
worktrees are never scanned.

### 3.1 Allowed third-party crates

| Crate | Used by | Why |
|---|---|---|
| `signal-hook` and its dependencies `libc`, `signal-hook-registry` | `slip_exec` | Returning jobserver tokens and killing children on `SIGINT`/`SIGTERM` (`spec/exec.md` §7). The handler's `unsafe` stays inside the dependency. |

Kiln's own dependencies (reached through the `kiln` submodule's path
dependencies) are allowed as Kiln's lock file lists them.

The list above is repeated in `xtask/src/lint/crates.rs`; a change is made
in both places.

## 4. Work packages

`cargo xtask wp` prints the work packages in `docs/wp/` as a table: id,
status, milestone, track, size, needs, title; ordered by id.
`cargo xtask wp --ready` lists those with status `todo` whose needs are all
`done`.

## 5. Jobserver

```
cargo xtask jobserver status [--fifo <path>] [--jobs <n>]
cargo xtask jobserver refill [--fifo <path>] [--jobs <n>]
```

As `spec/exec.md` §3 "Inspection". The fifo is `--fifo`, else the one named
by `MAKEFLAGS`, else `$SLIP_JOBSERVER_FIFO`, else
`/run/slip/jobserver.fifo`; `<n>` is `--jobs`, else `$SLIP_JOBS`, else 8.
Neither command blocks.

- `status` prints `jobserver: <fifo>: <idle> of <n> tokens idle` and exits
  0. It changes nothing.
- `refill` tops the pool up to `<n>` idle tokens and prints
  `jobserver: <fifo>: <idle> idle, <added> added`. It is only correct while
  no client holds a token; the command cannot know that and says so in its
  usage text.

A fifo that does not exist, is not a fifo or cannot be opened, a `<n>` that
is not a positive integer, or an unknown option: `xtask jobserver:
<message>` on standard error, exit 1 (2 for the usage errors).

## 6. Transform tests (outline; specified with M2)

A transform test is a directory: an input tree, compile commands, a
transform, and the expected result (post-image, report, exit status) as
golden files. Outcomes follow Kiln's model: `PASS`, `FAIL`, `XFAIL`
(`xfail: <reason>` required), `XPASS` (fails the gate), `SKIP`, `ERROR`,
`KILLED` (never masked by `xfail`). Unknown directives are errors. Every test
names the spec section it covers.

**Golden files.** A golden is the JSON rendering (`spec/model.md` §10) or the
exact bytes of an output. `--bless` rewrites goldens and prints each path it
changed; it never runs in the gate.

## 7. Performance records

A parallel phase writes a run record (`spec/model.md` §9) into its store
(`spec/exec.md` §2). The commands here take a run record **by its path in
the store**, `<store>/runs/<plan id>/<phase>.<round>.<started>.run`: the
store is the directory three levels up, the round is read from the file
name, and the plan and the shards of the run are read from that store
(shared access). A path of another shape, or a record whose plan or phase
disagrees with its path, is an error.

```
cargo xtask perf summarize <run record>…
cargo xtask perf record <corpus> <run record>
cargo xtask perf check  <corpus> <run record>
```

Errors (an unreadable or invalid file, an unknown corpus, a usage error)
print `xtask perf: <message>` and exit 1 (2 for usage).

### 7.1 What is computed

From the record, its plan and its shards:

- **Observers**: the distinct observers among the record's jobs. An
  observer's **last job** is its job with the greatest `attempt`.
- **Counters**, all exact and deterministic for a given corpus, tool and
  host headers:

  | Name | Value |
  |---|---|
  | `plan.observers` | observers in the plan |
  | `plan.excluded.<reason>` | excluded entries per reason, every reason of `spec/model.md` §7 present, zeros included |
  | `run.observers` | observers in the record |
  | `shard.<status>` | the record's observers by the status of their shard in the store, the four statuses present, zeros included |
  | every shard counter name | the sum of that counter over those shards |

  An observer of the record with no shard in the store, or a shard that
  does not validate or names another observer, phase or round, is an error:
  the record is of a cancelled run, or the store is not the run's.
- **Measured**, none deterministic: `wall_s` (the run's `wall_ns`), `cpu_s`
  (the sum of `cpu_ns` over every job), `peak_rss_max` and the percentiles
  `peak_rss_p50`, `peak_rss_p90`, `peak_rss_p99` (over the jobs that ended
  `completed` with a non-zero `peak_rss`; nearest rank: the value at
  position `ceil(p × n / 100)` of the ascending list; 0 when there is no
  such job), `peak_rss_sum`, `cores`, `workers_max`, `memory_budget`
  (those four as in the record). Seconds are printed with three decimals;
  sizes are whole bytes.

### 7.2 `summarize`

For each record, in the order given, separated by an empty line:

```
run <path as given>
plan <plan id>
phase <phase> round <round>
tool slip <version> kiln <version>
observers <run.observers> of <plan.observers>
shard <status> <n>                    four lines, in the order of spec/model.md §8
job <end> <n>                         five lines, in the order of spec/model.md §9; every job counted
wall_s <s>
cpu_s <s>
efficiency <cpu_s / (wall_s × workers_max), two decimals; `-` if the divisor is 0>
workers_max <n> cores <n> memory_budget <bytes>
peak_rss max <bytes> p50 <bytes> p90 <bytes> p99 <bytes> sum <bytes>
counter <name> <value>                the shard counters, by name
top cpu
  <cpu_s> <main file>                 the ten observers with the greatest summed cpu_ns, greatest first
top peak_rss
  <bytes> <main file>                 the ten observers with the greatest peak_rss of any job, greatest first
```

Ties in the two lists are broken by observer id. `<main file>` is the
observer's main file path from the plan. With a cancelled run (an observer
without a shard) `summarize` still prints everything the record alone
gives, the `shard` lines counting what is present, and a line
`missing shards <n>` after them.

### 7.3 Baselines

A baseline is `bench/baselines/<corpus>.json`, written by `perf record` in
the pretty JSON form (`spec/model.md` §4), members in this order:

```json
{
  "corpus": "llvm",
  "pin": "llvmorg-19.1.7",
  "slip": "0.0.0",
  "kiln": "1.3.0",
  "counters": { "inputs.bytes": 0, "inputs.files": 0, "plan.excluded.missing_file": 0, "plan.observers": 0, "run.observers": 0, "shard.ok": 0 },
  "measured": { "wall_s": 0.0, "cpu_s": 0.0, "peak_rss_max": 0, "peak_rss_p50": 0, "peak_rss_p90": 0, "peak_rss_p99": 0,
                "peak_rss_sum": 0, "cores": 0, "workers_max": 0, "memory_budget": 0 }
}
```

`corpus` is the row name of `bench/corpora.toml` and `pin` its tag (§8);
`slip` and `kiln` are the record's tool versions; `counters` is every
counter of §7.1 sorted by name. A baseline contains no absolute path and
no time of day.

- `perf record <corpus> <run record>` refuses a record that does not cover
  the plan (`run.observers` ≠ `plan.observers`), then writes the baseline,
  replacing one that exists, and prints its path. A baseline update changes
  `counters` only together with a stated reason in the commit message.
- `perf check <corpus> <run record>` compares against the committed
  baseline. `pin` and `counters` are compared **exactly**: a different pin,
  and each counter that is missing, extra or different (`<name>: baseline
  <a>, run <b>`), is printed and the command exits 1. `measured` is
  informational: each value is printed with its ratio to the baseline
  (`wall_s 12.300 (×1.04)`) and never fails the check. The tool versions
  are printed when they differ and do not fail it. With no difference the
  last line is `perf check: <corpus>: ok` and the exit status is 0.

## 8. Corpora (outline; specified by the M0 corpus work package)

Corpora live under `.corpus/` (git-ignored): pinned sources, their build
trees with `compile_commands.json`, and the stores of runs over them. The
pins are Kiln's (`kiln/scripts/oss/projects.toml`) so both projects measure
the same code: LLVM 19.1.7 as C++17 first.

## Tests

- `xtask/tests/lint.rs` — §3: for each lint, a fixture tree that passes and
  one that fails with the expected message.
- `xtask/tests/ci.rs` — §2: step selection, order, output lines, exit status,
  with fake `cargo`.
- `xtask/tests/wp.rs` — §4.
- `xtask/tests/jobserver.rs` — §5: over fifos the test makes.
- `xtask/tests/perf.rs` — §7: over stores the test writes (plan, shards,
  run record built with `slip_model`), with golden output.

## Open questions

- The run record does not carry its round; §7 reads it from the file name.
  Add `round` to `Run` (`spec/model.md` §9) when the M0 specs are folded.
- The `inputs.*` counters include system headers, so a baseline is exact
  only for one set of installed headers (the host compiler's and the
  standard library's). The baseline does not record which; a change of the
  container image shows up as a counter difference with no stated cause.
- `measured` has no gate. A regression budget (Kiln fails at a stated
  percentage on dedicated hardware) needs a quiet host; ours is shared.
- Hosted CI (GitHub Actions) for the Slip repository: not set up; the gate
  runs on the development host for now.
- `forbid-unsafe` looks for crate roots in the standard places only
  (`src/lib.rs`, `src/main.rs`, `src/bin/`). No member sets a `[lib]` or
  `[[bin]]` `path` elsewhere today; if one must, the lint has to follow it.
- Whether Slip and Kiln should share harness code as one crate (request KM2
  in `docs/design/05-kiln-interface.md`). Until then Slip ports what it
  needs, with attribution.
- The corpus lock is a file holding a process id, which only excludes within
  one container (`flock` does not exclude on the bind-mounted workspace).
  Corpora shared between containers need something else.
- `corpus configure` does not report a generator target that the row's
  `generate` list fails to reach; a pin change can leave the list short and
  only a failed parse would show it.
