# 02 — The pipeline

```
plan ─▶ map (per observer) ─▶ reduce (per file) ─▶ apply ─▶ verify (per observer) ─▶ patch + report
                 ▲                                                    │
                 └──────────── repair (affected observers only) ◀─────┘
```

An **observer** is one translation unit under one build configuration. Every
phase except apply is parallel; apply is I/O-bound and small.

## 1. Inputs and the plan

| Input | Form |
|---|---|
| Build description | One or more compilation databases. `compile_commands.json` (CMake, Ninja `-t compdb`, Bear, Bazel extractors) first; the reader is a trait, so other formats are adapters. Several databases = several configurations (debug/release, per platform, per feature set). |
| Synthesised TUs | For header-only code with no TU of its own (libc++ is the standing example): a list of headers plus flags, each compiled as a one-include TU, and/or the project's test TUs. |
| Ownership | Path globs (or a manifest) naming the files Slip may edit. Everything else is read-only: system headers, dependencies, generated files. |
| Transform | One or more transform source files, parameters (`-D`-style key/values readable through `slip::param`). |

The plan phase:

1. Normalises each command to a front-end invocation (strips output and
   codegen-only flags, expands response files, resolves the working directory).
   A flag Kiln does not understand fails that observer in the plan, loudly, not
   in the middle of a run.
2. Deduplicates observers with identical normalised commands.
3. Snapshots file identity: canonical path and content digest for every file as
   it is first read. All later phases refer to files by (path, digest); a file
   that changes during a run is detected and the run stops.
4. Orders observers largest-first using recorded costs from a previous run when
   available (file size otherwise), and writes the plan as a file. Every later
   phase is a pure function of the plan and the content-addressed store, which
   is what makes runs resumable and, later, distributable.

## 2. Map

Per observer, in a worker:

1. **Parse** the subject with Kiln's front end, with Slip's recorders attached
   to the preprocessor and to semantic analysis ([05](05-kiln-interface.md)).
2. **Extract the subject model**: entity table with stable keys; declaration
   sites; written references with their referents and lookup kind; expression
   and statement trees (Kiln's arenas, retained); the preprocessing record
   (inclusions, macro definitions, expansions with argument ranges, conditional
   regions taken and skipped); the token-origin map.
3. **Pre-digest**: the semantic digest of every top-level declaration
   ([03](03-equivalence.md) §3).
4. **Run the transform** against the model. Native selectors and Level A
   transforms run in Rust; rule actions and free-form code run in the Oven.
5. **Write the shard**: edits, facts, reports, digests, the coverage record
   (which bytes of which files this observer compiled) and cost counters.

Then the worker drops the TU and takes the next. Nothing about a TU outlives
its shard, which is what bounds memory.

### What an edit record carries

| Field | Purpose |
|---|---|
| File (path, content digest), byte range, replacement | The edit. Replacement is a `code` value, still symbolic: text, entity keys to spell, ranges to copy. |
| **Site key** | Identity of the *intent*: transform, rule, the anchor node's spelling range, a slot number. Two observers proposing the same change produce the same site key. Text equality alone is not edit identity: two different rules may insert the same text at one offset, and both must apply. |
| Observer, include context, instantiation context | Who proposed it and under which view of the file. |
| Guard, atomic group, order | See [01](01-transform-api.md) §7. |
| Origin tier | File text, macro argument, macro body ([04](04-macros-and-configurations.md)). |
| Obligations | What verification must show at this site (`resolves_to(key)`, or the default: enclosing declaration's digest preserved). |
| Spelling candidates | For each entity to be spelled: the shortest qualification valid in this observer's context, per guard outcome it depends on. |

## 3. Reduce

Input: all shards. Output: one edit set and one report. Partitioned by file
and parallel; streaming (external merge by file and offset), so it does not
hold all shards in memory.

1. **Fold facts** with their declared monoids. Associative and commutative, so
   shard order cannot matter.
2. **Evaluate guards**; drop edits whose guard is false.
3. **Normalise and deduplicate** by (site key, range, replacement).
4. **Unanimity.** For each site, compare the observers that proposed the edit
   with the observers that *compiled those bytes* (from coverage records). See
   §4.
5. **Finalise spellings.** For each entity to spell, take the longest of the
   observers' shortest valid qualifications, so the spelling is valid in every
   context that sees it; fully qualified is the top of that order.
6. **Geometry.** Overlapping edits from different sites conflict unless one
   contains the other's bytes verbatim through `copy` (then they nest). Equal-
   offset insertions are ordered by `order`, then site key.
7. **Atomic groups.** A group with any rejected member is rejected whole;
   iterate to a fixed point, since rejecting a group can un-conflict another.
8. **Emit** the surviving edits sorted by (file, offset), plus a rejection list
   in which every entry names its cause and the observers involved.

The reducer is deterministic: the output is a function of the *set* of shards.
This is a property test, not a hope ([06](06-engineering-plan.md) §3).

## 4. Shared headers

A header included by 5,000 TUs is seen by 5,000 observers, each of which will
propose edits to it. Three things can go wrong, and the usual design (each TU
emits edits; the tool takes the union and deduplicates) handles only the first:

| Hazard | Example | Handled by |
|---|---|---|
| Duplicates | Every TU proposes the same rename in `vector.h` | Deduplication by site key |
| Disagreement | In a template, `f(x)` resolves to `legacy::f` in one TU's instantiations and to `dep::f` in another's; they propose different edits | Conflict at one site key → rejected and reported with both observers |
| **Silent disagreement** | 4,999 TUs propose an edit; one TU compiles the same bytes and proposes *nothing*, because there the name means something else | Unanimity over coverage; and verification |

Silent disagreement is why union-of-edits is unsound: an abstention is
invisible in a set of edits. Slip makes it visible. Each observer's coverage
record lists the byte ranges of each owned file it actually compiled (not
skipped by the preprocessor, not an uninstantiated template body it had no
view on). For each proposed edit the reducer knows the observers that compiled
those bytes; if some of them did not propose it, the edit is *contested*.
Coverage records are interval lists and nearly all observers of a header share
one, so they are stored once by digest.

Policy for a contested edit is per transform: `unanimous` (default: reject and
report), or `any` (apply, and let verification decide). Either way
**verification is the backstop**: every observer of the header re-parses the
post-image and its digests must hold. Unanimity is the cheap early check that
gives a precise diagnostic before the expensive one; the correctness argument
does not depend on it.

The same mechanism covers one file included several times in one TU with
different macro state (X-macro headers, `.def` files): each inclusion is its
own include context and votes separately.

## 5. Apply

- Edits are applied to an **overlay** (an in-memory file system layered over
  the snapshot), never directly to the working tree. Verification reads the
  overlay.
- Byte-exact: untouched bytes are preserved, including line endings, BOM,
  trailing whitespace and encoding. Slip does not reformat.
- The run's product is a patch (unified diff, or the overlay written out) plus
  the report. Writing to the working tree is a separate, explicit step that
  first re-checks every file's content digest against the plan.

## 6. Verify and repair

Every observer that includes a changed file is re-parsed against the overlay,
its digests are computed and compared with the pre-digests under the declared
changes. [03](03-equivalence.md) defines the comparison.

Outcomes per observer: *verified*; *mismatch* (with the declaration and the
first differing reference localised); *failed to parse* (new diagnostics).

On mismatch, Slip runs the transform's `repair` entry point if it has one (the
L2 refactorings do), in the mismatching observer, with the mismatch as input:
which written reference now resolves to what, and what it resolved to before.
Repair emits further edits, typically "qualify this name explicitly". Those go
through reduce and apply, and only affected observers are re-verified. The
loop is bounded (default four rounds).

The end state is one of:

- **Verified**: every observer's digests hold. Exit 0.
- **Verified with residue**: every *applied* edit is verified, and the report
  lists edits that were rejected, with causes. The codebase is in a consistent
  state but the transformation is incomplete. Exit code distinct from 0.
- **Not verified**: some observers still mismatch. Slip emits the patch only on
  request, with the list of unverified sites. For transformations that cannot
  be partially applied (a namespace rename is one), this is the normal way a
  first run on a new codebase ends, and the report is the work list.

## 7. Removing redundant work

The floor for a verified run is two front-end passes over the codebase (map
and verify). Kiln parses at about clang's speed, so the planning number is
"two to three times the front-end share of a full build, in CPU". That is
measured, per phase, from the first milestone on. What can be saved on top:

- **Shard cache.** A shard is keyed by (normalised command, digests of every
  file the TU read, transform digest, parameters, Slip version). Unchanged
  observers are not re-run. Iterating on a transform against an unchanged
  tree still pays the transform, but:
- **Model cache** (later). The subject model is serialisable and
  transform-independent; with it cached, a new transform runs without parsing.
  Static queries are specified to be answerable from a stored model for this
  reason; probes are not.
- **Transform memoisation** (later). Rule actions on a header's declarations
  repeat across observers with identical results. A rule action's only inputs
  are host-call results, so Slip can record the trace (calls and result
  digests, in terms of stable keys, not TU-local ids) with the action's
  outputs. Another observer replays the queries natively against its own
  model; if every result digest matches, the recorded outputs are reused
  without running the Oven. Sound by construction, with no reasoning about
  "the same context" needed. Level A needs none of this; Level C opts out.
- **Verify pruning.** An observer none of whose input files changed is verified
  trivially. For repair rounds this is most observers.

What cannot be shared is parsing itself: a header's meaning depends on what
preceded it, and Slip does not attempt semantic reuse across TUs.

## 8. Execution: cores and memory

- **A coordinator and worker processes.** Workers are long-lived, take one
  observer at a time, and run each on a large-stack thread as Kiln's driver
  does. Processes rather than threads because a worker's memory is returned to
  the OS when it recycles, a panic or stack overflow on a pathological TU
  costs one observer (caught, retried once, then reported as failed), and
  per-observer peak RSS is measurable exactly. Reduce uses threads.
- **Jobserver.** Slip is a make-jobserver client when one is present (it
  shares the machine with builds and other tools) and otherwise sizes itself
  from the available cores.
- **Memory-aware admission.** Core count is not the limit on big machines;
  memory is. The coordinator admits an observer only if its estimated peak
  (from the previous run or phase; file-size heuristic the first time) fits in
  the budget (cgroup limit or `--memory`). Largest-first ordering keeps the
  tail short. A measured peak above estimate updates the estimate.
- **Scaling expectation.** Map and verify are embarrassingly parallel; reduce
  is partitioned by file. The serial part is plan and apply. The harness runs
  core-scaling sweeps (1, 2, 4, … cores) on the corpora and tracks the curve.
- **Distribution** (later). Shards are files and the plan is a file; map and
  verify can run on other machines with a shared store. Nothing in the design
  assumes one host, and nothing is built for more than one until measurements
  ask for it.

## 9. Failures and accounting

Every byte of owned code ends a run in exactly one state, and the report
totals them:

| State | Meaning |
|---|---|
| Covered, unchanged | Compiled by at least one observer; no edit proposed |
| Covered, changed, verified | |
| Covered, edit rejected | With cause: contested, conflict, macro tier, guard false, group |
| Uncovered | Compiled by no observer: inactive conditional branch, uninstantiated template body, macro never expanded, file in no TU. Listed with the reason. |
| Observer failed | Kiln diagnostics, crash, transform threw, budget exceeded. Files covered only by failed observers become uncovered. |

An observer that fails to parse *before* the transformation is a pre-existing
condition and is reported as such; the run continues without its vote unless
`--require-all-observers` is set.

## 10. Several transforms

Transforms in one run all see the same pre-image and cannot see each other's
edits; the reducer treats overlaps between them as conflicts. Sequential
composition (the second transform sees the first one's result) is two runs,
each verified, with the first run's overlay as the second's snapshot. The
shard cache makes the second parse the only repeated cost.
