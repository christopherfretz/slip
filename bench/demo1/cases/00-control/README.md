# 00-control: nothing edited

The post-image is the pristine tree made again: every file of `llvm` a
hard link to the file of `<src>/llvm`, nothing written. Nothing is
declared, and the case must be `verified`: it is invariant (b) of
`spec/harness.md` §9.2 on the selection, run through the demo's own
steps (a tree, a database rewritten to it, `slip verify` against it).
If it is not `verified`, no other case's result means anything.

There is no `edit.txt` (§10.1: absent for the control), no
`declared.txt` (nothing declared) and no `observers` (no name to
resolve).

## Checks

On a tree `<tree>` = `<corpus>/trees/wp043/00-control`, made with
`cp -al <src>/llvm <tree>/llvm`, and a copy of the database in which
`<src>/llvm/` is replaced by `<tree>/llvm/` in every `file` and
`command`.

1. **T2, the production compiler.** Every command of the copy whose
   file is under `/llvm/lib/Support/` (151), and those of
   `lib/IR/StructuralHash.cpp` and `unittests/ADT/HashingTest.cpp` (the
   two units outside the selection that `01-relocate` edits), run with
   `-fsyntax-only` in place of `-c` and `-o`, from the entry's
   directory, 6 at a time: **153 of 153 accepted, no warning** (the
   pristine sources give no warning under the corpus's flags, so a
   warning in any other case is one its edit adds).
2. **Kiln.** `slip parse --db <the copy> --store <store> --only
   /llvm/lib/Support/ -j 6`: selected 151, `ok` 151, `diagnosed`,
   `rejected`, `failed` 0. The pristine database gives the same:
   selected 151, `ok` 151. So no observer of the selection is excluded
   from the comparison in any case (none of the 19 units Kiln fails at
   Slip's pin is in `lib/Support`).
3. **The pristine tree is untouched**: `git -C <src> status --short` is
   empty before and after (with `-c core.trustctime=false`, as
   `01-relocate` says why).
4. Not applicable: nothing changes meaning.
