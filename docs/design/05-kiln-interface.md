# 05 — The Slip/Kiln interface

Status: **a proposal to the Kiln agent, not an agreement.** Statements about
Kiln come from reading revision `4bfd999da`; each is to be confirmed or
corrected by Kiln before any of this is scheduled. File references are to that
revision.

## 1. Principles

1. **Slip does not duplicate Kiln.** Lexing, preprocessing, parsing, semantic
   analysis, constant evaluation and reflection are Kiln's. Slip adds
   recording, the model, the pipeline and the checker.
2. **One Slip crate depends on Kiln** (`slip_kiln`). Everything else in Slip
   depends on Slip's own model types. A Kiln API change touches one crate.
3. **Kiln emits events; Slip stores them.** Extensions are observer hooks
   (traits with default no-op methods) that Kiln calls with data it already
   has at that moment. Kiln does not grow Slip's data structures, and
   compilation pays nothing when no observer is attached. Kiln's performance
   gates must not move.
4. **No behaviour change.** Nothing here alters what Kiln accepts, diagnoses or
   generates, with one exception that is a bug fix (KF4).
5. **A facade with tests.** Kiln's crate APIs are explicitly unstable
   (`docs/STABILITY.md`). Slip asks for a small `kiln_tooling` facade whose
   surface is exercised by tests in Kiln's own CI, so that breaking it is a
   Kiln test failure and a deliberate decision, not something Slip discovers
   on the next submodule bump.
6. **Safe Rust on both sides.**

## 2. What Slip can do with Kiln as it is

A prototype of the trusted base needs nothing: Slip can drive the front-end
crates, walk the AST arenas, compute entity keys and digests, and diff two
parses. Milestones M0 and M1 ([06](06-engineering-plan.md)) are planned on
that basis, with private re-implementation of the driver glue as a temporary
measure where it is `pub(crate)` today. The requests below are ordered by when
their absence starts to block.

## 3. Requests

Size: S = within one Kiln work package; M = a few; L = a spec of its own.

### Foundation (needed for M0–M2; all small)

| Id | Request | Today | Size |
|---|---|---|---|
| KF1 | **Front-end library entry.** A public function from (compile command, file system, observers) to a finished front-end state (preprocessor record, AST, Sema), with no dependency on code generation or LLVM. Public compilation-database reader. | Compile-command-to-preprocessor glue and the database reader (`kiln_driver/src/scan_deps_cdb.rs`) are `pub(crate)`. | S–M |
| KF2 | **File identity and overlay.** Public mapping from `FileId` to (path, content hash); a virtual file system parameter so a TU can be parsed against in-memory replacements. | File identity is private; `FileId`/`Loc` are TU-local. | S–M |
| KF3 | **Location utilities.** Public "this location range as an editable file character range, or none" (the fix-it logic in `kiln_sema/src/macro_fixit.rs`), end-of-token, and enumeration of macro-expansion entries. Preprocessor callbacks accept more than one observer. | Present but private; callbacks are single-observer. | S |
| KF4 | **Bug: ADL flag.** `spec/sema-overload.md:194-196` says calls resolved through argument-dependent lookup are flagged. `set_call_adl` is called only for structured bindings (`struct_bind.rs:984`) and operators (`overload/oper.rs:1160`); ordinary calls are not flagged. | Divergence from spec. | S |

### The reference and preprocessing record (needed for M4; the core ask)

| Id | Request | Today | Size |
|---|---|---|---|
| KR1 | **Written-reference events.** A Sema observer called whenever a written name is resolved, with: the name's token range; the entity found; the syntactic role (expression, type name, nested-name-specifier component, base specifier, template name, using-declaration, using-directive, namespace alias target, member access, designator); the enclosing declaration; and, inside template instantiation, the instantiation it is happening for. One event per qualifier component, each with its own range. | The AST keeps the resolved entity but not the as-written form: no type-location information, no per-component qualifier locations, no stored end locations for many nodes. Slip cannot map a type name in source to an entity. | **L** |
| KR2 | **Lookup provenance.** On the same event: how the name was found (unqualified scope walk, qualified, member, ADL), through which using-declaration or using-directive, and for ADL the associated namespaces considered. | Discarded after lookup. | M |
| KR3 | **Declaration extents.** Full source range of each declaration, its name, its body, and for namespaces each reopening's braces. | Partial. | M |
| KR4 | **Preprocessing record.** Events for: macro definition and undefinition (with a stable definition identity); each expansion with the invocation range and each argument's range; each conditional directive with its range, outcome and the skipped range; each inclusion. Per-token provenance for `#` and `##` results. | Only the final macro table is kept; the callback sees expansions without arguments or conditional structure; paste and stringise provenance is lost; skipped groups are not lexed. | M–L |
| KR5 | **Exhaustive traversal.** A public visitor over the AST that is guaranteed to visit every node and field, with a test that fails when a node kind or field is added without the visitor (and the digest spec) being updated. | Traversals exist per consumer. | M |

KR2 is deliberately separate and lower priority than KR1: because edits are
verified and repaired, Slip can work without knowing *how* a name was found
(it proposes, the checker disposes). Provenance improves first-pass accuracy
and diagnostics; it is not on the soundness path.

### The transform universe (needed for M3)

| Id | Request | Today | Size |
|---|---|---|---|
| KS1 | **Host-call intrinsic.** One generic builtin, usable in constant evaluation, that forwards to a host-registered handler (sketch below). Recognised only when a handler is registered. Never memoised; an error if reached during speculative ("is this a constant expression?") evaluation. | Intrinsics are closed enums inside Kiln; no external registration; no output channel from constant evaluation. | M |
| KS2 | **Repeated evaluation entry.** After a transform TU is compiled, evaluate a named `consteval` function many times (once per observer), each run isolated, with a per-run step budget, memory budget and cancellation. A budget profile suited to programs, not constant expressions. | No host API to evaluate a function after the TU; default budget 1,048,576 steps. Nearest hook: `parse_prefix` / `continue_preprocessed` (`kiln_parse/src/lib.rs`). | M |
| KS3 | **Evaluator profile.** Step counts and hot functions per run, so Slip can report where a transform spends its time. | Counters exist in aggregate. | S |
| KS4 | *(later, optional)* Token-sequence literals as values (the P4380 value model without injection), `constexpr` printing, expression reflection as it standardises. | Not implemented. | L |

Sketch of KS1. The C++ side is declared only in Slip's resource headers:

```cpp
constexpr unsigned long long
__slip_call(unsigned op,
            const unsigned long long* in,  unsigned n_in,     // integer arguments
            const char*               bin, unsigned n_bin,    // byte argument (names, text)
            unsigned long long*       out, unsigned cap_out,  // integer results
            char*                     bout, unsigned cap_bout);
// returns the number of results available; callers use count-then-fill
```

and the Rust side is a trait Slip implements:

```rust
pub trait HostCalls {
    fn call(&mut self, op: u32, ints: &[u64], bytes: &[u8], out: &mut HostOut<'_>)
        -> Result<u64, HostError>;   // HostError becomes a constexpr exception in the transform
}
```

Everything typed is built on this in `<slip/meta.h>`. Kiln gains one builtin
with a fixed signature and no knowledge of Slip's operations; Slip can add
queries without touching Kiln. If the evaluator's per-call overhead makes this
too slow for hot paths, the fallback is a second builtin specialised for
"integer in, integer out", still generic.

Transforms use the C++ standard library inside the Oven far more heavily than
ordinary constant expressions do. Kiln's measured gap between scalar code
(about 65 M steps/s) and string-heavy library code (about 10 M) is therefore
of direct interest to Slip, and Slip will supply transform workloads as
evaluator benchmarks.

### Shared infrastructure

| Id | Request | Size |
|---|---|---|
| KM1 | The `kiln_tooling` facade crate (principle 5), and a written statement of what may be shared across threads and how long a finished front-end state may be retained. | M |
| KM2 | Kiln's test and benchmark harness pieces that are not Kiln-specific (jobserver client, golden-file handling, counter gating) factored so Slip can depend on them instead of copying. | M |

## 4. What Slip gives back

- A second, demanding consumer of the front end: every digest mismatch on an
  identity transform is a Kiln non-determinism or a Slip bug, and Slip reports
  the former with a reduced case.
- Corpus runs (libc++, LLVM) that exercise paths compilation does not, such
  as the as-written record.
- Tier T2 comparisons against clang ([03](03-equivalence.md) §8) surface
  accepts-invalid cases with locations.

## 5. Sequencing and fallbacks

| Slip milestone | Blocked without | Fallback until it lands |
|---|---|---|
| M0 parse the corpora | — | Private copy of driver glue; replaced by KF1 |
| M1 digests and verify | KR5 helps; KF2 for the overlay | Hand-written traversal with Slip-side exhaustiveness test; temp-directory post-image |
| M2 rename of functions and variables | KF3 | Expression-level names have locations today; types do not |
| M3 transform universe | KS1, KS2 | None that is honest. Level A (native) transforms work without it. |
| M4 namespace migration | **KR1**, KR3, KR4 | None. This is the critical path. |

KR1 is the long pole and has the most design freedom on the Kiln side, so it
is the first thing to discuss.

## 6. How requests are made

One request per message to the Kiln agent, sized to one of Kiln's work
packages where possible, containing: the event or function signature Slip
proposes; the behaviour in the form of spec text Kiln can adopt or rewrite;
and acceptance tests (inputs and the expected event stream). Kiln owns the
design of anything inside Kiln; Slip states needs and constraints.

## 7. Questions for the Kiln agent

1. Is the reading of each "Today" cell correct at your current head?
2. KR1: is an event sink at resolution time the right shape, or would you
   rather store as-written information in the AST (type locations, qualifier
   locations), which Tuck and diagnostics could also use?
3. KS1/KS2: can a compiled TU's evaluator state be reused across many isolated
   top-level evaluations cheaply, or is re-entering from a parsed prefix the
   practical route?
4. Is retaining a finished front-end state after the TU (for queries against
   it) compatible with how Sema owns its arenas today?
5. Which of these would you rather Slip prototypes on a branch for you to
   review, and which do you want to own outright?
