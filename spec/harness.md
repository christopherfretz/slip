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
5. **Shared machine.** Everything the harness starts goes through
   `slip_exec::process` and holds a jobserver token while it runs
   (`spec/exec.md` §3, §4).

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

## 4. Work packages

`cargo xtask wp` prints the work packages in `docs/wp/` as a table: id,
status, milestone, track, size, needs, title; ordered by id.
`cargo xtask wp --ready` lists those with status `todo` whose needs are all
`done`.

## 5. Jobserver

`cargo xtask jobserver status|refill [--fifo <path>] [--jobs <n>]`, as
`spec/exec.md` §3 "Inspection". The fifo is the one named by `MAKEFLAGS`,
else `$SLIP_JOBSERVER_FIFO`, else `/run/slip/jobserver.fifo`; `<n>` is
`$SLIP_JOBS`, else 8.

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

A parallel phase writes a run record (`spec/model.md` §9).
`cargo xtask perf summarize <run record>…` prints, per record: observers,
jobs per `End`, wall time, summed CPU time, parallel efficiency
(`cpu / (wall × workers_max)`), the largest and the 50th/90th/99th
percentile `peak_rss`, `peak_rss_sum`, and the ten most expensive observers
by CPU time and by peak memory.

Baselines are committed under `bench/baselines/<corpus>.json`:

```json
{
  "corpus": "llvm-19.1.7",
  "slip": "0.0.0", "kiln": "1.3.0",
  "counters": { "plan.observers": 0, "shard.ok": 0, "inputs.files": 0, "pp.tokens": 0 },
  "measured": { "wall_s": 0.0, "cpu_s": 0.0, "peak_rss_max": 0, "peak_rss_sum": 0, "load_average": 0.0, "cores": 0 }
}
```

`counters` are sums of shard counters over the corpus plus plan and status
counts; they are compared **exactly** by
`cargo xtask perf check <corpus> <run record>`, and a difference fails. A
baseline update changes `counters` only together with a stated reason in the
commit message. `measured` is informational: `perf check` prints the ratio
to the baseline and never fails on it. Baselines contain no absolute paths.

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

## Open questions

- Hosted CI (GitHub Actions) for the Slip repository: not set up; the gate
  runs on the development host for now.
- Whether Slip and Kiln should share harness code as one crate (request KM2
  in `docs/design/05-kiln-interface.md`). Until then Slip ports what it
  needs, with attribution.
