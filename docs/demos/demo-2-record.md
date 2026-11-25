# Demo 2 — the record

The record of the four runs of [demo-2.md](demo-2.md) §3, one section per
run, made with `cargo xtask corpus rename` (`spec/harness.md` §6.5). Each
run also writes `bench/results/llvm.rename.<target>.json`; this file
states what that file does not (the host, the front-end configuration,
the comparison with demo-2.md §2) and quotes what is read from it.

Every value below is to be filled by the run. Nothing here has been run.

## Common to the four runs

| Field | Value |
|---|---|
| Host: cores, memory, the container's memory limit | to be filled by the run |
| Jobserver tokens (`scripts/jobserver.sh`) | to be filled by the run |
| Corpus: name, pin, observers in the plan | to be filled by the run |
| Volume of the corpus directory (folds case or not; `inputs.respelled` seen) | to be filled by the run |

## Run 1 — §2.1, a function in a header everything includes

`llvm::getAsUnsignedInteger` to `parseAsUnsignedInteger`.

| Field | Value |
|---|---|
| Date (UTC, start and end) | to be filled by the run |
| Slip: version (`tool_slip`) and commit | to be filled by the run |
| Kiln: version (`tool_kiln`) and the pin's commit | to be filled by the run |
| Front-end configuration: the written-reference table by Kiln's dump flag or by the recording option (Kiln WP-2320) | to be filled by the run |
| Command, exactly as run | to be filled by the run |
| Exit status | to be filled by the run |
| Wall time of the whole command | to be filled by the run |

### Summary lines

As printed: `slip rename`'s `plan`, `round`, `edits`, `overlay`,
`result`, `state` and `patch` lines; the five `check` lines; `counters:`,
`result` and `corpus rename:`.

```
to be filled by the run
```

### Measurements (demo-2.md §4)

| Quantity | Value |
|---|---|
| Wall time per step (plan, map, reduce, apply, verify, scan, conclude, write) | to be filled by the run |
| Coordinator's peak resident size per step | to be filled by the run |
| Map: `peak_rss_max`, `peak_rss_sum`, workers at once, cores | to be filled by the run |
| Verify: `peak_rss_max`, `peak_rss_sum`, workers at once, cores | to be filled by the run |
| `reduce.claims`, `reduce.sites`, `reduce.edits`, `reduce.edits.shared`, `reduce.edits.max_observers`, `reduce.files`, `reduce.runs` | to be filled by the run |
| Tally of verdicts; stale observers; unused rules | to be filled by the run |
| Spelling obligations: count, passed, failed | to be filled by the run |
| Residue by category; mentions | to be filled by the run |
| Determinism: `slip reduce` again (check 2) | to be filled by the run |
| Determinism: the run repeated with `-j 4` in a fresh store (same edit set, overlay and result ids) | to be filled by the run |
| `--accept`: commands, rejected on both images, which | to be filled by the run |
| `--build`: copy, patch, configure, generate, build times; the copy's size and the free space | to be filled by the run |
| `back`: the inverse rename's state; differences from the source | to be filled by the run |

### Comparison with demo-2.md §2.1

| Expected | Found |
|---|---|
| About 28 tokens in 12 files, the sites of the table | to be filled by the run |
| `StringRef.h:34` one edit, claimed by about four thousand observers | to be filled by the run |
| Residue `not_planned`: Mips, Sparc, and four uses in `mlir`, `lldb`, `clang-tools-extra` | to be filled by the run |
| The mention at `StringRef.cpp:491` | to be filled by the run |
| State `verified_with_residue` | to be filled by the run |

`not_plain` refusals at a macro invocation (`not_plain_at_macro`): to be
filled by the run.

## Run 2 — §2.2, a variable

`llvm::TimePassesIsEnabled` to `TimePassesEnabled`.

| Field | Value |
|---|---|
| Date (UTC, start and end) | to be filled by the run |
| Slip: version (`tool_slip`) and commit | to be filled by the run |
| Kiln: version (`tool_kiln`) and the pin's commit | to be filled by the run |
| Front-end configuration: the written-reference table by Kiln's dump flag or by the recording option (Kiln WP-2320) | to be filled by the run |
| Command, exactly as run | to be filled by the run |
| Exit status | to be filled by the run |
| Wall time of the whole command | to be filled by the run |

### Summary lines

```
to be filled by the run
```

### Measurements (demo-2.md §4)

| Quantity | Value |
|---|---|
| Wall time per step (plan, map, reduce, apply, verify, scan, conclude, write) | to be filled by the run |
| Coordinator's peak resident size per step | to be filled by the run |
| Map: `peak_rss_max`, `peak_rss_sum`, workers at once, cores | to be filled by the run |
| Verify: `peak_rss_max`, `peak_rss_sum`, workers at once, cores | to be filled by the run |
| `reduce.claims`, `reduce.sites`, `reduce.edits`, `reduce.edits.shared`, `reduce.edits.max_observers`, `reduce.files`, `reduce.runs` | to be filled by the run |
| Tally of verdicts; stale observers; unused rules | to be filled by the run |
| Spelling obligations: count, passed, failed | to be filled by the run |
| Residue by category; mentions | to be filled by the run |
| Determinism: `slip reduce` again (check 2) | to be filled by the run |
| `--accept`: commands, rejected on both images, which | to be filled by the run |

### Comparison with demo-2.md §2.2

| Expected | Found |
|---|---|
| About 29 tokens in 8 files, the sites of the table | to be filled by the run |
| The second declaration in `IRReader.cpp:25` a subject, no observer stale | to be filled by the run |
| No use in another project of the checkout | to be filled by the run |
| State `verified` (or `verified_with_residue` for a token the search missed) | to be filled by the run |

`not_plain` refusals at a macro invocation (`not_plain_at_macro`): to be
filled by the run.

## Run 3 — §2.3, a rename that must be refused

`llvm::callDefaultCtor` to `makeDefaultPass`.

| Field | Value |
|---|---|
| Date (UTC, start and end) | to be filled by the run |
| Slip: version (`tool_slip`) and commit | to be filled by the run |
| Kiln: version (`tool_kiln`) and the pin's commit | to be filled by the run |
| Front-end configuration: the written-reference table by Kiln's dump flag or by the recording option (Kiln WP-2320) | to be filled by the run |
| Command, exactly as run | to be filled by the run |
| Exit status | to be filled by the run |
| Wall time of the whole command | to be filled by the run |

### Summary lines

The refusal lines of the report are quoted too.

```
to be filled by the run
```

### Measurements (demo-2.md §4)

| Quantity | Value |
|---|---|
| Wall time per step (plan, map, reduce, conclude) | to be filled by the run |
| Coordinator's peak resident size per step | to be filled by the run |
| Map: `peak_rss_max`, `peak_rss_sum`, workers at once, cores | to be filled by the run |
| `reduce.claims`, `reduce.sites`, `reduce.edits`, `reduce.files`, `reduce.runs` | to be filled by the run |
| Determinism: `slip reduce` again (check 2) | to be filled by the run |

### Comparison with demo-2.md §2.3

| Expected | Found |
|---|---|
| Five `site_refused` refusals with reason `macro_body`, each with its macro and its count of observers | to be filled by the run |
| The edit set `refused`; no overlay; no verify phase | to be filled by the run |
| State `refused`, exit status of `slip rename` 1, no patch | to be filled by the run |

`not_plain` refusals at a macro invocation (`not_plain_at_macro`): to be
filled by the run.

## Run 4 — §2.4, a rename that verification must reject

`llvm::PowerOf2Ceil` to `powerOf2Ceil`.

| Field | Value |
|---|---|
| Date (UTC, start and end) | to be filled by the run |
| Slip: version (`tool_slip`) and commit | to be filled by the run |
| Kiln: version (`tool_kiln`) and the pin's commit | to be filled by the run |
| Front-end configuration: the written-reference table by Kiln's dump flag or by the recording option (Kiln WP-2320) | to be filled by the run |
| Whether KR1 amendment 1 §2 and `docs/wp/WP-073.md` part A are in the build | to be filled by the run |
| Command, exactly as run | to be filled by the run |
| Exit status | to be filled by the run |
| Wall time of the whole command | to be filled by the run |

### Summary lines

The report's lines for the observers not verified are quoted too.

```
to be filled by the run
```

### Measurements (demo-2.md §4)

| Quantity | Value |
|---|---|
| Wall time per step (plan, map, reduce, apply, verify, scan, conclude) | to be filled by the run |
| Coordinator's peak resident size per step | to be filled by the run |
| Map: `peak_rss_max`, `peak_rss_sum`, workers at once, cores | to be filled by the run |
| Verify: `peak_rss_max`, `peak_rss_sum`, workers at once, cores | to be filled by the run |
| `reduce.claims`, `reduce.sites`, `reduce.edits`, `reduce.edits.shared`, `reduce.edits.max_observers`, `reduce.files`, `reduce.runs` | to be filled by the run |
| Tally of verdicts; stale observers; unused rules | to be filled by the run |
| Spelling obligations: count, passed, failed | to be filled by the run |
| Residue by category; mentions | to be filled by the run |
| Determinism: `slip reduce` again (check 2) | to be filled by the run |

### Comparison with demo-2.md §2.4

| Expected | Found |
|---|---|
| `MathExtrasTest.cpp:150`–`:152`: `mismatch changed`, at the string literal | to be filled by the run |
| `DenseSet.h:71`, `:76`: edited and exempted, or `unresolved` | to be filled by the run |
| State `not_verified`, no patch written | to be filled by the run |

`not_plain` refusals at a macro invocation (`not_plain_at_macro`): to be
filled by the run.

## The claims of demo-2.md §5 the runs support

For each claim of §5, whether the runs support it and by which run's
lines, measurements or result file; a claim no run supports is not
made.

| Claim (demo-2.md §5) | Supported by |
|---|---|
| Kiln resolves the post-image to the same program, for every observer that parses | to be filled by the run |
| No observer still declares the entity under its old name | to be filled by the run |
| Every rewritten reference names the entity it named before | to be filled by the run |
| The edit set is a function of the inputs (one worker or twelve) | to be filled by the run |
| Each shared-header site claimed by thousands of observers and applied once | to be filled by the run |
| Clang accepts what it accepted; builds the function's post-image; the inverse restores the tree | to be filled by the run |
| The refused target refused before anything was parsed twice, with its five tokens; the rejected target rejected by verification | to be filled by the run |

## Findings

Every prediction of demo-2.md §2 that did not hold, with what differed
(WP-079 "Tests"): to be filled by the run.
