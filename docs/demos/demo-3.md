# Demo 3 — a transform written in C++, run across LLVM

The exit of milestone M3
([06-engineering-plan.md](../design/06-engineering-plan.md) §5): example
9.1 of [01-transform-api.md](../design/01-transform-api.md), written as a
transform against `<slip/meta.h>` version 0
([spec/api.md](../../spec/api.md) §10.1), run on the `llvm` corpus (LLVM
and clang 19.1.7, 4061 observers, `spec/harness.md` §8.7) against real
pairs of functions, in parallel over every translation unit; the edits
reduced, applied and verified. Run by `docs/wp/WP-094.md`.

Line numbers are in the corpus's sources at `llvmorg-19.1.7`, relative to
its source directory. Counts are from a textual search and are what to
expect, not what the run is checked against: the run's own counters are
the record.

## 1. What must be in place

- Demo 2 has run: plan, map, reduce, apply and verify work for a native
  rename on this corpus.
- `docs/wp/WP-080.md` to `WP-093.md` and `WP-095.md`.
- `spec/edits.md` carries what `spec/api.md` §7.6 hands over, and reduce
  and apply implement it: groups, declared changes with their places,
  reports, and either composed copies or their rejection.
- Slip's Kiln pin has: the host call and the evaluation session (Kiln
  WP-1990, WP-1995); the written-reference table stages 1 and 2 with a
  way to switch it on through the front-end entry
  (`docs/requests/KR1-amendment-1.md` §1); a handler that may borrow the
  subject (`docs/requests/KS1-amendment-1.md`, unless WP-080 finds it is
  not needed).

## 2. Part A: exchange the arguments of four comparisons

`llvm::KnownBits` has eight static comparisons. Four are defined as the
other four with the arguments exchanged
(`llvm/lib/Support/KnownBits.cpp:518-548`):

```cpp
std::optional<bool> KnownBits::ult(const KnownBits &LHS, const KnownBits &RHS) { return ugt(RHS, LHS); }
std::optional<bool> KnownBits::ule(const KnownBits &LHS, const KnownBits &RHS) { return uge(RHS, LHS); }
std::optional<bool> KnownBits::slt(const KnownBits &LHS, const KnownBits &RHS) { return sgt(RHS, LHS); }
std::optional<bool> KnownBits::sle(const KnownBits &LHS, const KnownBits &RHS) { return sge(RHS, LHS); }
```

So `KnownBits::ult(a, b)` and `KnownBits::ugt(b, a)` are the same
computation, and the rewrite of example 9.1 is behaviour-preserving for
these four pairs by the library's own definition. That is why this pair
was chosen: the demonstration can be checked a third way, by running
LLVM's own unit test (§5).

The transform is `resource/examples/swap_arguments.cpp`, unchanged, with

```
-DSWAP_PAIRS='{"llvm::KnownBits::ult","llvm::KnownBits::ugt"},{"llvm::KnownBits::ule","llvm::KnownBits::uge"},{"llvm::KnownBits::slt","llvm::KnownBits::sgt"},{"llvm::KnownBits::sle","llvm::KnownBits::sge"}'
```

and the corpus's source directory owned.

| Where | What |
|---|---|
| `llvm/include/llvm/Support/KnownBits.h:421-442` | the eight declarations. 77 files of `llvm/` and `clang/` include `KnownBits.h` directly and many more through other headers; in every observer that reads it, the transform's `lookup` finds the functions and `calls_to` is asked |
| `llvm/lib/CodeGen/SelectionDAG/TargetLowering.cpp:2263`, `:2265`, `:2277`, `:2279` | four calls in `TargetLowering::SimplifyDemandedBits` |
| `llvm/lib/CodeGen/GlobalISel/CombinerHelper.cpp:4388`, `:4391`, `:4400`, `:4403` | four calls |
| `llvm/unittests/Support/KnownBitsTest.cpp:611`, `:612`, `:615`, `:616` | four calls, inside a lambda inside a `TEST` body |
| `llvm/lib/Support/KnownBits.cpp:518`, `:522`, `:542`, `:546` | the four definitions: names that are not calls. The transform reports nothing for them (a declaration is not a reference) and leaves them |

What makes it a test of meaning and not of spelling: `llvm::APInt` has
member functions `ult`, `ule`, `slt`, `sle` with one parameter, called as
`A.ult(B)` in hundreds of places (the definitions above use them eight
lines away). They are different entities; no call of them may be touched,
and none is, because `calls_to` is asked for an entity.

**Expected.** Twelve groups of three edits (the callee's name, each
argument), thirty-six edits, in three files, none in a header, so no edit
is claimed by more than one observer. Declared changed, each at the place
of its call: the unit of `TargetLowering::SimplifyDemandedBits`, the unit
or units that hold the calls in `CombinerHelper.cpp`, and the unit of the
test body. No report: every call has two arguments and is plain text.
Every one of the 4061 observers runs the transform; all but three find no
call.

Each callee edit replaces only the name (`KnownBits::ult` becomes
`KnownBits::ugt`): `spell` uses the sibling rule of `spec/api.md` §7.2
because the two functions have the same parent and the qualifier was
written.

## 3. Part B: the native rename, as a C++ program

Part A's twelve sites are all in source files. To show the property the
pipeline exists for, an edit in a header that thousands of observers
claim, the second example is run too:
`resource/examples/rename_function.cpp` with Demo 2's first target
(`docs/demos/demo-2.md` §2.1, written by milestone M2):

```
-DRENAME_FROM='"llvm::getAsUnsignedInteger"' -DRENAME_TO='"parseAsUnsignedInteger"'
```

The check is a differential: **the edit set and the rules of the C++
transform must equal those of the native rename**, edit for edit. Where
they differ, one of the two is wrong or the API cannot yet say what the
native code knows; each difference is listed with its cause. Expected
differences, from the two specifications: the native rename makes `keep`
and `unresolved` claims and follows linked entities; the transform makes
neither, so the comparison is of accepted edits and rules only.

### 3.1 A target that shows the limits (run and reported, not gated)

`llvm::isAligned(Align, uint64_t)`
(`llvm/include/llvm/Support/Alignment.h:145`) renamed to `isSizeAligned`
(which does not occur in the tree). A textual search of `llvm/` and
`clang/` finds 48 occurrences of `isAligned(` that are not member calls.
Six are other entities: `APInt::isAligned`
(`llvm/include/llvm/ADT/APInt.h:441`, `llvm/lib/Support/APInt.cpp:168`),
`MemOp::isAligned` (`llvm/include/llvm/CodeGen/TargetLowering.h:188`,
whose body calls `llvm::isAligned` qualified at `:183` and `:186`), and a
file-local function of four parameters in the global namespace
(`llvm/lib/Analysis/Loads.cpp:29`, `:108`, `:144`). The other 42 name the
function. Of those 42:

| Occurrences | Where | What a run on this corpus does |
|---|---|---|
| 14 | compiled text: `Alignment.h:145`, `:151`; `TargetLowering.h:183`, `:186`; ten in source files | found, edited, verified |
| 1 | `llvm/unittests/Support/AlignmentTest.cpp:154`, an argument of `EXPECT_EQ`, which also stringises it | found and edited at tier 1; the string literal the macro makes changes, so the unit that holds it differs and is not declared: **verification reports a mismatch**, correctly (`spec/api.md` "Open questions" 5) |
| 5 | the argument of `assert(...)` in files the corpus compiles | **not seen**: the corpus is Release (`spec/harness.md` §8.7), `NDEBUG` is defined, the argument is never parsed |
| 22 | `llvm/lib/Target/` for PowerPC, Hexagon, LoongArch, Mips, AMDGPU, ARM and CSKY (five of them also inside `assert`) | **not seen**: the corpus builds the X86 and AArch64 targets only, so no observer compiles these files |

So one configuration of LLVM sees 15 of 42 uses. Verification of that
run is truthful about the 4061 observers and says nothing about a build
with assertions or with another target. Slip's claim is over the observed
configurations (`docs/design/04-macros-and-configurations.md`), and this
is the plainest example of what that means for a codebase that is built
many ways. The run must list the 27 leftover occurrences (M2's residue
scan does) and the report must lead with them.

Closing the gap is not a matter of the API. It needs observers for the
other configurations: a second build tree with assertions on and all
targets, planned together with the first. That is an operator decision
(it roughly doubles the corpus's build tree and the run's parse time)
and is the first thing the real migration will need.

## 4. The run

1. `plan` over the corpus's compilation database: 4061 observers.
2. `map` with the transform. Each worker compiles the transform once;
   each job parses its unit, digests it, runs `transform()`, publishes a
   shard.
3. `reduce`: the claims of all shards to one edit set and its declared
   changes.
4. `apply` to an overlay.
5. `verify`: the observers that read an edited file are parsed against
   the overlay and digested; every unit not declared changed must be
   equal; every unit declared changed must exist.
6. Part A only, outside Slip: write the overlay to a copy of the tree,
   build `SupportTests` with the corpus's own compiler and run
   `--gtest_filter='KnownBitsTest.*'`. It must pass. (An operator
   decision: it needs a build of `LLVMSupport` and the test, a few
   CPU-minutes within the jobserver.)

Run twice with different worker counts (the machine's and half of it):
the edit sets must be byte-identical.

## 5. What is checked

| Check | Pass |
|---|---|
| Every observer that parsed in the M1 baseline completed its run | no failed run; the observers that are `failed` or `diagnosed` in the baseline are listed, not hidden |
| Part A edits | exactly the 36 of §2, in 12 groups; the diff of the three files is shown in full in the report |
| Part A declared units | each is a unit that holds one of the 12 calls; nothing else is exempt |
| Part A verification | every other unit of every affected observer is equal |
| Part A, independent | `KnownBitsTest` passes on the post-image |
| Part A, negative | the same run with the pair `{"llvm::KnownBits::ult","llvm::KnownBits::uge"}` (not equivalent) still verifies, because the calls are declared changed, and `KnownBitsTest` fails. This is recorded to show what verification does not claim |
| Part A, sabotage | a transform that swaps the arguments and does not call `expect_changed` is refused by verification, naming the three units |
| Part B edits | equal to the native rename's accepted edits and rules for the same target; every difference explained |
| Part B verification | all equal under the rule; no unit exempt |
| §3.1 | not gated: the run's edits, its one mismatch and its residue are recorded and compared with the table of §3.1 |
| Determinism | two runs, different worker counts: identical edit sets and transform ids |

## 6. What is measured

Per phase (plan, map, reduce, apply, verify), from the run records: wall
time, CPU time, peak resident memory of the coordinator and the workers'
distribution. And for the transform:

| Quantity | Why |
|---|---|
| map with the transform against the M1 map of the same commit (parse and digest only), wall and CPU | the cost of programmability: the claim is that it is within noise for a transform that finds nothing in most units |
| transform compile time, per worker process, and how many compiles the run did | whether one compile per worker holds; the cost of `--worker-jobs 1` |
| runs, host calls and steps: total, and the largest single run | the budget's headroom |
| time in each index (`references_to`, `calls_to`), total and largest | the real cost of a query on a large unit |
| observers by what the run did: no name found, name found and no call, edits | how much work the early exit saves |
| claims, distinct sites, largest number of observers claiming one site | deduplication (Part B) |
| edits accepted, rejected and why; groups | |
| units compared, units exempt, units under a rule | the size of what verification vouches for |
| reports by severity; `api.unresolved_spelling_matches` | what was not examined |
| leftover textual occurrences of the old names after apply | the residue |

The numbers go into the demonstration's record under `docs/demos/` and,
for the deterministic ones, a baseline (`spec/harness.md` §7.3).

## 7. What the demonstration may claim, and what it may not

It may claim:

1. A refactoring written as an ordinary C++ program against a small
   header, with no knowledge of Slip's pipeline, ran over every
   translation unit of LLVM and clang in parallel, and produced the same
   result whatever the number of workers.
2. Every call it rewrote was selected by the entity it resolves to, not
   by its spelling; the same spelling for other entities was not touched.
3. For every translation unit of the corpus in the configuration it is
   built in, every declaration other than the listed ones means exactly
   what it meant before (invariant (c) of
   `docs/design/03-equivalence.md`), checked by code the transform cannot
   influence.
4. For Part B, that nothing at all is exempt.

It may not claim, and the report must say so in these words or better:

1. **That the rewritten calls are equivalent.** The units that hold them
   are declared changed and are not compared. That they still behave the
   same is the author's claim; for this pair it is supported by the
   library's definitions and by its unit test, not by Slip. The negative
   check of §5 is there to make this visible.
2. **Anything about other configurations**: other targets, assertions
   on, other platforms' `#if` branches (§3.1 measures how much that is
   for one function).
3. **Completeness inside templates.** A call whose callee is a dependent
   name is not a call in version 0 (`spec/api.md` §6.7 rule 2); the
   counter of unresolved names spelled like the functions is reported.
   For these targets it is expected to be zero or nearly.
4. **Completeness outside what the compiler parses**: comments,
   documentation, strings, build files, and code no observer compiles.
5. That the cost numbers transfer to a transform that edits most units.
