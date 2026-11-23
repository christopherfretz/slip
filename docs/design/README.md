# Slip — end-to-end design

Status: **proposal, revision 1 (2026-11-21)**; [05](05-kiln-interface.md) is at
revision 2 after the Kiln agent's review. Nothing here is implemented. This
is the first deliverable: a design for composable, scalable, evolvable code
transformation written in general-purpose constexpr C++.

Evidence for the claims about Kiln, the C++ standard track and prior art is in
the research notes that fed this design (kept in the agent workspace under
`research/`, six files). Claims about Kiln are from reading Kiln at revision
`4bfd999da`; those in [05](05-kiln-interface.md) have been checked and
corrected by the Kiln agent.

## Reading order

| File | Contents |
|---|---|
| this file | Goals, the ten decisions, the picture, open questions |
| [01-transform-api.md](01-transform-api.md) | The programming model: `<slip/meta.h>`, handles, queries, edits, guards, examples, stability |
| [02-pipeline.md](02-pipeline.md) | Plan → map → reduce → apply → verify; compilation databases; shared headers; parallelism and memory |
| [03-equivalence.md](03-equivalence.md) | Proving "everything resolves as before": entity keys, semantic digests, the namespace migration |
| [04-macros-and-configurations.md](04-macros-and-configurations.md) | Macro tiers, conditional compilation, coverage accounting, research tracks |
| [05-kiln-interface.md](05-kiln-interface.md) | The Slip/Kiln boundary and the extensions requested from Kiln |
| [06-engineering-plan.md](06-engineering-plan.md) | Crates, specs, validation harness, performance tracking, milestones, work breakdown |
| [07-reference-transforms.md](07-reference-transforms.md) | Design input: the transforms the general API must express (feature-flag removal in three forms), what each demands, how a decision crosses translation units |

## Goals, in priority order

1. **Correct.** A transformation that claims to preserve meaning is checked, per
   translation unit, against a semantic model of the code before and after. An
   edit Slip cannot justify is not applied silently: it is rejected or reported.
2. **Memory safe.** Safe Rust throughout (`#![forbid(unsafe_code)]`), as in Kiln.
3. **Programmable.** Transformations are ordinary constexpr C++ programs over a
   stable header API; they compose as libraries.
4. **Fast and lean.** Tens of millions of lines, all cores used, wall clock and
   memory tracked by the validation harness as part of correctness.

## The ten decisions

**D1. A Slip-owned header over intrinsics, with standard-track vocabulary.**
Transformations are written against `<slip/meta.h>`, a versioned header built
on private `__slip_*` intrinsics. It borrows the standard track's *value model*
(one opaque handle type plus free functions, as `std::meta::info`; names mirror
`std::meta` and the in-flight expression-reflection papers where the meaning is
the same) and rejects its *effect model* (injection into the translation unit
being compiled). Reason: every C++29 code-generation proposal adds tokens to
the current TU; refactoring replaces byte ranges of existing files, including
shared headers, and nothing on the standard track covers edits, use sites,
lookup provenance, macros or source ranges. Detail: [01](01-transform-api.md).

**D2. Two universes.** The code being transformed (the *subject*) and the
transformation program (the *transform*) are separate compilations. The subject
is parsed by Kiln exactly as its build compiles it (its own `-std=`, flags,
standard library). The transform is a C++26 program compiled once, in its own
Kiln instance, whose intrinsics query the subject through the Slip host. Reason:
real subjects are C++17 or older (LLVM builds as C++17) and cannot host C++26
reflection code; the subject's semantic model must not be perturbed by the
tool; and the transform is compiled once instead of once per TU. Consequence:
subject entities are named by lookup (`slip::lookup("a::b")`), not by `^^a::b`.

**D3. Edits are untrusted; equivalence checking is the trusted base.** A
transform emits edits. Slip applies them, re-parses every affected TU, and
compares a *semantic digest* of each TU before and after, modulo the changes
the transform declared (for a namespace migration: the relocation map). The
digest is a hash over Kiln's resolved AST: every reference contributes the
identity of the entity it resolved to, including implicit calls, conversions
and template instantiations. A wrong edit, a gap in what Slip records, or a
shared-header edit that is right for one TU and wrong for another all surface
as a digest mismatch at a named site. Detail: [03](03-equivalence.md).

**D4. Map/reduce over (TU, configuration) observers.** Each TU under each
build configuration is an *observer*. Map: parse, extract the subject model,
run the transform, write a shard of edits, facts and reports. Reduce: merge
shards per file, deduplicate by semantic site key, require unanimity among
observers of a site, resolve guards, form atomic groups. Then apply, then
verify, then (if needed) repair and repeat to a fixed point. Detail:
[02](02-pipeline.md).

**D5. Shared headers: everyone observes, identical work is memoised.** All
observers of a header vote on its edits; the reducer deduplicates. Redundant
transform *execution* is removed by content-addressed memoisation: the
transform runs once per distinct semantic context of a header, not once per
TU. This is sound because a constexpr transform's only inputs are host calls,
which Slip can record and re-check.

**D6. Global decisions without a second parse.** A transform emits *facts*
(folded across TUs by a declared monoid) and *guarded edits* (applied only if
a predicate over folded facts holds). Decisions that need whole-program
knowledge — "is every declaration of this entity in code we own?" — are
settled in reduce, not by parsing everything twice.

**D7. Three granularities.** Native coarse transformations configured by data
(fastest); rules with a native selector and a constexpr action per match; and
free-form traversal with fine-grained queries (most flexible).

**D8. Macros by tiers, enforced by unanimity and verification.** Every edit is
keyed by where its bytes are *spelled*. A token in a macro body has one
spelling shared by every expansion in every TU; all of them must agree before
the definition is edited. Pasted and stringised tokens are refused and
reported. Bytes no configuration ever compiled are listed, not guessed at.
Detail: [04](04-macros-and-configurations.md).

**D9. One narrow, versioned boundary to Kiln.** Exactly one Slip crate depends
on Kiln's crates. Slip asks Kiln for a small set of extensions (a written-
reference table, a richer preprocessing record, a front-end library entry
point, a host-call intrinsic) and a facade tested in Kiln's CI. A first
prototype needs none of them. Detail: [05](05-kiln-interface.md).

**D10. Kiln's engineering process, adapted.** Specs before code, tests tied to
spec sections, single-session work packages, a specified harness, performance
counters gated in CI, corpus runs on libc++ and LLVM. Detail:
[06](06-engineering-plan.md).

## The picture

```
 compile_commands.json (one per configuration)        transform.cpp + <slip/meta.h>
            │                                                   │ compiled once (C++26)
            ▼                                                   ▼
   ┌─────────────────┐   per (TU, config), in parallel   ┌───────────────┐
   │ plan            │──────────────────────────────────▶│ MAP worker    │
   │ normalise, shard│                                   │ Kiln front end│ subject, as built
   └─────────────────┘                                   │ subject model │ refs, macros, ranges
                                                         │ pre-digest    │
                                                         │ run transform │ host calls only
                                                         └──────┬────────┘
                                                                │ shard: edits, facts,
                                                                ▼        reports, digests
   ┌────────────────────────────────────────────────────────────────────┐
   │ REDUCE (per file, parallel): dedup by site, unanimity, guards,     │
   │ conflicts, atomic groups                                           │
   └──────┬─────────────────────────────────────────────────────────────┘
          ▼
   ┌─────────────┐     ┌──────────────────────────────────────────────┐
   │ APPLY       │────▶│ VERIFY (per observer, parallel): re-parse the │
   │ byte-exact  │     │ post-image, digest, compare modulo declared   │
   └─────────────┘     │ changes; mismatches → repair hook → repeat    │
                       └──────────────────────────────────────────────┘
          ▼
   patch + report: applied / rejected (with cause) / uncovered bytes / residual manual work
```

## What Slip guarantees, stated exactly

For a transformation declared semantics-preserving modulo a relocation map `M`:

- For every observer (TU × configuration) that was analysed: the post-image
  parses without new diagnostics, and every declaration's semantic digest
  equals the pre-image digest mapped through `M`. "Semantic digest" is defined
  in [03](03-equivalence.md); it is as strong as Kiln's AST is complete.
- For bytes no observer compiled (inactive `#if` branches, macros never
  expanded, templates never instantiated): no guarantee; they are enumerated
  in the coverage report with the reason.
- Kiln accepts some code clang rejects. A build of the result with the
  production compiler remains a recommended final gate, and Slip's harness
  runs one on its corpora.

## Open questions for the operator

1. **Two universes (D2).** *Decided 2026-11-21: accepted by the operator.*
   The deciding argument is that a transform compiled inside the subject TU
   changes the shape of the AST being refactored, more than language mode:
   the motivating codebase is old but maintained and builds as C++23. The
   in-TU bridge for C++26 subjects (`slip::subject(^^ns::f)` converting a
   native reflection into a Slip handle) is wanted, at lower priority; the
   API must keep room for it ([01](01-transform-api.md) §3).
2. **The transform's standard library.** *Decided 2026-11-21: pinned, with
   an option to use another installation.* Slip pins the standard library
   the transform universe is compiled against and records its identity in
   the plan, so a transform behaves the same on every machine; the subject
   is unaffected (D2). An option selects a different installation. The pin
   is **libstdc++ (GCC 16.1)**, not libc++: it is the only production
   library with `<meta>`, and the one Kiln's reflection support is validated
   against. `<slip/meta.h>` itself stays library-neutral and is tested
   against both, so the pin can move. The pinned headers are located on the
   host and checked against the pinned digest; bundling them with Slip is
   deferred until there is a release to package (libstdc++ is GPLv3 with the
   GCC Runtime Library Exception; the operator's only requirement is that it
   is used within its terms).
3. **Formatting.** *Decided 2026-11-21.* Slip does not contain a formatter.
   It edits bytes and leaves everything else untouched, and provides:
   insertion helpers that copy the surrounding indentation; a
   machine-readable list of the ranges it changed; and an optional hook that
   runs the user's formatter (some configuration of clang-format, for the
   motivating codebase) over the touched ranges *before* verification, so
   the bytes that are verified are the bytes that land.
4. **What "dependencies" means for the migration.** *Decided 2026-11-21.*
   A file is *owned* (Slip may edit it, and names it declares move) or
   *read-only* (never edited; names it declares keep the old namespace and
   are requalified at their uses). Two sources, in this order: path
   prefixes given by the user (the motivating codebase installs its
   dependencies under one prefix), then, for files no prefix decides, how
   the build treats them: a file every observer reaches as a system header
   (`-isystem` and the like) is read-only. A file that some observers see
   as a system header and others do not is reported, not guessed.
5. **Naming.** *Decided 2026-11-22: the names stay.* A user's program is a
   *transform*, the code it runs on the *subject*, and a (TU, configuration)
   pair an *observer*.
