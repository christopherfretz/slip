# harness — `cargo xtask`

Status: draft (M0). Implemented by `xtask`. The harness is specified to the
same standard as the tool: a re-implementation of Slip needs a
re-implementable definition of "passes".

`xtask` is a workspace member run through the alias `cargo xtask`. It is
`#![forbid(unsafe_code)]`, links none of the crates that link Kiln, and
reaches `slip` only as a built executable. Its only dependencies are the
workspace crates `slip_model` and `slip_exec` (path dependencies through
`[workspace.dependencies]`), and through `slip_exec` `signal-hook`, which
§3.1 allows.

M0 defines the gate (§2), the lints (§3), the work-package table (§4), the
jobserver commands (§5), performance records (§7) and corpora (§8).
Transform tests (§6) are outlined here and specified with M2.

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
5. **Shared machine.** The harness starts its commands with
   `std::process::Command`, not through `slip_exec::process`, and takes no
   jobserver token for them. `cargo` and `make` are jobserver clients: they
   inherit the environment and take their own tokens through `MAKEFLAGS`.
   `git` and `cmake` (§8) run without a token ("Open questions").

## 2. The gate

```
cargo xtask ci [--quick] [--step <name>]… [--keep-going]
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
`--step` runs only the named steps, in gate order; it may repeat, and a
step named twice runs once. With `--quick` and `--step` together the named
steps run and each must be a quick step: `--quick --step kiln` is a usage
error. An unknown step, a `--step` without a name or an unknown option is a
usage error: `xtask ci: <message>` and the usage text on standard error,
exit status 2, nothing runs.

- `lint` runs in-process over the repository root and prints its
  violations (§3), or `lint: <error>` when the files cannot be listed.
- `fmt`, `clippy` and `test` run the commands of the table without `-p`,
  so cargo takes the default members.
- `kiln` runs the same clippy and test with
  `-p slip_kiln -p slip_digest -p slip_script -p slip_cli`, the test only
  if clippy passed.
- `transform` prints
  `transform: no transform tests yet (spec/harness.md §6)` and passes.

Cargo is `$CARGO` if set and non-empty, else `cargo` on `PATH`, run in the
repository root with the inherited environment and output. A cargo that
cannot be started fails the step with
``cannot run `<cargo> <args>`: <error>``.

Each step prints `==> <name>` when it starts and `<name>: ok (<seconds>s)`
(seconds with one decimal, `fmt: ok (8.4s)`) or `<name>: FAILED` when it
ends. The gate's exit status is 0 only if every step that ran passed, else
1; the last line is `ci: ok` or `ci: FAILED (<steps>)`, the failed steps
joined by `, ` in gate order.

Three gating tiers use this command (`docs/design/06-engineering-plan.md`
§3): *commit* is `ci --quick`; *merge* is `ci` plus the corpus slice of §8;
*release candidate* adds full corpora, oracles and performance budgets, and
is specified with M5.

## 3. Lints

`cargo xtask lint` runs all of these over the repository root (`cargo
xtask` uses the checkout it was built from) and reports every violation on
standard output, one per line, as `<path>[:<line>]: <lint>: <message>`:
lints in the order of the table below and, within a lint, by path then
line. Nothing is printed when there is none. With any, standard error gets
`xtask lint: <n> violation(s)` and the exit status is 1. An argument after
`lint` is a usage error, exit status 2.

| Lint | Rule |
|---|---|
| `kiln-boundary` | Only `crates/slip_kiln/Cargo.toml` names a `kiln_*` dependency. Only `slip_kiln`, `slip_digest`, `slip_script` and `slip_cli` depend on `slip_kiln`, directly or indirectly. |
| `forbid-unsafe` | Every crate root (`src/lib.rs`, `src/main.rs`, each `.rs` file under `src/bin/`) of every workspace member contains `#![forbid(unsafe_code)]`. |
| `dependencies` | Every third-party crate in `Cargo.lock` is in the allow list in this spec (§3.1). |
| `spec-index` | `spec/README.md`'s index: every `spec/*.md` other than `README.md` has a row; every row whose status is not `not written` names an existing file; the status is `not written`, `draft` or `stable`. |
| `spec-shape` | Every `spec/*.md` other than `README.md` contains the headings `## Tests` and `## Open questions`. |
| `landed` | Every file under `spec/landed/` is `<spec>/WP-NNN.md` for an existing `spec/<spec>.md`, starts with `# WP-NNN — `, and `docs/wp/WP-NNN.md` exists. |
| `wp` | Every `docs/wp/WP-*.md` follows `docs/wp/README.md`: the title line with the same id as the file name; `Status`, `Milestone`, `Track`, `Needs`, `Size` lines with allowed values; `Needs` names existing files. |
| `local-paths` | No committed text file contains an absolute path into a home directory or this host's workspace (`/home/`, `/Users/`, `/root/`, `/Volumes/`). |
| `doc-links` | Every relative Markdown link in `docs/`, `spec/` and `README.md` points at an existing file. |

Lints read the files tracked by git, so build output and worktrees are
never scanned. If the root holds `.git` (a directory, or a file as in a
worktree), the file list is `git ls-files --stage`; gitlink entries
(submodules such as `kiln`) are not files and their contents are not
listed. Otherwise (a fixture tree) the tree is walked, skipping only
`.git`; symbolic links are listed as files and not followed. Failing to
list files is an error: exit status 1 with `xtask lint: <reason>`.

### 3.1 Allowed third-party crates

| Crate | Used by | Why |
|---|---|---|
| `signal-hook` and its dependencies `libc`, `signal-hook-registry` | `slip_exec` (and `xtask` through it) | Returning jobserver tokens and killing children on `SIGINT`/`SIGTERM` (`spec/exec.md` §7). The handler's `unsafe` stays inside the dependency. |

Kiln's own dependencies (reached through the `kiln` submodule's path
dependencies) are allowed as Kiln's lock file lists them.

The list above is repeated in `xtask/src/lint/crates.rs`; a change is made
in both places.

### 3.2 Lint rules

- **Workspace members** are the directories with a listed `Cargo.toml` that
  match the root manifest's `[workspace] members` and no `exclude` pattern
  (`*` matches within one path component), plus the root if its manifest has
  a `[package]`.
- **`kiln-boundary`.** Dependencies are the keys of `[dependencies]`,
  `[dev-dependencies]`, `[build-dependencies]` and those tables under
  `[target.<cfg>]`, in both the `name = …` / `name.workspace = true` and the
  `[dependencies.name]` forms; a `package = "…"` rename names the real
  crate. The root's `[workspace.dependencies]` declares crates and is not a
  dependency, so the root may list `kiln_*` there. "Directly or indirectly"
  follows dependencies of every kind between members, so a dev-dependency
  path to `slip_kiln` counts. Messages:
  ``names Kiln crate `<crate>`; only crates/slip_kiln/Cargo.toml may`` at the
  dependency's line, and
  `` `<crate>` depends on `slip_kiln` (<a> -> … -> slip_kiln); only
  slip_digest, slip_script and slip_cli may`` at the line of the first hop.
- **`forbid-unsafe`.** A root passes when a line, after leading spaces,
  starts with `#![forbid(unsafe_code)]`. Every `.rs` file under `src/bin/`,
  at any depth, counts as a root. A `[lib]` or `[[bin]]` `path` outside
  these places is not followed.
- **`dependencies`.** A third-party crate is a `Cargo.lock` package with a
  `source`; a workspace member has none, so `xtask`'s dependencies on
  `slip_model` and `slip_exec` are not third-party. Allowed: the §3.1
  crates at any version, and every package with a `source` in
  `kiln/Cargo.lock` at the same name and version. The Kiln lock is read
  from disk (it is in the submodule); when it cannot be read the message
  ends `(kiln/Cargo.lock cannot be read)`. No `Cargo.lock` is no violation.
  Message: ``third-party crate `<name> <version>` is not allowed by
  spec/harness.md §3.1``.
- **`spec-index`.** The index is the first Markdown table after the line
  `## Index` (outside fenced code) up to the next `## ` heading. The first
  column names the file (a link's target, else the text without
  backticks) relative to `spec/`; the last is the status, of which a
  trailing parenthesised note is not part (`not written (M1)` is
  `not written`). Specs are listed `spec/*.md` files other than
  `spec/README.md`, not files in subdirectories.
- **`spec-shape`.** The headings must be whole lines outside fenced code.
- **`landed`.** The title's id must be the file name's id. `spec/README.md`
  is not a spec a landed file can name.
- **`wp`.** An id is `WP-` and three or more digits. Lines 3 to 7 are
  exactly `- Status:`, `- Milestone:`, `- Track:`, `- Needs:`, `- Size:` in
  that order and line 2 is blank (`docs/wp/README.md`: "The first seven
  lines are checked"). Allowed values: status `todo`, `in-progress`,
  `review`, `done`, `dropped`; milestone `M` and digits; track `A` to `H`;
  size `S`, `M`, `L`; needs `none` or comma-separated ids with files.
  `Parallel with` is not checked.
- **`local-paths`.** Kiln's detector with Slip's four prefixes: a path
  starts at the start of a line, after a character that cannot be in a
  path, or after a one-letter flag (`-I`), not after `>`, `}`, `)`, `$`, `*`;
  a bare prefix (followed by a quote, a backtick or the end) is not a path.
  Binary files (NUL byte, or not UTF-8) are skipped. **No file is exempt**:
  the rule's text names only bare prefixes, and the lint's tests build
  their local paths with `concat!` of a bare prefix and a name, so the
  committed tree spells none and `cargo xtask lint` passes on itself.
  Message: ``absolute local path `<path>` ``.
- **`doc-links`.** Documents are listed `*.md` files under `docs/` and
  `spec/`, and `README.md`. Links are inline links and images (angle-bracket
  targets accepted) and reference definitions, outside fenced code and
  inline code. Targets with a scheme, `mailto:`, a leading `#` or a leading
  `/` are not relative and not checked. The `#anchor` and `?query` are
  dropped, `%20` is decoded, and the target is resolved against the
  document's directory; it must be a listed file, a directory containing
  listed files, or a path inside a submodule that exists on disk. A target
  above the root fails. Anchors are not checked. Message:
  ``link `<target>` points at no file``.

## 4. Work packages

`cargo xtask wp` prints the work packages in `docs/wp/` as a table: id,
status, milestone, track, size, needs, title; ordered by id.
`cargo xtask wp --ready` lists those with status `todo` whose needs are all
`done`.

The table has a header row `id status milestone track size needs title`,
columns padded to the widest cell and separated by two spaces, trailing
spaces trimmed. `needs` is the ids joined by `, `, or `none`. Ids order
numerically (`WP-999` before `WP-1000`). `--ready` prints the same table,
header included even when empty. A file that fails the `wp` lint makes the
command print those violations to standard error and exit 1; any other
argument is a usage error, exit 2.

## 5. Jobserver

```
cargo xtask jobserver status [--fifo <path>] [--jobs <n>]
cargo xtask jobserver refill [--fifo <path>] [--jobs <n>]
```

As `spec/exec.md` §3 "Inspection". Neither command blocks.

- `status` prints `jobserver: <fifo>: <idle> of <n> tokens idle` and exits
  0. It changes nothing.
- `refill` tops the pool up to `<n>` idle tokens and prints
  `jobserver: <fifo>: <idle> idle, <added> added`. It is only correct while
  no client holds a token; the command cannot know that and says so in its
  usage text.

The line goes to standard output, the fifo's path as given or resolved
(`Path::display`).

- **Options.** `--fifo <path>` and `--jobs <n>`, each at most once, in any
  order, value as the next argument. A missing value, an empty `--fifo`, a
  repeated option, any other argument, and a missing or unknown command are
  usage errors: `xtask jobserver: <message>` and the usage text on standard
  error, exit 2.
- **The fifo** is `--fifo`, else `slip_exec::jobserver::resolve_fifo`: the
  last `--jobserver-auth=` word of `MAKEFLAGS` if it is `fifo:<path>` (a
  descriptor form names no fifo and falls through), else a non-empty
  `$SLIP_JOBSERVER_FIFO`, else `/run/slip/jobserver.fifo`. A `MAKEFLAGS`
  that is not UTF-8 is ignored, as `default_fifo` ignores it.
- **`<n>`** is `--jobs`, else `$SLIP_JOBS`, else 8
  (`slip_exec::jobserver::DEFAULT_POOL_SIZE`). It is ASCII digits only (no
  sign, no spaces), not 0, fitting `usize`. A bad `--jobs` is a usage error
  (exit 2) ``--jobs: `<v>` is not a positive integer``; a bad `$SLIP_JOBS`,
  including an empty one, is an error (exit 1)
  ``SLIP_JOBS: `<v>` is not a positive integer``, and one that is not UTF-8
  is the error ``SLIP_JOBS `<v>`: not UTF-8``. This is stricter than
  `slip_exec::jobserver::resolve_pool_size`, which trims and accepts 0.
- **Checks before opening** (exit 1, `xtask jobserver: <path>: <what>`):
  `does not exist`; `is not a fifo` (symbolic links followed; a directory
  or regular file is never opened); `cannot stat: <error>`. A failure of
  `status` or `refill` itself is `cannot read the pool: <error>` or
  `cannot refill: <error>`.

### 5.1 Interface

`xtask::jobserver`:

```rust
pub enum Failure { Usage(String), Error(String) }               // exit 2, exit 1
pub struct Request { pub refill: bool, pub fifo: PathBuf, pub jobs: usize }
pub fn main(args: &[String], env: &dyn Fn(&str) -> Option<OsString>) -> ExitCode;
pub fn run(args: &[String], env: &dyn Fn(&str) -> Option<OsString>) -> Result<String, Failure>;
pub fn parse(args: &[String], env: &dyn Fn(&str) -> Option<OsString>) -> Result<Request, Failure>;
pub fn inspect(request: &Request) -> Result<String, String>;
```

The environment is a parameter, so tests never read the host's own.

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

### 6.1 The digest's pair tests

The pair tests of `slip_digest` (`crates/slip_digest/tests/{differ,equal,demand,indeterminate}/`,
the forms of `spec/digest-kinds.md` §1.5) parse each side with
`slip_kiln::snippet`: Kiln's front end over the test's files held in
memory, as `kiln -std=<std> [--target=<t>] <args> -fsyntax-only` would,
with no predefined macro, no header search beyond the test's own files and
no file system (`crates/slip_kiln/src/snippet.rs`, "Language options",
lists what else differs from that command line). The default line is
`-std=c++20` for `x86_64-unknown-linux-gnu`. A side's main file (`t.cpp`
for both sides, or each side's `a.cpp` / `b.cpp`) changes it with header
lines, in any order, among its leading lines:

```
// std: c++26
// errors: yes
// target: aarch64-unknown-linux-gnu
// args: -fcf-protection=branch -frounding-math
```

- The header block is the main file's leading lines that are header lines
  or `#include "side.h"`; it ends at the first line that is neither. A
  header line is exactly `// <key>: <value>`, with the keys `std`,
  `errors` (value `yes`), `target` (one word) and `args` (split at
  spaces, no quoting). A line of the block that looks like one
  (`//`, blanks, a word of letters and `-`, blanks, `:`) but is not one
  (`// arg:`, `//target:`, `// std:c++26`, a key given twice) fails the
  test; a comment that does not look like one ends the block. A header
  line anywhere after the block (after a blank line, a comment, a
  byte-order mark, a `/* */`) fails the test: it would not be read.
- `target` is a triple Kiln's driver knows (`aarch64` or `x86_64` Linux);
  `args` are Kiln driver arguments that change the language, never what
  is read or written or the form of the diagnostics. A line on which
  `kiln` would stop before the job (a driver error, an invalid
  diagnostic option, a driver warning `-Werror` makes an error, a
  per-input error such as `-fgnu89-inline` for C++) is refused too.
  `slip_kiln::snippet::parse_with` refuses them, and the refusal fails
  the test (`spec/landed/harness/WP-038.md`).
- In a `t.cpp` test both sides read the same lines. In an `a.cpp` /
  `b.cpp` test each side reads its own, so a `differ` test may differ in
  nothing but its `// args:` line: that shows a command-line fact is in
  the digest.
- A file of the test directory other than `t.cpp`, `a.cpp`, `b.cpp` and
  `expect.txt` is a file of both sides' snippets, and so is each file of
  `crates/slip_digest/tests/support/` the directory has none of the name
  of: `meta.h` declares the `std::meta` intrinsics and class types the
  reflection tests use, as Kiln's own test header
  (`kiln/tests/support/kiln_meta.h`) declares them.
- `SLIP_DIGEST_DUMP=1` prints, for each side, the command line in effect
  and the units; `SLIP_DIGEST_ONLY=<text>` makes the four pair runners
  run only the directories whose path contains `<text>`, and they print
  `SLIP_DIGEST_ONLY=<text>: <n> of <m> directories`. Nothing else
  narrows (the determinism and tables tests use every directory), and
  `cargo xtask ci` removes the variable from the environment of what it
  runs.

## 7. Performance records

A parallel phase writes a run record (`spec/model.md` §9) into its store
(`spec/exec.md` §2). The commands here take a run record **by its path in
the store**, `<store>/runs/<plan id>/<phase>.<round>.<started>.run`: the
store is the directory three levels up, and the plan and the shards of
the run are read from that store (shared access). The plan, phase and
round are the record's own (`spec/model.md` §9); the file name only
locates it.

```
cargo xtask perf summarize <run record>…
cargo xtask perf record <corpus> <run record>
cargo xtask perf check  <corpus> <run record>
```

**Path shape.** The last three components must be plain names: `runs`, a
plan id (64 lower-case hexadecimal digits), and
`<phase>.<round>.<started>.run` with phase `map` or `verify` and the two
numbers canonical decimal `u64` (`0`, or digits without a leading zero).
The store is the path without those three components, `.` when nothing is
left. Components are taken as written (`Path::components`): a `..` among
the last three is an error, not resolved. A path of another shape is an
error.

**Loading**, in this order:

1. The record is read and validated as a `RUNR` container; its `plan`,
   `phase`, `round` and `started_unix_ns` must be its path's (the store
   names the file by all four, `spec/exec.md` §2). The record is read
   before the store is opened, so an absent or empty directory is never
   initialised as a store.
2. The store is opened `Shared`.
3. The plan must be in the store, validate, and have the content id its
   name says.
4. The **observers of the record** are the distinct `Job.observer` values.
   One that is not an observer of the plan is an error (its main file,
   which the summary prints, would be unknown).
5. For each observer of the record, the shard of the run's phase and round
   is read; one that does not validate, or names another observer, phase or
   round, is an error naming the shard file. A missing shard is not an
   error when loading: `summarize` reports it, `counters` (§7.1, and so
   `record` and `check`) refuse it. A shard's content id is not compared
   with any `Job.shard`.

Errors name their file: the path given, or the store's path for the plan
and shards (`Store::plan_path`, `shard_path`). Every error (an unreadable
or invalid file, an unknown corpus, a cancelled run where counters are
needed, an overflow) prints `xtask perf: <message>` on standard error and
exits 1; a wrong argument count or unknown command is a usage error with
the usage text, exit 2.

### 7.1 What is computed

From the record, its plan and its shards:

- **Observers**: the distinct observers among the record's jobs.
- **Counters**, all exact and deterministic for a given corpus, tool and
  host headers:

  | Name | Value |
  |---|---|
  | `plan.observers` | observers in the plan |
  | `plan.excluded.<reason>` | excluded entries per reason, every reason of `spec/model.md` §7 present, zeros included |
  | `run.observers` | observers in the record |
  | `shard.<status>` | the record's observers by the status of their shard in the store, the four statuses present, zeros included |
  | every shard counter name | the sum of that counter over those shards |

  Shards are those of the record's observers only. An observer of the
  record with no shard in the store is an error: the record is of a
  cancelled run, or the store is not the run's. A shard counter whose name
  is one of the names this table computes (`plan.observers`,
  `plan.excluded.*`, `run.observers`, `shard.*`) is an error, not an
  overwrite.
- **Measured**, none deterministic: `wall_s` (the run's `wall_ns`), `cpu_s`
  (the sum of `cpu_ns` over every job), `peak_rss_max` and the percentiles
  `peak_rss_p50`, `peak_rss_p90`, `peak_rss_p99` (over the jobs that ended
  `completed` with a non-zero `peak_rss`; nearest rank: the value at
  position `ceil(p × n / 100)` of the ascending list, clamped to `1..=n`;
  0 when there is no such job), `peak_rss_sum`, `cores`, `workers_max`,
  `memory_budget` (those four as in the record). Sizes are whole bytes.
  Seconds are printed with three decimals, computed in integers from
  nanoseconds: `(ns + 500 000) / 1 000 000` milliseconds in `u128`,
  printed `<ms / 1000>.<ms % 1000, three digits>` (half up; `u64::MAX` ns
  is `18446744073.710`).

Every sum is a checked `u64` addition; an overflow is an error
(`<what> overflows a 64-bit unsigned integer`).

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
efficiency <cpu / (wall × workers_max), two decimals; `-` if the divisor is 0>
workers_max <n> cores <n> memory_budget <bytes>
peak_rss max <bytes> p50 <bytes> p90 <bytes> p99 <bytes> sum <bytes>
counter <name> <value>                the shard counters, by name
top cpu
  <cpu_s> <main file>                 the ten observers with the greatest summed cpu_ns, greatest first
top peak_rss
  <bytes> <main file>                 the ten observers with the greatest peak_rss of any job, greatest first
```

- Every record is loaded before anything is printed; an error in any of
  them prints nothing on standard output and exits 1.
- Lines are exactly the template with single spaces; the `top` entries are
  indented two spaces; the output ends with a newline. `shard` lines are in
  the order `ok`, `diagnosed`, `rejected`, `failed`; `job` lines in the
  order `completed`, `crashed`, `killed_memory`, `killed_time`,
  `cancelled`, every job (attempt) counted. `counter` lines are the shard
  counters summed over the shards present, by name (byte order). `tool` is
  the record's `tool_slip` and `tool_kiln`.
- **Efficiency** is computed from nanoseconds, not from the printed
  seconds: `cpu_ns × 100 / (wall_ns × workers_max)` rounded half up to two
  decimals in integers; `-` when `wall_ns × workers_max` is 0.
- `top cpu` ranks every observer of the record, zeros included, by its
  summed `cpu_ns` (every attempt); `top peak_rss` by the greatest
  `peak_rss` of any of its jobs, whatever their end. At most ten each;
  ties are broken by observer id, ascending. `<main file>` is the
  observer's main file path from the plan.
- With a cancelled run (an observer without a shard) `summarize` still
  prints everything the record alone gives, the `shard` lines counting the
  shards present, and a line `missing shards <n>` right after them. The
  line is printed only when `n` > 0.
- **Text.** The given path, the tool versions and the main file paths are
  printed as one line: valid UTF-8 as itself, `\` as `\\`, control
  characters below U+0100 and bytes that are not UTF-8 as `\xNN`.

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
counter of §7.1 sorted by name; `measured` is in the order above. A
baseline contains no absolute path and no time of day.

**Writing.** A baseline is written with `slip_model::json::write_pretty`.
Counters and the whole-number measured values are JSON integers, also
above 2^53 (the string rule of `spec/model.md` §10 is for container
renderings, not baselines). `wall_s` and `cpu_s` are JSON numbers with a
fraction: the `f64` nearest the three-decimal seconds of §7.1, written
shortest round-trip (`7.5`, `4.124`, `0.0`).

**Reading** (for `check`) is with `slip_model::json::parse`. Every member
is required and none other is allowed, in any order. Messages:
`not JSON: <reader error>`, `at the top level: expected an object`,
``no member `<key>` ``, `at <key>: unknown member`,
`at <key>: expected a string`, `at counters: expected an object`,
`at counters.<name>: expected an integer in 0..2^64`,
`at measured: expected an object`, ``at measured: no member `<name>` ``,
`at measured.<name>: unknown member`,
`at measured.<name>: expected an integer in 0..2^64`, and for `wall_s` and
`cpu_s` `at measured.<name>: expected a non-negative number of seconds
with a fraction` (a JSON number without a fraction or an exponent, such as
`4`, is refused). The command prefixes `bench/baselines/<corpus>.json: `.
A baseline whose `corpus` is not the corpus named on the command line is
an error.

- `perf record <corpus> <run record>` looks the corpus up
  (`xtask::corpus::rows`) before reading the record; refuses a record that
  does not cover the plan (`run.observers` ≠ `plan.observers`: ``the
  record covers <a> of the plan's <b> observers``) before computing
  counters; then writes `bench/baselines/<corpus>.json` under the
  repository root through `.<name>.<pid>.tmp` in the same directory and
  one rename (creating the directory, replacing a file that exists,
  removing the temporary file on failure), and prints the full path. A
  baseline update changes `counters` only together with a stated reason in
  the commit message.
- `perf check <corpus> <run record>` compares against the committed
  baseline. `pin` and `counters` are compared **exactly**; `measured` is
  informational and never fails the check; the tool versions are printed
  when they differ and do not fail it. Output, on standard output, in this
  order: `pin: baseline <a>, run <b>` if the pin differs (the run's pin is
  the corpus row's tag); `slip: baseline <a>, run <b>` and `kiln: …` if a
  tool version differs; one line `<name>: baseline <a>, run <b>` per
  counter that is missing, extra or different, by name, `-` standing for a
  counter absent on that side; then all ten measured values
  `<name> <run value> (×<ratio>)` (`wall_s 12.300 (×1.04)`: the run's
  seconds with three decimals, whole numbers in decimal; the ratio is
  `run / baseline` in `f64` printed with two decimals, `-` when the
  baseline value is 0); and last `perf check: <corpus>: ok` (exit 0) or
  `perf check: <corpus>: FAILED (<n> difference(s))` (exit 1), `n`
  counting the pin and the counters.

### 7.4 Interface

`xtask::perf`, with `xtask::perf::render`; of `render`, everything but
`TOP` and `text` is re-exported from `perf`:

```rust
// perf
pub const BASELINES: &str = "bench/baselines";
pub struct RunPath { pub store: PathBuf, pub plan: Digest, pub phase: Phase, pub round: u64, pub started_unix_ns: u64 }
pub fn parse_run_path(path: &Path) -> Result<RunPath, String>;
pub struct Loaded { pub path: RunPath, pub record: RunRecord, pub plan: Plan,
                    pub shards: BTreeMap<Digest, Option<Shard>> }   // every record observer; None: no shard
impl Loaded { pub fn missing_shards(&self) -> usize; }
pub fn load(path: &Path) -> Result<Loaded, String>;
pub const REASONS: [Reason; 6]; pub const STATUSES: [Status; 4]; pub const ENDS: [End; 5];
pub fn reason_name(Reason) -> &'static str; pub fn status_name(Status) -> &'static str; pub fn end_name(End) -> &'static str;
pub fn shard_statuses(loaded: &Loaded) -> [(Status, u64); 4];
pub fn shard_counters(loaded: &Loaded) -> Result<BTreeMap<String, u64>, String>;
pub fn counters(loaded: &Loaded) -> Result<BTreeMap<String, u64>, String>;
pub struct Measured { pub wall_ns: u64, pub cpu_ns: u64, pub peak_rss_max: u64, pub peak_rss_p50: u64,
                      pub peak_rss_p90: u64, pub peak_rss_p99: u64, pub peak_rss_sum: u64,
                      pub cores: u64, pub workers_max: u64, pub memory_budget: u64 }
impl Measured { pub fn texts(&self) -> [String; 10]; pub fn values(&self) -> [(&'static str, Num); 10]; }
pub fn measured(run: &RunRecord) -> Result<Measured, String>;
pub fn percentile(sorted: &[u64], p: u64) -> u64;
pub enum Failure { Usage(String), Error(String) }
pub fn main(repo: &Path, args: &[String]) -> ExitCode;
pub fn run(repo: &Path, args: &[String], out: &mut dyn Write) -> Result<bool, Failure>;  // Ok(false): check failed

// perf::render
pub const TOP: usize = 10;
pub fn seconds(ns: u64) -> String;
pub fn text(bytes: &[u8]) -> String;
pub fn render_summary(given: &str, loaded: &Loaded) -> Result<String, String>;
pub enum Num { Seconds(f64), Count(u64) }
pub const MEASURED: [(&str, bool /* seconds */); 10];
pub struct Baseline { pub corpus: String, pub pin: String, pub slip: String, pub kiln: String,
                      pub counters: BTreeMap<String, u64>, pub measured: Vec<(String, Num)> }
pub fn render_baseline(corpus: &Corpus, loaded: &Loaded) -> Result<String, String>;
pub fn read_baseline(bytes: &[u8]) -> Result<Baseline, String>;
pub fn check(baseline: &Baseline, corpus: &Corpus, loaded: &Loaded) -> Result<(String, bool), String>;
```

`measured` and `render_summary` return `Result` because a sum can
overflow. `check` takes a parsed `Baseline`, so that an error in the
baseline names the baseline file (the caller prefixes it) and an error in
the run names the run's file.

## 8. Corpora

A corpus is pinned third-party source with a CMake build tree that has a
compilation database and the generated sources its translation units
include, so that every entry of the database can be parsed. Building the
corpus itself is not part of it. Corpora live under a corpus root (§8.2),
`.corpus/` (git-ignored) by default, together with the stores of runs over
them. The pins are Kiln's (`kiln/scripts/oss/projects.toml`) so both
projects measure the same code: LLVM 19.1.7 as C++17 first (§8.7).

```
cargo xtask corpus list
cargo xtask corpus fetch <name>
cargo xtask corpus configure <name>
cargo xtask corpus status <name>
cargo xtask corpus path <name> [--db]
```

### 8.1 `bench/corpora.toml`

One table per corpus; the table name is the corpus name (lower-case ASCII
letters, digits, `-`, `_`). All keys are required; any other key is an
error.

| Key | Value |
|---|---|
| `kiln` | the name of a table of `kiln/scripts/oss/projects.toml` |
| `cxx_std` | non-negative integer (fitting 32 bits), `CMAKE_CXX_STANDARD` |
| `build_type` | string, `CMAKE_BUILD_TYPE`: non-empty, no whitespace, no `/` |
| `c_compiler`, `cxx_compiler` | strings, compiler names looked up on `PATH`: non-empty, no whitespace, no `/` |
| `configure` | array of strings, further CMake arguments, in order |
| `generate` | non-empty array of distinct entries: a build-target name (`[A-Za-z0-9_.+-]`, not starting with `-`), or `<target>:<file>`, one generated file made by a rule of that target's `build.make` (a relative path without `.`, `..` or empty components, without whitespace or `:`, with an extension of §8.6) |

The pin comes from the Kiln row, so Slip and Kiln measure the same code:
its `url` (a required string: non-empty, not starting with `-`, no
whitespace), its `tag` (a required string: non-empty, not starting with
`-`, of `[A-Za-z0-9_./+-]`) and its optional `source_subdir` (a relative
path without `.`, `..` or empty components; absent means the checkout
itself). The Kiln row must be a plain git clone: a row whose `archive`,
`local`, `submodules` or `stage_of` is set to anything but `false` is an
error. Other Kiln keys are ignored.

`configure` must not set what xtask sets itself: an argument starting
`-G`, `-S` or `-B`, `--fresh`, or `-D` of `CMAKE_EXPORT_COMPILE_COMMANDS`,
`CMAKE_BUILD_TYPE`, `CMAKE_C_COMPILER`, `CMAKE_CXX_COMPILER` or
`CMAKE_CXX_STANDARD` (with or without a `:TYPE`).

Both files are read with the line reader of the lints (tables, strings,
integers, arrays of strings, `#` comments outside strings). A nested or
array table, an entry outside a table, a duplicate table or key, or a value
of the wrong shape is an error naming the file and line. Errors exit 1
with `xtask corpus: <message>`.

### 8.2 Layout

The corpus root is `$SLIP_CORPUS_DIR` if set and non-empty, else `.corpus/`
in the repository. A relative `$SLIP_CORPUS_DIR` is an error. Corpus
`<name>` lives in `<root>/<name>/`:

| Path | What |
|---|---|
| `src/` | the clone |
| `src.partial/` | a clone in progress; removed before the next clone |
| `build/` | the build tree; `build/compile_commands.json` is the database |
| `fetch.stamp`, `configure.stamp`, `generate.stamp` | §8.4 |
| `fetch.log`, `build/configure.log`, `build/generate.log` | the output of `git clone`, `cmake`, `make` |
| `lock` | §8.5 |

### 8.3 Steps

**fetch.** Needs `git`. If `src/` exists and the fetch stamp is current
(§8.4), nothing runs. If `src/` exists without a current stamp, its `HEAD`
must be the commit of `refs/tags/<tag>` there; then the stamp is written
(and the later stamps removed), else it is an error that says to remove
`src/` to fetch again: an existing clone is never deleted or overwritten.
Otherwise all three stamps are removed, `src.partial/` is removed if
present, and

```
git -c advice.detachedHead=false clone --quiet --depth 1 --branch <tag> -- <url> <dir>/src.partial
```

runs in `<root>/<name>/`; then `HEAD` is checked against the tag as above,
`src.partial/` is renamed to `src/`, and the stamp is written.

**configure.** Runs fetch first (a no-op when present), then needs
`cmake`, `make`, and the two compilers on `PATH`, each resolved to its
absolute path. The CMake arguments are, in order:

```
--fresh -S <src>[/<source_subdir>] -B <build> -G "Unix Makefiles"
-DCMAKE_BUILD_TYPE=<build_type> -DCMAKE_C_COMPILER=<cc path>
-DCMAKE_CXX_COMPILER=<cxx path> -DCMAKE_CXX_STANDARD=<cxx_std>
-DCMAKE_EXPORT_COMPILE_COMMANDS=ON <configure…>
```

If the configure stamp is current and the database exists, cmake does not
run. Otherwise the configure and generate stamps are removed, cmake runs in
`build/`, and the stamp is written if it succeeded and wrote the database.

Then the **generate step**: every `generate` target must be a target of
the configured build (§8.6), and every `<target>:<file>` file must have a
rule (a line `<file>:`) in its target's `build.make`, else an error before
anything is built. If the generate stamp is current nothing is built;
otherwise `make <targets…>` (the plain entries, if any) and then, per
`<target>:<file>` entry in order, `make -f <dir>/build.make <file>` run in
`build/`. xtask never passes `-j`: make inherits `MAKEFLAGS` and
takes its parallelism from the jobserver named there (one job without
one). Then the generated check (§8.6) runs, whether or not make ran; if it
fails, the generate stamp is removed and configure fails. Otherwise the
stamp is written (if make ran) and configure prints the number of generated
files checked and the number of database entries (the objects of its
top-level JSON array; anything else is an error).

**status.** Runs nothing; prints `corpus`, `directory`, `source` (present
at the stamped commit, present but not stamped for this pin, or absent),
`build` (configured when a configure stamp exists for the stamped commit),
`database` (path and entries, unreadable, or absent) and `generated`
(`done (<n> targets)` when the generate stamp is current for the configure
stamp and the row's targets, else `not done`). Whether the configure stamp
matches the row is not checked by `status`, since the compilers' paths are
part of it; `configure` is the check.

**path.** Prints `<root>/<name>`; with `--db`, the database's path, and
exits 1 if it does not exist.

**list.** A table `name kiln tag url`, columns padded as in §4.

A wrong argument count, an unknown command or option is a usage error
(exit 2). An unknown corpus is an error naming the known ones.

A step's tool is looked up on `PATH` (absolute directories only, an
executable regular file). A missing one fails with the single line
``xtask corpus: `<tool>` is not on PATH: run scripts/provision.sh --full
(nothing is installed by xtask)``. A command that fails prints the last 20
lines of its log, each prefixed `  | `, and fails with
`<command> failed (<status>); the log is <path>`.

Progress lines (on stdout) say what each step did and how long it took.

### 8.4 Stamps

A stamp is a text file whose first line is `slip-corpus-stamp 1`, written
to `<stamp>.tmp` and renamed. A stamp is current when its bytes are exactly
what the step would write now:

- fetch: `step fetch`, `url <url>`, `tag <tag>`, `commit <sha>` (the commit
  read back from the stamp itself, so a changed `url` or `tag` makes it
  stale);
- configure: `step configure`, `commit <sha>`, then `arg <a>` for every
  CMake argument of §8.3 in order (so the compilers' resolved paths, the
  directories and every `configure` value are part of it);
- generate: `step generate`, every line of the configure stamp after its
  first prefixed `configured `, then `target <t>` per target in order.

So an unchanged row is a no-op, a changed configuration runs cmake and the
generate step, and a changed target list runs only the generate step. A
step that runs removes the stamps after it before it starts.

### 8.5 Lock

`fetch` and `configure` hold `<root>/<name>/lock` (created exclusively,
holding the process id) while they run, and remove it when they end,
successfully or not. If it exists and `/proc/<pid>` exists (or the pid
cannot be read, or there is no `/proc`), the command fails naming the
holder; a lock whose process is gone is taken over. `status`, `path` and
`list` take no lock. (`flock` is not used: on this host's bind-mounted
workspace it does not exclude.)

### 8.6 The generated check

Read from the configured "Unix Makefiles" tree:

- `CMakeFiles/Makefile2` maps a target name `t` to its rule directory by
  the lines `t: <dir>/rule` (`<dir>` ending `.dir`, `t` without `/`), and
  gives target dependencies by the lines `<dir>/all: <dir'>/all`;
- `<dir>/build.make` lists what building the target makes by the lines
  `t: <file>…`.

Recipe lines (tab-indented), comments and blank lines are ignored. The
files a list of targets **promise** are, over the targets and every target
they depend on transitively, each `<file>` of a `t: <file>` line that is not
under a `CMakeFiles` directory and whose extension is one of `inc`, `def`,
`h`, `hpp`, `hh`, `hxx`, `gen`; plus the file of each `<target>:<file>`
entry. Relative paths are relative to `build/`.
Every promised file must exist after the generate step. A failure lists up
to 20 missing paths and fails with
`<m> of the <n> generated files the generate targets promise are missing`.

### 8.7 The `llvm` corpus

`llvm` is Kiln's `llvm_project` row (llvm-project, tag `llvmorg-19.1.7`,
source directory `llvm`) configured as Release, C++17, with `clang-19` and
`clang++-19`, `LLVM_ENABLE_PROJECTS=clang`, `LLVM_TARGETS_TO_BUILD=X86;AArch64`,
tests included, benchmarks and examples excluded, and the optional host
libraries (zlib, zstd, libxml2, libedit, libpfm) off so the configuration
does not depend on what the host has installed.

Its `generate` list covers every target of the configured tree whose
`build.make` lists generated files as its own outputs (the §8.6 rule),
except the four that copy clang's builtin headers into the build tree
(`clang-resource-headers`, `arm-resource-headers`,
`arm-common-resource-headers`, `aarch64-resource-headers`): no translation
unit includes those copies through the build tree (no database command
names `lib/clang/19/include`). It names the umbrella targets
(`intrinsics_gen`, `omp_gen`, `acc_gen`, `vt_gen`, `llvm_vcsrevision_h`,
the three `…TargetParserTableGen`, `X86CommonTableGen`,
`AArch64CommonTableGen`, `clang-tablegen-targets`) and every generator
target they do not reach through `Makefile2`. One generated file,
`tools/clang/lib/Basic/VCSVersion.inc`, is a source of the library target
`obj.clangBasic` rather than the output of a target of its own, hence the
entry `obj.clangBasic:tools/clang/lib/Basic/VCSVersion.inc`.

What a parse of it finds, and why (`bench/baselines/llvm.json` holds the
counts):

- The database has 4069 entries and the plan 4061 observers, none
  excluded: eight files (seven of `llvm/utils/TableGen`, and
  `clang/tools/clang-fuzzer/DummyClangFuzzer.cpp`) are each compiled twice
  with one command, and each pair is one observer (`spec/cdb.md` §7).
- Two units are `diagnosed`, and clang 19 rejects them with the same
  error at the same place: `clang/tools/clang-fuzzer/ClangFuzzer.cpp` and
  `ClangObjectiveCFuzzer.cpp` name `uint8_t` without including a header
  that declares it (no target of the configuration builds them).
- With Kiln at `62d190007`, nineteen units are `failed`: the front end
  panics in three defects of Kiln's constant evaluator. Six are the two
  of `docs/requests/KD1-oven-panics-llvm.md`; thirteen (eleven of
  `llvm/lib/DWARFLinker/Parallel`, two unit tests) include
  `llvm/ADT/ConcurrentHashtable.h`, whose parenthesised `std::min` call
  as a shift count panics a fast path Kiln added between Slip's two pins
  (`docs/requests/KD2-oven-minmax-paren.md`). Slip's pin is now
  `83566cc47` (Kiln's `main`, WP-044), which has both fixes: the
  reproducers of KD1 and KD2 parse `ok` (`crates/slip_kiln/tests/
  parse.rs`). The counts in this section and in
  `bench/baselines/llvm.json` are still those of `62d190007` until the
  corpus is run again at the new pin.
- The parse task on Kiln's front-end entry (WP-025) gives every unit
  that is not `failed` the shard the dependency-file flow of WP-010 gave
  it at `4bfd999da`: 4040 `ok` and 2 `diagnosed` shards with the same
  outcome and the same inputs, entry for entry. `pp.tokens` is 2 787 822 931.

## Tests

- `xtask/tests/lint.rs` — §3: for each lint, a fixture tree that passes and
  one that fails with the expected message.
- `xtask/tests/ci.rs` — §2: step selection, order, output lines, exit status,
  with fake `cargo`.
- `xtask/tests/wp.rs` — §4.
- `xtask/tests/jobserver.rs` — §5, over fifos the test makes and holds
  open: `status` (tokens left in place), `refill` short, full and above
  `<n>`, `<n>` from `$SLIP_JOBS` and the default, fifo precedence
  `--fifo` > `MAKEFLAGS` > `$SLIP_JOBSERVER_FIFO` (and the descriptor form
  falling through), the default resolved without opening it, `<n>` precedence, bad
  `<n>` from either source, a missing path, a regular file and a directory,
  the usage errors, and the executable with the host's variables cleared.
- `xtask/tests/perf.rs` — §7, over stores the test writes (plan, shards,
  run record built with `slip_model`), with golden output in
  `xtask/tests/golden/perf/`: path shapes, records disagreeing with their
  path, every counter row, a missing shard, a shard of another observer,
  phase or round, an invalid shard, overflow, counter-name collision, an
  observer not in the plan, percentiles and measured over 0, 1, 2, 100 and
  101 jobs, seconds rounding, summary goldens of a full and a cancelled run
  with ties, efficiency `-`, the ten entry limit and escaped paths, two
  records through the executable, the baseline golden, partial coverage
  refused, `record` then `check`, every kind of check difference, malformed
  baselines, and the executable's usage, unknown-corpus and
  unreadable-record errors.
- `xtask/tests/corpus.rs` — §8: rows and their Kiln pins, every
  malformed-row error, the committed `llvm` row (§8.1); root and layout
  (§8.2); `path`, `list` and usage errors through the binary (§8.3); the
  missing-tool message (§8.3); stamps with fake `cmake` and `make` (§8.4);
  the generated check including a dependency's outputs (§8.6); the lock
  (§8.5); fetch and configure of a local git repository with a three-file
  CMake project and one generated header, and a fetch of a missing tag
  (reported `SKIP` without `git`, `cmake`, `make` and a compiler). Unit
  tests in `xtask/src/corpus/build_tree.rs`: the database entry count and a
  target's outputs.

## Open questions

- The `inputs.*` counters include system headers, so a baseline is exact
  only for one set of installed headers (the host compiler's and the
  standard library's). The baseline does not record which; a change of the
  container image shows up as a counter difference with no stated cause.
- Nothing checks that a shard in the store is the one the record's job
  wrote (`Job.shard` is its content id): a later run of the same plan,
  phase and round replaces shards. Counters of shards that are not `failed`
  are the same either way, which is why this is left open.
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
- `git` and `cmake` (§8.3) are not jobserver clients and run without a
  token (§1 principle 5). Starting them through `slip_exec::process`, which
  holds one, would account for them on the shared host.
- The corpus lock is a file holding a process id, which only excludes within
  one container (`flock` does not exclude on the bind-mounted workspace).
  Corpora shared between containers need something else.
- `corpus configure` does not report a generator target that the row's
  `generate` list fails to reach; a pin change can leave the list short and
  only a failed parse would show it.
