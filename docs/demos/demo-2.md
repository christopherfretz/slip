# Demo 2 — a native rename across LLVM

The exit of milestone M2
([06-engineering-plan.md](../design/06-engineering-plan.md) §5): a native
rename of a function and of a variable, one of them declared in a header
that nearly every translation unit includes, across the `llvm` corpus
(LLVM and clang 19.1.7, 4061 observers, `spec/harness.md` §8.7), in
parallel; the post-image verifies and clang builds it. Specified by
[spec/edits.md](../../spec/edits.md); run by `docs/wp/WP-079.md`.

Line numbers below are in the corpus's sources at `llvmorg-19.1.7`,
relative to its source directory. Counts of tokens are from a textual
search of `llvm/` and `clang/` and are what to expect, not what the run
is checked against: the run's own counters are the record.

## 1. What must be in place

- Every package of `docs/wp/WP-060.md` to `WP-078.md` except `WP-073.md`
  (`WP-079.md` lists what each gives the demonstration), and three
  packages of M3 that the rename and its verification read through:
  `WP-084.md` (file ranges), `WP-087.md` (keys and rename rules) and
  `WP-096.md` (the check of spelled entities, `spec/verify.md` §10).
  **Decided:** M2's exit runs through these three as they are. M2 does
  not carry smaller versions of its own, so the native rename and a
  rename written in C++ read one implementation of ranges, of keys and
  of the check.
- Slip's Kiln pin at Kiln's main (`docs/wp/WP-044.md`): the front-end
  entry (KF1, Kiln WP-2120); the written-reference table stages 1 and 2
  (Kiln WP-1930, WP-1931) and the declaration extents (Kiln WP-2090);
  the fixes of KD1 and KD2 (so that no unit of the corpus is `failed`).
  Then the pin move after WP-044 (`docs/wp/WP-101.md`), to Kiln `main`
  at or after `913a490bb`: Kiln WP-2150, which delivered
  [KF3](../requests/KF3-location-utilities.md) stage 1 (the end of a
  token, a location range as file characters, the expansion entries).
  `docs/wp/WP-084.md` and `docs/wp/WP-068.md` call those functions and
  compute no range of their own. Beyond what Kiln's main has today:
  KF2 stage 1 (the overlay).
- **Not a prerequisite, decided:** Kiln WP-2320 (an option of the entry
  that records the written-reference table without printing it,
  [KR1 amendment 1](../requests/KR1-amendment-1.md) §1). The
  demonstration runs before it. Until it lands, every parse that needs
  the table is given Kiln's dump flag and the dump is discarded
  (`docs/wp/WP-067.md`), so the map and verify phases are measured with
  a front end that also formats a dump nobody reads. The record of the
  demonstration states which front-end configuration each run used (the
  Kiln commit, and whether the table was switched on by the dump flag
  or by the recording option), and its measurements are read with that
  beside them (§4). If WP-2320 lands first, the runs use it and the
  record says so; the runs are not repeated only to have both.
- M1 closed for the corpus: every `ok` unit has digests with no
  indeterminate unit, or each remaining cause is listed — checked before
  a `verified` or `--accept` run by `cargo xtask census <corpus>`
  (`spec/harness.md` §9.5).

## 2. The targets

### 2.1 A function in a header everything includes

`llvm::getAsUnsignedInteger` to `llvm::parseAsUnsignedInteger`.

| Where | What |
|---|---|
| `llvm/include/llvm/ADT/StringRef.h:34` | The declaration. `StringRef.h` is read by nearly every observer, so this one site is claimed some four thousand times and is one edit |
| `llvm/include/llvm/ADT/StringRef.h:467` | A use inside the member template `StringRef::getAsInteger`; the arguments are not dependent, so the name resolves in the pattern |
| `llvm/lib/Support/StringRef.cpp:486` | The definition, with a qualified declarator-id (`llvm::getAsUnsignedInteger`): only the last name changes |
| `llvm/include/llvm/CodeGen/MIRYamlMapping.h:154`, `:170` | Uses in a second shared header |
| `llvm/lib/Support/YAMLTraits.cpp` (8), `llvm/lib/CodeGen/BasicBlockSectionsProfileReader.cpp` (5), `llvm/lib/TextAPI/PackedVersion.cpp` (4), `llvm/lib/ObjectYAML/` (2), `llvm/lib/Support/UnicodeNameToCodepoint.cpp` (1), `llvm/utils/UnicodeData/UnicodeNameMappingGenerator.cpp` (1), `clang/lib/Driver/ToolChains/HLSL.cpp` (2, qualified) | Uses in source files |

About 28 tokens in 12 files. Expected beside them:

- **Residue `not_planned`**: `llvm/lib/Target/Mips/MipsISelLowering.cpp:4049`
  and `llvm/lib/Target/Sparc/SparcISelLowering.cpp:3543`, `:3559` (the
  corpus configures X86 and AArch64 only), and four uses in `mlir`, `lldb`
  and `clang-tools-extra` (not among the corpus's projects). No observer
  compiles them; the run lists them and does not edit them.
- **A mention**: the comment at `llvm/lib/Support/StringRef.cpp:491`.
- Expected state: `verified_with_residue`.

Why this one: the declaration is in the most shared header there is, the
uses are plain, and no use is inside a macro argument that is stringised
(§2.4 shows what happens when one is).

### 2.2 A variable

`llvm::TimePassesIsEnabled` to `llvm::TimePassesEnabled`.

| Where | What |
|---|---|
| `llvm/include/llvm/Pass.h:338` | `extern bool TimePassesIsEnabled;` in a header every legacy pass includes |
| `llvm/lib/IRReader/IRReader.cpp:25` | **A second declaration**, `namespace llvm { extern bool TimePassesIsEnabled; }`, in a file that does not take it from `Pass.h`. It is the same entity by key, so it is a subject in that observer too; were it missed, `spec/verify.md` §9 reports the observer as stale |
| `llvm/lib/IR/PassTimingInfo.cpp:37` | The definition; `:41`, `:106`, `:201` uses; `:47` a use inside a lambda |
| `llvm/lib/CodeGen/SelectionDAG/SelectionDAGISel.cpp` (12), `llvm/lib/CodeGen/RegAllocGreedy.cpp` (4), `llvm/lib/CodeGen/RegAllocBase.cpp` (1), `llvm/lib/IRReader/IRReader.cpp:74` | Uses as an argument |
| `clang/lib/CodeGen/CodeGenAction.cpp:129`, `:153` | Qualified uses, assigned to |
| `llvm/unittests/IR/TimePassesTest.cpp:81`, `:115` | Assignments in a test body, outside any macro argument |

About 29 tokens in 8 files; no use in another project of the checkout.
Expected state: `verified`, or `verified_with_residue` if the scan finds a
token the search did not.

### 2.3 A rename that must be refused

`llvm::callDefaultCtor` to `llvm::makeDefaultPass`.

The two function templates are declared at
`llvm/include/llvm/PassSupport.h:84` and `:91`. Every reference to them
in the tree is written in the replacement list of a macro of the same
header (`:42`, `:62`, `:114`, `:174`, `:196`: `INITIALIZE_PASS`,
`INITIALIZE_PASS_END` and their relatives), as
`PassInfo::NormalCtor_t(callDefaultCtor<passName>)`. Those tokens are
tier 2 (`spec/edits.md` §2.2). M2 edits no macro body, and renaming the
declarations alone would break every pass registration in LLVM.

Of those five, only the bodies of `INITIALIZE_PASS` (`:42`) and
`INITIALIZE_PASS_END` (`:62`) are expanded in LLVM 19.1.7:
`INITIALIZE_AG_PASS` and `INITIALIZE_AG_PASS_END` (`:174`, `:196`) are
invoked nowhere, and `:114` is not a macro body but the constructor of
the class template `RegisterPass`, whose reference names the overload
set through a dependent argument (an `unresolved` site). (Corrected after
run 3; the first text expected five refusals.)

Expected: each expanded body token is a `refuse` claim with reason
`macro_body`, from every observer that expands the macro; reduce turns
them into two `site_refused` refusals, each one line of the report with
its macro and its count of observers; the edit set is `refused`; there is
no overlay and no verify phase; the state is `refused`, exit status 1,
and nothing is written.

### 2.4 A rename that verification must reject

`llvm::PowerOf2Ceil` to `llvm::powerOf2Ceil`
(`llvm/include/llvm/Support/MathExtras.h:394`). Recorded as an expected
`not_verified`, to be turned into an accepted target when M4 lands.

- `llvm/unittests/Support/MathExtrasTest.cpp:150` to `:152` use the
  function inside `EXPECT_EQ(…)`. Googletest stringises its arguments, so
  the test body holds the string `"PowerOf2Ceil(8U)"`, which the rename
  changes. The unit of that test body differs and is not exempt:
  `mismatch changed`, localised at the string literal.
- `llvm/include/llvm/ADT/DenseSet.h:71` and `:76` call it with a dependent
  argument inside a class template: the dependent callee of
  `spec/edits.md` §9.4. With the join of
  [KR1 amendment 1](../requests/KR1-amendment-1.md) §2 in the pin and
  `docs/wp/WP-073.md` part A the name is edited and its pattern
  exempted; without them the name is `unresolved`, and every observer
  that instantiates `DenseSet` fails to parse the post-image.

Either way the run must end `not_verified` and write nothing. The point
of the exhibit is that the pipeline did produce edits that M2 cannot
justify, and the trusted base said no.

## 3. The commands

```
cargo xtask corpus configure llvm            # once; the corpus as fetched and generated
cargo xtask census llvm -j 12                # check the precondition (§1; also run automatically by --accept / --expect verified)

cargo xtask corpus rename llvm --from llvm::getAsUnsignedInteger --to parseAsUnsignedInteger \
      --kind function --accept --build -j 12
cargo xtask corpus rename llvm --from llvm::TimePassesIsEnabled --to TimePassesEnabled \
      --kind variable --accept -j 12
cargo xtask corpus rename llvm --from llvm::callDefaultCtor --to makeDefaultPass \
      --expect refused -j 12
cargo xtask corpus rename llvm --from llvm::PowerOf2Ceil --to powerOf2Ceil \
      --expect not_verified -j 12
```

Each is (`spec/harness.md` §6.5) one
`slip rename --from … --to … --db <the corpus's database> --owned <its
source directory> --allow-unparsed --allow-residue --patch <file>
--relative-to <its source directory>` (`spec/exec.md` §8.11,
`spec/edits.md` §10.1), followed by the harness's checks. The same run
can be made a stage at a time (`slip map`, `slip reduce`, `slip check`,
`slip apply`, `spec/edits.md` §10.2); the harness's determinism check
is `slip reduce` run again. `--owned` is the whole
checkout, so that uses in projects the corpus does not configure are
residue rather than unseen. `--allow-unparsed` is for the two units of
`clang/tools/clang-fuzzer` that neither Kiln nor clang accepts
(`spec/harness.md` §8.7).

The first run is given `--build`: the patch is applied to a copy of the
checkout, which is configured as the corpus is and built with clang, and
the inverse rename is run on the copy and must restore it. The second is
given `--accept` only: clang parses every command of the plan through
the overlay.

All runs go through the shared jobserver, as every build of this
workspace does.

## 4. What is measured

Written to `bench/results/llvm.rename.<target>.json` by each run:

| Quantity | From | Looked for |
|---|---|---|
| Wall time of each step: map, reduce, apply, verify, scan | the transform run record (`spec/edits.md` §10.6) | map and verify each about one digest pass of the corpus; reduce, apply and scan in seconds |
| Peak resident size of the coordinator per step | the same | reduce flat in the number of observers (`spec/edits.md` §5.6) |
| `peak_rss_max`, `peak_rss_sum`, workers at once, for map and verify | the two phase run records | as the M1 digest pass |
| `reduce.claims`, `reduce.sites`, `reduce.edits`, `reduce.edits.shared`, `reduce.edits.max_observers`, `reduce.files`, `reduce.runs` | the edit set | `max_observers` near the number of observers that read `StringRef.h`; `claims` in the tens of thousands collapsing to under thirty edits |
| The tally of verdicts; stale observers; unused rules | the result | every `ok` observer `verified`; none stale; none unused |
| Spelling obligations: how many, passed, failed | the result (`spec/verify.md` §10) | one per replaced reference (about 26 for each target: every token but the declarations'); all passed |
| Residue by category; mentions | the result | the lists of §2 |
| Determinism | `reduce` run twice; the whole first run repeated once with `-j 4` | the same edit set, overlay and result ids |

The counters are exact and are compared with the committed file on later
runs. Times and sizes are recorded with the host's core count and with
the front-end configuration of §1 (the Kiln commit, and how the
written-reference table was switched on): a map or verify time measured
with the dump flag is not compared with one measured after Kiln WP-2320
without saying so.

## 5. What the demonstration may claim

It may claim, for the two accepted targets:

- For every one of the plan's observers that parses (4059 of 4061, once
  KD1 and KD2 are in the pin), Kiln resolves the post-image to the same
  program as the pre-image, with the one entity renamed: every
  declaration, every call, every overload chosen, every instantiation
  made, every constant evaluated, by the equality of digests
  (`spec/digest.md`) under one declared rule.
- No observer of the plan still declares the entity under its old name
  (`spec/verify.md` §9).
- Every reference the rename rewrote names, in the post-image, the
  entity it named before, as some observer of the plan resolves it
  (`spec/verify.md` §10). This holds also for a reference inside a unit
  that a transform declared changed, though no target here has one.
- The edit set is a function of the inputs: the same bytes from one
  worker or twelve.
- Each shared-header site was claimed by thousands of observers and
  applied once.
- Clang accepts every translation unit of the post-image that it accepts
  of the pre-image; and, for the function, clang builds the post-image
  and the inverse rename restores the tree byte for byte.
- The refused target was refused before anything was parsed twice, with
  the five tokens that caused it; the rejected target was rejected by
  verification.

It may not claim:

- Anything about code no observer compiles. The Mips and Sparc back ends,
  `mlir`, `lldb` and `clang-tools-extra` still use the old name of the
  function; the run lists those lines and edits none. A build with
  assertions on, or with `EXPENSIVE_CHECKS`, is another configuration and
  was not planned.
- That comments, strings and documentation were updated.
- That Kiln's reading of either image is clang's. Both images are
  resolved by Kiln; clang's part here is to accept and build the result,
  not to compare it.
- That a rename in general works: members of class templates, names in
  macro bodies, names in stringised arguments and names found only at
  instantiation are refused, rejected or reported as residue, as §2.3 and
  §2.4 show.
- Performance beyond this host, this corpus and these targets.

## 6. The packages

`docs/wp/WP-060.md` to `WP-079.md`. A package can start when every
package it needs, all in earlier waves, has merged; packages of one wave
share no file.

| Wave | Packages | Waits for, outside M2's packages |
|---|---|---|
| 1 | 060 (model: request, shard sections), 064 (protocol 2, the store's files) | nothing |
| 2 | 061 (model: edit set, overlay, result), 069 (rule use), 067 (written references, declaration names) | 067: the pin at Kiln's main (WP-044); 069: `slip_digest`'s `lib.rs` and `keys.rs` free of M1's open packages |
| 3 | 062 (reduce), 063 (apply), 077 (test runner), 070 (agreement, spellings, guards, state), 066 (the overlay), 068 (sites, scan) | 070: M1's run report (WP-055); 066: Kiln's answer to KF2, then its review; 068: M3's file ranges (WP-084), which wait for the pin move after WP-044 (WP-101; Kiln WP-2150); 068's `scan` and `tree` do not |
| 4 | 071 (rename, core), 075 (`reduce`, `apply`) | 071: M3's keys (WP-087); 075: M1's commands (WP-053) |
| 5 | 072 (families, followers), 073 (templates), 074 (worker) | 074: M1's map and verify tasks (WP-050, WP-052) and M3's check of spelled entities (WP-096); 073: KR1 amendment 1 §2 and Kiln WP-1933, part by part |
| 6 | 076 (`map`, `check`, `rename`) | |
| 7 | 078 (the rename tests) | |
| 8 | 079 (corpus, this demonstration) | M1 closed for the corpus. Not Kiln WP-2320 (§1) |

The critical path is the one through Kiln's tables: WP-044, 067, the
pin move after WP-044, WP-084, 068, WP-087, 071, WP-096, 074, 076, 078,
079, with the overlay
(KF2, 066) joining it at 074. The packages that need neither Kiln nor an
open package of M1 (060, 061, 062, 063, 064, 069, 077) can be done
before that path reaches 071. 073 is on no path to this demonstration's
two accepted targets. Three packages of M3 (WP-084, WP-087, WP-096) are
on M2's path, by decision (§1): the rename reads the subject through
the interfaces a programmable transform will, and its replaced
references are checked as a script's are.
