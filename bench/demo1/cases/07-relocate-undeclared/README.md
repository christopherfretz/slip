# 07-relocate-undeclared: the correct relocation, not declared

The tree is `01-relocate`'s (`base`), with no edit of its own (so no
`edit.txt`) and nothing declared (no `declared.txt`). The program is as
correct as `01-relocate`'s, but the claim is plain equality, and the
relocation breaks it: the declared changes are not taken on trust in
either direction, and a correct change that was not declared is not
verified.

## What it gives

Without the rule, the post-image's namespace has the key of
`(llvm::hashing, namespace, impl)`, which the pre-image does not have,
and the pre-image's `(llvm::hashing, namespace, detail)` has no
counterpart:

- `removed llvm::hashing::detail`, at its first declaration in the
  pre-image, `llvm/include/llvm/ADT/Hashing.h:134` (in every observer of
  the selection that includes `Hashing.h` its first block is the first
  declaration: no file of `lib/Support` includes `StableHashing.h`, the
  only other header that declares it);
- `added llvm::hashing::impl`, at its first declaration in the
  post-image, the same line (no line moved).

Every member of the namespace, and every unit that refers to one, also
differs (`removed` and `added` for the members, whose keys have the
namespace as parent; `changed` for the referrers): "among others", and
`expect.txt` makes no claim about them.

## Checks

On `<tree>` = `<corpus>/trees/wp043/07-relocate-undeclared`: hard links
of `01-relocate`'s tree, nothing written; and a copy of the database
with `<src>/llvm/` replaced by `<tree>/llvm/`. The bytes are
`01-relocate`'s; the checks were run on this tree again all the same.

1. **T2** (`clang++-19`, each command with `-fsyntax-only` in place of
   `-c` and `-o`, 6 at a time): the 151 units of `/llvm/lib/Support/`:
   **151 of 151 accepted, no warning**. (`01-relocate` also compiled
   the two units outside the selection that its edit touches, and the
   eight that include `StableHashing.h`.)
2. **Kiln.** `slip parse --db <the copy> --store <store> --only
   /llvm/lib/Support/ -j 6`: selected 151, `ok` 151 (pristine: `ok`
   151).
3. **The pristine tree is untouched**: `git -C <src> status --short`
   empty before and after (`-c core.trustctime=false`, see
   `01-relocate`), and every file an `edit.txt` names hashes to its blob
   at `HEAD`.
4. Not applicable: nothing changes meaning; the case is about what is
   declared, not about the edit.
