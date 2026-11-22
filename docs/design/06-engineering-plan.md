# 06 — Engineering plan

How Slip gets built, validated and kept re-implementable. The process is
Kiln's, adapted; where this document is silent, Kiln's `docs/AGENTS.md`
conventions apply.

## 1. Crates

```
                      slip_model          data types + on-disk formats; no Kiln, no I/O policy
                     ╱    │     ╲
          slip_kiln   slip_digest   slip_cdb        slip_kiln is the ONLY crate that depends on Kiln
             │  ╲        │
             │   slip_verify            slip_reduce   slip_apply      (model-only; no Kiln)
             │        │                      │            │
        slip_script   └────────── slip_exec ─┴────────────┘     coordinator, workers, store, admission
             │                        │
      resource headers            slip_cli
   <slip/meta.h>, L2, pinned libc++
                                   xtask   test/bench/corpus harness
```

| Crate | Responsibility | Depends on Kiln |
|---|---|---|
| `slip_model` | Entity keys, references, ranges, edits, facts, shards, reports; versioned binary and JSON formats | no |
| `slip_kiln` | Drive Kiln's front end; attach recorders; extract the subject model | **yes** |
| `slip_digest` | Entity-key construction and the semantic digest. The trusted base; kept small. | via `slip_kiln`'s traversal |
| `slip_verify` | Compare digests under `M`; localise mismatches | no |
| `slip_cdb` | Compilation-database readers, command normalisation, plan | no |
| `slip_script` | Host side of the transform universe: implement host calls over the model; run transforms | yes (evaluator) |
| `slip_reduce` | Reduce: facts, guards, unanimity, geometry, groups | no |
| `slip_apply` | Overlay, byte-exact application, patch output | no |
| `slip_exec` | Coordinator, worker pool, jobserver, memory admission, content-addressed store | no |
| `slip_cli` | The `slip` binary | no |

Reduce, apply, verify and the formats being Kiln-free is what makes them
testable with hand-written shards, implementable in parallel, and
re-implementable from the spec.

## 2. Specifications

`spec/` is normative; code implements it; tests cite it.

| Spec | Defines |
|---|---|
| `spec/model.md` | Entity keys; the subject model; shard, plan and report formats (byte level, versioned) |
| `spec/digest.md` | The semantic digest: for every Kiln AST node kind and field, included or excluded, and why |
| `spec/api.md` | `<slip/meta.h>`: every function's signature, preconditions, result, errors; versioning rules |
| `spec/edits.md` | Claims, reduce as a function from a set of shards to an edit set, apply, the verify loop, native rename (planned as `spec/reduce.md`; written as one file because the formats run through all four) |
| `spec/verify.md` | Comparison under declared changes; outcomes; localisation |
| `spec/macros.md` | Token origin; tiers; coverage states |
| `spec/exec.md` | Phases, the store and cache keys, admission, failure handling, exit codes |
| `spec/harness.md` | Test kinds, golden formats, gating tiers, performance counters |
| `spec/pitfalls.md` | The catalogue of semantic hazards ([03](03-equivalence.md) §7); one test per row |

Rules, as in Kiln: every test names the spec section it covers; a traceability
report lists sections without tests; work is done in **work packages** (one
agent session, under about two thousand lines, one spec section plus its tests
plus its code), recorded under `spec/landed/`. A second implementation should
be buildable from `spec/` and the test suite alone; formats are specified to
the byte for that reason.

## 3. Validation

| Kind | What it checks | Runs |
|---|---|---|
| Unit | Per crate | every commit |
| **Transform tests** | Input tree + compile commands + transform → expected post-image and report, as golden files | every commit |
| Model goldens | Subject model dump for small inputs. Pins the Slip/Kiln boundary; the first thing to fail on a Kiln bump. | every commit |
| **Checker mutation tests** | For each pitfall: a correct edit that must verify, and a wrong edit that must be caught with the right location | every commit |
| Reduce properties | Output invariant under shard order, duplication and partitioning; random shards | every commit |
| API conformance | Each `<slip/meta.h>` function against `spec/api.md`, run as transforms | every commit |
| Corpus invariants | On libc++ and LLVM: (a) parsing twice gives equal digests; (b) the identity transform gives zero edits and verifies; (c) rename there and back gives a byte-identical tree; (d) coverage does not regress | merge / nightly |
| Independent oracles | Post-image builds with clang (T2); symbol sets match under `M` (T3); the corpus's own tests pass (T4) | nightly / release |
| **Checker fuzzing** | Apply random meaning-changing mutations to corpus code (drop a qualifier, add an overload, reorder declarations). If the object code changes and the digest does not, that is a checker bug. | nightly |
| Performance | §4 | merge (counters), nightly (time, memory) |

Gating tiers: *commit* (minutes, no corpora), *merge* (full suite plus a
corpus slice), *release candidate* (everything, on full corpora, all oracles,
performance within budget, traceability complete, no open soundness bug).
Prototypes may land behind a feature flag with commit-tier tests only; nothing
reaches a release candidate without the rest.

The two rows in bold are specific to Slip's correctness argument: the checker
is the trusted base, so it gets tests whose purpose is to make it fail.

## 4. Performance and memory

Tracked by the harness from the first milestone, per phase and per corpus:

| Metric | Gate |
|---|---|
| Deterministic counters: TUs parsed, host calls, evaluator steps, shard bytes, edits, cache hits | Tight (exact or a small tolerance); a change needs a baseline update in the same commit, with a reason |
| CPU time and wall time per phase | Loose threshold plus trend |
| Peak RSS per worker; peak total; RSS-seconds | Loose threshold plus trend |
| Scaling: wall time at 1, 2, 4, … cores | Curve stored; parallel efficiency reported |
| Map overhead: Slip's map phase vs Kiln `-fsyntax-only` on the same TU | Reported as a ratio; a budget is set once M0 has measured it |

Baselines live in the repository. Counters are what CI gates on, because they
are reproducible on a shared machine; time and memory are measured on a quiet
run and reviewed as trends.

## 5. Milestones

Each ends with a demonstration on the corpora, not with a feature list.

| | Content | Exit | Needs from Kiln |
|---|---|---|---|
| **M0 Foundations** | Workspace, harness, `slip_model` v0, compilation databases, coordinator and workers with jobserver and memory admission | `slip` parses every LLVM/clang TU and the libc++ test TUs inside the memory budget; per-phase baseline recorded | nothing (KF1 when available) |
| **M1 Trusted base** | Entity keys, digest, verify, localisation | Corpus invariants (a) and (b) hold on full corpora; mutation suite v1 catches every catalogued wrong edit | nothing (KR5 helps) |
| **M2 Edits** | Edit records, reduce, overlay apply, verify loop, native rename of functions and variables | Invariant (c) for a sample of LLVM functions including ones declared in widely shared headers; clang builds the post-image | KF2, KF3 |
| **M3 Transforms** | Transform universe, `<slip/meta.h>` v0, rules, facts and guards | Example 9.1 of [01](01-transform-api.md) written as a transform and run on LLVM against a real API pair; API conformance suite | KS1, KS2 |
| **M4 Namespace migration** | Written-reference model, `spell`, `relocate_namespace`, macro tiers 0–2, coverage report, repair | The stand-in for the motivating case: relocate the `llvm` namespace for an owned subset of LLVM while a chosen part (ADT and Support) plays the read-only dependency that keeps it. Verified; builds with clang; `check-llvm` passes. | **KR1**, KR3, KR4 |
| **M5 Scale and hardening** | Shard and model caches, transform memoisation, multiple configurations, performance budgets, release-candidate process | First release candidate | KR2, KM1 |
| **M6+** | Whole-cloth generation library; macro research R1–R4; single-file interactive mode for editors; further build-system adapters; distributed map | — | KS4 |

M0–M2 do not wait on Kiln. KR1 is requested now so that it can land while
they are built.

## 6. Parallel work

Tracks, each owning crates and specs:

| Track | Owns | Can start |
|---|---|---|
| A. Model and formats | `slip_model`, `spec/model.md` | first; everything depends on it |
| B. Kiln adapter | `slip_kiln` | after A's types |
| C. Trusted base | `slip_digest`, `slip_verify`, `spec/digest.md`, `spec/verify.md`, `spec/pitfalls.md` | after A |
| D. Pipeline | `slip_cdb`, `slip_exec`, `slip_reduce`, `slip_apply` | after A; reduce and apply need only shard formats |
| E. Transform universe | `slip_script`, resource headers, `spec/api.md` | spec now; code when KS1 lands |
| F. Refactoring library | L2 headers, pitfall tests | after E |
| G. Harness and corpora | `xtask`, corpora setup, oracles, performance | first, alongside A |
| H. Preprocessing and macros | model's preprocessing record, `spec/macros.md`, research | spec now; code when KR4 lands |

How the work is run:

- **Interfaces first.** The main session writes each spec section and the
  `slip_model` types it implies before implementers start; implementers work
  against types and golden formats, not against each other's code.
- **Implementers are Opus subagents**, one work package each, in hand-made git
  worktrees with per-checkout target directories. Design and review stay in
  the main session.
- **All builds go through the shared jobserver** (`scripts/env.sh`). The
  container has a 16 GiB memory limit; corpus runs use Slip's own memory
  admission and are scheduled one at a time.
- **Review.** Every work package is reviewed against its spec section before
  merge. Anything in the trusted base (`slip_digest`, `slip_verify`) gets a
  second, independent review that is given the spec and the code but not the
  author's reasoning.

## 7. Risks

| Risk | Consequence | Mitigation |
|---|---|---|
| KR1 is large and Kiln has its own roadmap | M4 slips | Requested first; M0–M3 independent of it; Slip offers to prototype on a branch |
| The evaluator is slow on library-heavy transform code | Level B/C too slow at scale | Native selectors and host-side `code` values keep per-site evaluation small; Level A; memoisation; Slip workloads as Kiln benchmarks |
| Digest incomplete | A changed meaning verifies | Spec tied to AST fields with an exhaustiveness test; mutation tests; checker fuzzing against object code; oracles T2–T4 |
| Kiln accepts invalid code or misresolves consistently | Post-image "verifies" but clang disagrees | T2 and T3 in the harness; reported to Kiln |
| Memory: 16 GiB container vs LLVM's largest TUs | Corpus runs thrash or fail | Memory admission from M0; measured per-TU peaks; ask for more if measurements say so |
| Standard-track papers change | API names drift from the standard | Slip's namespace is its own; provisional names listed in the spec; inline namespace versioning |
| First real migration surfaces hazards not in the catalogue | Long tail of manual work | By design they surface as located mismatches; catalogue and repair library grow from them |

## 8. Next steps

1. Operator review of the open questions in the [README](README.md).
2. Kiln agent review of [05](05-kiln-interface.md); agree the shape of KR1.
3. Workspace skeleton, `xtask` harness, `spec/model.md` and `spec/harness.md`
   (tracks A and G), then M0 work packages.
