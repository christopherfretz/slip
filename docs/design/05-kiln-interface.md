# 05 — The Slip/Kiln interface

Status: **revision 2 (2026-11-21), reviewed by the Kiln agent** against Kiln
`0af36a384`. The "Today" cells below carry Kiln's corrections; the shape of
KR1 and the ownership of each request (§7) are agreed between the two agents.
Nothing is scheduled: size and order of the Kiln-side work are the operator's
decision. File references are to Kiln `4bfd999da`.

## 1. Principles

1. **Slip does not duplicate Kiln.** Lexing, preprocessing, parsing, semantic
   analysis, constant evaluation and reflection are Kiln's. Slip adds
   recording, the model, the pipeline and the checker.
2. **One Slip crate depends on Kiln** (`slip_kiln`). Everything else in Slip
   depends on Slip's own model types. A Kiln API change touches one crate.
3. **Kiln records what only it knows; Slip reads it afterwards.** Semantic
   facts that exist only while Kiln resolves a name are kept by Kiln in
   opt-in, cold side tables next to the AST and read by Slip once the TU is
   finished (KR1; §4 gives the reasons a stored table is preferred to a
   callback). Preprocessor extensions are observer hooks (traits with default
   no-op methods). Either way Kiln does not grow Slip's data structures, and
   compilation pays at most one predictable branch per site when recording is
   off. Kiln's performance gates must not move.
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
| KF1 | **Front-end library entry.** A public function from (compile command, file system, observers) to a finished front-end state (preprocessor record, AST, Sema), with no dependency on code generation or LLVM. Public compilation-database reader. | `syntax_only`, the compile-command-to-preprocessor glue and both database readers (`kiln_driver/src/scan_deps_cdb.rs`) are `pub(crate)`. | S–M |
| KF2 | **File identity and overlay.** A content hash per file; a virtual file system (overlay) parameter that `#include` resolution goes through, so a TU can be parsed against in-memory replacements. | `SourceManager` has public `text(FileId)`, `file_name(FileId)` and `add_virtual(name, contents)`. There is no overlay that inclusion consults and no content hash; `FileId`/`Loc` are TU-local. | S–M |
| KF3 | **Location utilities.** Public "this location range as an editable file character range, or none" (the fix-it logic in `kiln_sema/src/macro_fixit.rs`), end-of-token, and enumeration of macro-expansion entries. Preprocessor callbacks accept more than one observer. | **Delivered by Kiln WP-2150 at `913a490bb`:** `kiln_lex::token_end`; `SourceManager::editable_range` / `editable_file_range` with `file_loc_at_macro_start` / `_end` (clang's `makeFileCharRange` and `canInsert` rules, and one buffer range in order); `expansion_count` / `expansion_entries`; `Preprocessor::add_callbacks`. Kiln's fix-its use the same functions. Slip calls them through `slip_kiln` (`docs/wp/WP-084.md`, `docs/wp/WP-068.md`) once its pin is at or after that commit (the pin move after WP-044, `docs/wp/WP-101.md`), and writes none of them a second time. Slip's own, because Kiln has no function for them: the tier (where a token is written), and the refusal of a file that was recoded on load. A name that is a whole macro expansion (`ID(foo)`) is refused, not given a range by a rule of Slip's. [KF3](../requests/KF3-location-utilities.md) §2 maps what was asked to Kiln's names and says where the rules differ. | S |
| KF4 | **Bug: ADL flag.** `spec/sema-overload.md:194-196` says calls resolved through argument-dependent lookup are flagged. `set_call_adl` is called only for structured bindings (`struct_bind.rs:984`) and operators (`overload/oper.rs:1160`); ordinary calls are not flagged. | Divergence from spec. | S |

### The reference and preprocessing record (needed for M4; the core ask)

| Id | Request | Today | Size |
|---|---|---|---|
| KR1 | **The written-reference table.** An opt-in side table, filled while Kiln resolves names, with one entry per written name: the name's token range; the entity found; the syntactic role (expression, type name, nested-name-specifier component, base specifier, template name, using-declaration, using-directive, namespace alias target, member access, designator); the enclosing declaration; the parent entry, for the components of a nested-name-specifier (one entry per component, each with its own range); and the specialization, for names resolved during template instantiation. Detail in §4. | The AST keeps the resolved entity but not the as-written form. There is no type-location information: a written type is a `QualType` plus a few single locations. Nested-name-specifier components carry no location. `DeclRefExpr` and `MemberExpr` keep no name token range. Slip cannot map a type name in source to an entity. | **L** (3–5 Kiln work packages: it touches every name-resolution site) |
| KR2 | **Lookup provenance.** On the same entry: how the name was found (unqualified scope walk, qualified, member, ADL), through which using-directive, and for ADL the associated namespaces considered. | Partly kept: `DeclRefExtra.found_decl` (and the same for `MemberExpr`) holds the using-shadow declaration lookup found when it differs from the referenced declaration. Not kept: which using-directive was used; the ADL associated namespaces. | M |
| KR3 | **Declaration extents.** Full source range of each declaration, its name, its body, and for namespaces each reopening's braces. | Declarations have a name `loc` and an optional `begin_loc`, and no end. Namespaces have no brace locations; each reopening is its own redeclaration with one location. | M |
| KR4 | **Preprocessing record.** Events for: macro definition and undefinition (with a stable definition identity); each expansion with the invocation range and each argument's range; each conditional directive with its range, outcome and the skipped range; each inclusion. Per-token provenance for `#` and `##` results. | `PpCallbacks` (`kiln_pp/src/callbacks.rs`) holds a single boxed observer. `macro_expands` receives the whole invocation span and no argument ranges. `source_range_skipped` reports skipped groups (they are not lexed). There is no callback for the outcome of `#if`/`#elif`, no preprocessing record, and no `#`/`##` provenance. | M–L |
| KR5 | **Exhaustive traversal.** A public visitor over the AST that is guaranteed to visit every node and field, with a test that fails when a node kind or field is added without the visitor (and the digest spec) being updated. | Traversals exist per consumer. | M |

KR2 is deliberately separate and lower priority than KR1: because edits are
verified and repaired, Slip can work without knowing *how* a name was found
(it proposes, the checker disposes). Provenance improves first-pass accuracy
and diagnostics; it is not on the soundness path.

### The transform universe (needed for M3)

| Id | Request | Today | Size |
|---|---|---|---|
| KS1 | **Host-call intrinsic.** One generic builtin, usable in constant evaluation, that forwards to a host-registered handler (sketch below). Recognised only when a handler is registered. An error if reached during speculative ("is this a constant expression?") evaluation. Kiln's planned mechanism: one bytecode builtin dispatching to a `&mut dyn HostCalls` the Oven holds; a frame that executes a host call is marked, and the mark propagates up the stack, so no frame on that stack is memoised, while pure callees keep their memo entries, across runs too. | Builtins are closed enums inside Kiln; there is no embedder hook (`SemaBridge` is implemented by Sema only) and no output channel from constant evaluation. | M |
| KS2 | **Repeated evaluation entry.** After a transform TU is compiled, evaluate a named `consteval` function many times (once per observer), each run isolated, with a per-run step budget, memory budget and cancellation, and memory reclaimed per run. A budget profile suited to programs, not constant expressions. Kiln's planned mechanism: a retained *session* (Sema and Oven kept alive) with `evaluate(function, arguments, budget)`. | No host API to evaluate a function after the TU. Every Oven entry point takes a `&mut dyn SemaBridge` (lazy instantiation, on-demand compilation of callees), so evaluation after the TU needs Sema kept alive, not only the AST. Default budget: 1,048,576 steps, depth 512, 256 MiB. Stopgap: re-entering from `parse_prefix` / `continue_preprocessed` (`kiln_parse/src/lib.rs`), which re-parses every time. | M |
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
| KM1 | The `kiln_tooling` facade crate (principle 5), with static `Send`/`Sync` assertions and a written statement of what may be shared across threads and how long a finished front-end state may be retained. Kiln's account of today: `ParsedTu` owns `Ast` and `Oven` by value with no lifetime, so it can be kept and queried after the TU; the `Ast` arenas hold no `Rc`/`RefCell` (probably `Send + Sync`, not yet asserted); the Oven holds a `RefCell` (`Send`, not `Sync`); Sema holds `Rc`/`RefCell` (not `Send`). A session (Sema and Oven) is therefore single-threaded, which matches Slip's one-observer-per-worker-thread execution ([02](02-pipeline.md) §8). | M |
| KM2 | Kiln's test and benchmark harness pieces that are not Kiln-specific (jobserver client, golden-file handling, counter gating) factored so Slip can depend on them instead of copying. | M |

## 4. KR1 in detail: a stored table, not an event sink

Revision 1 proposed a Sema observer called at each resolution. Kiln prefers,
and Slip agrees to, as-written information **stored beside the AST in an
opt-in, cold side table**:

- **PCH and modules.** Names in a precompiled header or an imported module are
  resolved when that file is built; an observer in the consuming TU would
  never see them. A stored table travels in the snapshot and in the module
  image.
- **Instantiation.** Kiln instantiates lazily and once per specialization, so
  the order of events would depend on the point of first use. A table keyed
  by pattern node and specialization is queried instead, independent of order.
- **No re-entrancy.** Nothing calls into Slip in the middle of semantic
  analysis.
- **Reuse.** Kiln's own diagnostics and fix-its can use type ranges later, and
  so can Tuck.

An entry is: token range; entity; role; enclosing declaration; parent entry
(for nested-name-specifier chains); specialization. Cost with recording off:
one branch per resolution site, and Kiln's performance gates must hold. Cost
with recording on: Kiln's estimate is 16–24 bytes per written name. The facade
exposes the table to Slip as an iterator (per declaration and whole-TU), which
is all Slip's extraction needs.

Consequences on the Slip side: the subject model's written-reference section
([02](02-pipeline.md) §2) is built by one pass over this table after the TU,
not incrementally; and references inside a PCH or module are available in the
consuming observer, which revision 1 could not promise.

## 5. What Slip gives back

- A second, demanding consumer of the front end: every digest mismatch on an
  identity transform is a Kiln non-determinism or a Slip bug, and Slip reports
  the former with a reduced case.
- Corpus runs (libc++, LLVM) that exercise paths compilation does not, such
  as the as-written record.
- Tier T2 comparisons against clang ([03](03-equivalence.md) §8) surface
  accepts-invalid cases with locations.

## 6. Sequencing and fallbacks

| Slip milestone | Blocked without | Fallback until it lands |
|---|---|---|
| M0 parse the corpora | — | Private copy of driver glue; replaced by KF1 |
| M1 digests and verify | KR5 helps; KF2 for the overlay | Hand-written traversal with Slip-side exhaustiveness test; temp-directory post-image |
| M2 rename of functions and variables | KF2 stage 1 (the overlay). KF3 is delivered (Kiln WP-2150, `913a490bb`): what M2 needs of it is Slip's pin at or after that commit | The names come from the written-reference table (stages 1 and 2) and the declaration extents, both on Kiln main; their file ranges are Kiln's `editable_file_range` and `token_end`, called by `slip_kiln::ranges` (`docs/wp/WP-084.md`). Until Kiln's recording switch (KR1 amendment 1 §1, Kiln WP-2320) the table is switched on by Kiln's dump flag and the dump discarded |
| M3 transform universe | KS1, KS2 | None that is honest. Level A (native) transforms work without it. |
| M4 namespace migration | **KR1**, KR3, KR4 | None. This is the critical path. |

KR1 is the long pole and has the most design freedom on the Kiln side, so it
is the first thing to discuss.

## 7. Ownership (agreed with the Kiln agent)

| Who | Requests |
|---|---|
| **Kiln designs and implements** | KF4 (being fixed; it has a Kiln backlog row), KR1, KR3, KR4, KS1, KS2, KM1 (the facade) |
| **Slip prototypes on a Kiln branch; Kiln reviews and lands it as a work package** | KF1 (landed, Kiln WP-2120), KF2, KR5 (a generated or exhaustive visitor, with the test that fails when a field is added), KS3. KF3 was in this row: Kiln delivered it itself (WP-2150, `913a490bb`) and no branch was cut for it |
| **Later** | KR2, KM2, KS4 |

## 8. How requests are made

One request per message to the Kiln agent, sized to one of Kiln's work
packages where possible, containing: the table entry, event or function
signature Slip proposes; the behaviour in the form of spec text Kiln can adopt
or rewrite; and acceptance tests (inputs and the expected table or event
stream). Kiln owns the design of anything inside Kiln; Slip states needs and
constraints. The Kiln agent relays each request to the operator, who decides
scheduling.
