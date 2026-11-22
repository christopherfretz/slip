# Slip specifications

`spec/` is normative. Code implements it and tests cite it. If code and spec
disagree, one of them is fixed and the change says which. A spec change
without tests is incomplete. The aim is that a second implementation of Slip
could be built from `spec/` and the test suite alone, which is why on-disk
formats are specified to the byte.

Design rationale lives in `docs/design/`; it explains why, and is not
normative. Where the two disagree, the spec wins and the design is corrected.

## Index

| Spec | Defines | Implemented by | Status |
|---|---|---|---|
| [model.md](model.md) | Encoding primitives, the hash, the container format, file and observer identity, plan and observer-record formats | `slip_model` | draft |
| [cdb.md](cdb.md) | Compilation databases, command-line splitting, response files, command normalisation | `slip_cdb` | draft |
| [exec.md](exec.md) | Phases, workers, jobserver, memory admission, the store, failure handling, exit codes | `slip_exec`, `slip_cli` | draft |
| [harness.md](harness.md) | `cargo xtask`: lints, test kinds, golden files, gating tiers, performance records | `xtask` | draft |
| [digest.md](digest.md) | The semantic digest: units, keys, streams, the `digests` section, completeness obligations | `slip_digest`, `slip_model` | draft |
| digest-kinds.md | The digest's tables: every Kiln AST kind and field, its class and decision, its stream encoding | `slip_digest` | not written (M1) |
| [verify.md](verify.md) | Comparison under declared changes; localisation | `slip_verify` | draft |
| pitfalls.md | The catalogue of semantic hazards; one test per row | tests | not written (M1) |
| reduce.md | Reduce as a function from shards to an edit set and report | `slip_reduce` | not written (M2) |
| api.md | `<slip/meta.h>` | `slip_script`, resource headers | not written (M3) |
| macros.md | Token origin, tiers, coverage states | `slip_kiln`, `slip_reduce` | not written (M4) |

Status is one of: `not written`, `draft` (may change freely), `stable`
(changes need a version bump of whatever format or interface they touch).
`cargo xtask lint` checks this table: every file in `spec/` has a row, every
row's file exists unless its status is `not written`, and the status is one
of the three values.

## Conventions

- **Section references.** Sections are numbered and never renumbered once a
  spec is `stable`; a removed section keeps its number with the text
  "removed". Tests and code cite `spec/<file>.md §N.M`.
- **Requirement words.** "Must" and "must not" are requirements that a test
  checks. "May" is a permitted variation that no test relies on. Anything
  described as an error names the error (§ "Errors" in each spec) so tests
  can match it.
- **C++ and Kiln references.** The C++ standard is cited by stable section
  name (`[basic.lookup.argdep]`), never by paragraph numbers of one draft.
  Kiln is cited by spec file and heading, at the revision the `kiln`
  submodule pins.
- **Every spec ends with** `## Tests` (the test files that back it) and
  `## Open questions`.

## Landed sections

A work package that lands behaviour not yet described by its spec records it
in its own file, `spec/landed/<spec>/WP-NNN.md`, first line
`# WP-NNN — <title>`: what the code now does, the rules it follows, the tests
that back them. One file per work package never conflicts between branches;
appending to a shared spec does. Landed files are as normative as the spec.
At each milestone the supervisor folds them back into the base spec and
deletes them, so no spec carries more than one milestone of patches.

`cargo xtask lint` fails on a file under `spec/landed/` that is not
`<spec>/WP-NNN.md` for an existing spec, has the wrong title line, or names a
work package with no file in `docs/wp/`.
