# Demo 2 — the record

The record of the four runs of [demo-2.md](demo-2.md) §3, one section per
run, made with `cargo xtask corpus rename` (`spec/harness.md` §6.5). Each
run also writes `bench/results/llvm.rename.<target>.json`; this file
states what that file does not (the host, the front-end configuration,
the comparison with demo-2.md §2) and quotes what is read from it.

Every value marked "to be filled by the run" waits for a run that meets
demo-2.md §1. Run 1's attempt a ended `not_verified`, for two reasons
recorded under "Findings"; run 3 (attempt 3c) is accepted; runs 2 and 4
have not been made.

## Common to the four runs

| Field | Value |
|---|---|
| Host: cores, memory, the container's memory limit | attempt 1a: 24 cores; memory budget reported by `slip` 27,058,293,965 bytes (the container's limit is 28 GB) |
| Jobserver tokens (`scripts/jobserver.sh`) | attempt 1a: 6, the run at `-j 4` |
| Corpus: name, pin, observers in the plan | attempt 1a: `llvm`, `llvmorg-19.1.7`, 4,061 |
| Volume of the corpus directory (folds case or not; `inputs.respelled` seen) | attempt 1a: a read-only copy on the container's own disk (does not fold case); `inputs.respelled` not seen |

## Run 1 — §2.1, a function in a header everything includes

`llvm::getAsUnsignedInteger` to `parseAsUnsignedInteger`.

### Attempt a (2026-11-25): `not_verified`, not accepted

Started before the precondition of demo-2.md §1 held ("every `ok` unit
has digests with no indeterminate unit, or each remaining cause is
listed"): the run is recorded for what it measured and found, not as
run 1.

| Field | Value |
|---|---|
| Date (UTC, start and end) | 2026-11-25T01:58:32Z to 2026-11-25T07:31:58Z |
| Slip: version and commit | `0.0.0`, `52c1bc18` (release build; store version 1) |
| Kiln: version and the pin's commit | `1.3.0`, `4a83f6e3c` |
| Command | `SLIP_CORPUS_DIR=<copy> xtask corpus rename llvm --from llvm::getAsUnsignedInteger --to parseAsUnsignedInteger --kind function -j 4 --slip <release slip>` (no `--accept`, no `--build`) |
| Exit status | 1 (`corpus rename: FAILED (state)`) |
| Wall time of the whole command | 20,005.2 s (5 h 33 min) |

```
plan 27a4c3c6689c5049f3470838dbfc3a0fd391c5e6d57104bd84f673c5ae930cc7
observers 4061
transform e9bf07830b1f0b9d3f2fd60f768677750c222ba661a65e0f33b692c4347b35f6
round 1
map: selected 4061, ok 4059, not ok 2, failed runs 0
edits 3a6e79c0c0623ac37ad0eb39c4e0eebf4d1d6196d9282566c049bd639d8d7dbc
state accepted
edit 28 in 11 file(s); shared 4; most observers 3924; nested 0
kept 0; unresolved 0
declared 5dfadb48dbab898464821adcacd2064b4d9ba1c8d5ebb17f0402d6545544a3ae: rules 1, exempt 0
overlay 20af7b432c4372d8898a233207dcf639debe643401cb6d98bb0ff131d2f99db8: files 11, spellings 26
check state: FAIL (state not_verified, expected verified or verified_with_residue)
check determinism: not run (an earlier check failed)
check accept: not run (an earlier check failed)
check build: not run (an earlier check failed)
check back: not run (an earlier check failed)
corpus rename: FAILED (state)
```

Result id `17664749773303131137c3dc18ddfc8c36b912c6efe8aeaa1852eef3fbd6a581`.
No patch was written.

| Quantity | Value |
|---|---|
| Wall time per step | plan 0.5 s; map 4,000.0 s; reduce 1.1 s; apply 0.0 s; verify 15,995.4 s; scan 2.4 s; conclude 0.0 s; write 0.0 s |
| Coordinator's peak resident size | 47.3 MiB through verify; 124.5 MiB from scan |
| Map | `peak_rss_max` 833,961,984; `peak_rss_sum` 1,775,890,432; 4 workers at once; 24 cores; CPU 13,097.7 s |
| Verify | `peak_rss_max` 1,638,559,744; `peak_rss_sum` 3,097,112,576; 4 workers at once; 24 cores; CPU 59,708.3 s |
| `reduce.*` | claims 7,964 (all `replace`); sites 28; edits 28; shared 4; `max_observers` 3,924; files 11; `failed_runs` 0; rules 1; shards 4,059; unparsed 2 |
| Tally of verdicts; stale observers; unused rules | selected 4,061; unparsed 2; verified 63; mismatch 3,996; unchanged 0; stale 0; unused rules 0 |
| Spelling obligations | 26; passed 23; failed 3 |
| Residue by category; mentions | `not_planned` 7; `mention` 1; `dependent` 0; `not_compiled` 0 |
| `not_plain_at_macro` | 0 |

Against demo-2.md §2.1: 28 tokens in 11 files (expected about 28 in
12); the declaration in `StringRef.h` one edit claimed by 3,924
observers; residue `not_planned` at Mips (1), Sparc (2), `mlir` (1),
`lldb` (1), `clang-tools-extra` (2) and the mention at
`StringRef.cpp:491:10`, as predicted; the state `not_verified` where
`verified_with_residue` was expected. The two observers not parsed
are `clang/tools/clang-fuzzer/ClangFuzzer.cpp` and
`ClangObjectiveCFuzzer.cpp` (`unknown type name 'uint8_t'`).

### The run

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

`llvm::callDefaultCtor` to `makeDefaultPass`. Run 3c, accepted. (Run 3b,
08:44–08:55 EST the same day, is void: the corpus copy lacked the build
directories the database's entries enter, and 3,441 observers failed
with `cannot enter`; the harness still said `ok`, which is WP-169.)

| Field | Value |
|---|---|
| Date (UTC, start and end) | 2026-11-25 13:58:16 to 14:44:50 |
| Slip: version (`tool_slip`) and commit | 0.0.0; the binaries built from `3ae796f1` (no code change on `main` through `95986385`) |
| Kiln: version (`tool_kiln`) and the pin's commit | 1.3.0; `60fb5ab9d` |
| Front-end configuration: the written-reference table by Kiln's dump flag or by the recording option (Kiln WP-2320) | not recorded for this run |
| Command, exactly as run | `xtask corpus rename llvm --from llvm::callDefaultCtor --to makeDefaultPass --expect refused -j 8 --slip <target>/release/slip`, `SLIP_CORPUS_DIR=/tmp/slip-helper/corpus` |
| Exit status | 0 (`corpus rename: ok`); `slip rename` 1 |
| Wall time of the whole command | 2,794 s (46 min 34 s) |

Host for this run: 24 cores, memory budget 27,058,293,965 bytes (the
container's limit 28 GB), 16 jobserver tokens, `-j 8`; the corpus a
read-only copy on the container's disk (does not fold case).

### Summary lines

```
map: selected 4061, ok 4055, not ok 6, failed runs 0
state refused
edit 0 in 0 file(s); shared 0; most observers 0; nested 0
kept 0; unresolved 1
refused site_refused 2
refusal site_refused macro_body: llvm/include/llvm/PassSupport.h:42:32: llvm::callDefaultCtor (138 observer(s), first llvm/lib/IR/Verifier.cpp)
refusal site_refused macro_body: llvm/include/llvm/PassSupport.h:62:30: llvm::callDefaultCtor (178 observer(s), first llvm/lib/Analysis/ScalarEvolution.cpp)
```

The six not `ok` are `diagnosed`: the two of `spec/harness.md` §8.7
(`ClangFuzzer.cpp`, `ClangObjectiveCFuzzer.cpp`, `uint8_t`) and four that
clang accepts, a regression of Kiln at this pin (Findings, F-3).

### Measurements (demo-2.md §4)

| Quantity | Value |
|---|---|
| Wall time per step (the steps its transform run record has) | plan 0.55 s; map 2,762.6 s; reduce 12.5 s; conclude 0.0002 s; write 0.0001 s |
| Coordinator's peak resident size per step | 47.3 MB at every step |
| Map: `peak_rss_max`, `peak_rss_sum`, workers at once, cores | 838,889,472; 2,950,103,040; 8; 24 |
| `reduce.claims`, `reduce.sites`, `reduce.edits`, `reduce.files`, `reduce.runs` | 3,418 (refuse 316, replace 2,072, unresolved 1,030); 9 (live 6, rejected 2, unresolved 1); 0; 0; shards 4,055, unparsed 6 |
| Determinism: `slip reduce` again (check 2) | ok, the same edit set `299e9b67…`, 13.6 s |

### Comparison with demo-2.md §2.3

| Expected | Found |
|---|---|
| Five `site_refused` refusals with reason `macro_body`, each with its macro and its count of observers | Two, and two is right: demo-2.md's five was wrong (corrected there). `:42` (`INITIALIZE_PASS`, 138 observers) and `:62` (`INITIALIZE_PASS_END`, 178) are the macros LLVM expands; `:174` and `:196` are `INITIALIZE_AG_PASS` and `INITIALIZE_AG_PASS_END`, which nothing in 19.1.7 invokes, so no observer claims their bodies; `:114` is no macro body but `RegisterPass`'s constructor in a class template, whose reference names the overload set through a dependent argument: the one `unresolved` site |
| The edit set `refused`; no overlay; no verify phase | Yes: `state refused`, `overlay` null, no verify step |
| State `refused`, exit status of `slip rename` 1, no patch | Yes: status 1, "nothing was written" |

`not_plain` refusals at a macro invocation (`not_plain_at_macro`): 0.

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
(WP-079 "Tests").

**Run 1, attempt a.** The plan, the map, the reduction, the edit set
and the residue are as demo-2.md §2.1 predicts. The state is
`not_verified` for two reasons, neither a wrong answer:

1. **The precondition was not met.** 3,996 of 4,059 observers have a
   mismatch made only of `indeterminate` (and derived) findings, in the
   thousands per large unit: the base image has units whose digest is
   not determined (causes seen: `kiln.pattern_mem_initializers`,
   `spelling.fold_operator`, `spelling.friend_name`,
   `spelling.destructor_name`, and units with errors). A unit that is
   indeterminate before the change cannot be compared after it, and
   Slip says so. `verified` over the whole corpus waits for the
   packages that close those causes (`docs/wp/WP-163.md`,
   `docs/wp/WP-156.md`, `docs/wp/WP-143.md`, `docs/wp/WP-138.md`,
   `docs/wp/WP-154.md`).
2. **One observer where the declared rule is not applied.** In
   `llvm/lib/Target/AArch64/AArch64RegisterInfo.cpp` the three spelling
   obligations fail with `denotes_other` (`StringRef.h:467:13`,
   `MIRYamlMapping.h:154:9` and `:170:9`: "meant 3fab1885d5fb1942,
   found llvm::parseAsUnsignedInteger(...)"). That unit first reaches
   `StringRef.h` through
   `build/lib/Target/AArch64/AArch64GenRegisterInfo.inc`, a generated
   file outside the owned root; by the per-inclusion rule of
   `spec/verify.md` §2.1 the header is then read-only in that unit, the
   function is a dependency's entity there, and the rule is not
   applied to it. The answer is loud (the rule is only ever withheld)
   and is the specification as written; it is wrong for every code
   base whose build generates files that include owned headers. The
   package is `docs/wp/WP-118.md` (a `rebuilt` class of file).

Not a prediction of §2 but found by the attempt: at `-j 4` the verify
phase took 4.4 hours against 67 minutes for the map phase.

**Run 3.** The refusal, its reason and its state are as predicted; the
count of refusals is not, and the prediction was wrong:

- **F-1, demo-2.md §2.3 said five refusals.** Two of the five lines it
  named are in macros LLVM 19.1.7 never invokes (`INITIALIZE_AG_PASS`,
  `INITIALIZE_AG_PASS_END`), and one (`:114`) is not in a macro at all
  (`RegisterPass`'s constructor). Two is right. demo-2.md §2.3 is
  corrected.
- **F-2, a void attempt (3b) passed the harness.** With 3,441 observers
  unable to enter their directory, `corpus rename` said `ok`: it passes
  `--allow-unparsed` and checks nothing of the map's outcomes. The
  package is `docs/wp/WP-169.md` (a `coverage` check, and a check of the
  database's directories before `slip` runs).
- **F-3, Kiln at `60fb5ab9d` rejects four units clang accepts**, with
  "default member initializer for '…' needed within definition of
  enclosing class '…' outside of member functions":
  `clang/lib/Sema/SemaOpenMP.cpp:109:10`,
  `llvm/lib/Bitcode/Writer/ValueEnumerator.h:101:5` (two units) and
  `llvm/lib/Transforms/Utils/ValueMapper.cpp:194:10`. With Kiln at
  `4a83f6e3c` (run 1a) they parsed. Reported to Kiln in the supervisor's
  held batch. The rename here is refused either way; runs 1 and 2 cannot
  be `verified` over the whole corpus until those units parse.
