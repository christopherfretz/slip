# 06-relocate-and-change: a constant changed inside the relocated namespace

What the tool exists for: a semantic change riding on a mechanical one.
The tree is `01-relocate`'s (`base`), and on it one constant of the
relocated namespace is changed: in `llvm/include/llvm/ADT/Hashing.h`,
line 156,

```cpp
static constexpr uint64_t k3 = 0xc949d7c7509e6557ULL;   // before
static constexpr uint64_t k3 = 0xc949d7c7509e6577ULL;   // after
```

one hexadecimal digit of one of the four mixing primes, the kind of
slip a large mechanical diff hides. Declared: what `01-relocate`
declares, the relocation and nothing else (`declared.txt` is the same).

## What it changes

The variable `k3` is in the relocated namespace, so in the post-image
it is `llvm::hashing::impl::k3`; under the declared rule its key is that
of `llvm::hashing::detail::k3`, the unit is compared with the
pre-image's, and its initialiser differs. The finding is reported in
pre-image terms (`spec/verify.md` §6.1: the name of the key's unit in
the pre-image): `changed llvm::hashing::detail::k3`, at line 156 of the
post-image (no line moved). Every hash that mixes in `k3` (`hash_short`
and the functions that call it) computes another value; whether their
own units differ depends on whether the walk writes the referenced
constant's value or a reference to it, and `expect.txt` makes no claim
about them.

`k3` is `static constexpr` at namespace scope: internal linkage, one
variable per translation unit, no symbol shared across units. It is not
used in a constant expression that the selection checks (no
`static_assert`, no template argument), so the tree compiles (check 1).

## Checks

On `<tree>` = `<corpus>/trees/wp043/06-relocate-and-change`: hard
links of `01-relocate`'s tree (whose four edited files are themselves
new files, not links into `<src>`), then `Hashing.h` written as a new
file again by a throwaway script that applies `edit.txt` under the
rules of §10.1; and a copy of the database with `<src>/llvm/` replaced
by `<tree>/llvm/`.

1. **T2** (`clang++-19`, each command with `-fsyntax-only` in place of
   `-c` and `-o`, 6 at a time): the 151 units of `/llvm/lib/Support/`:
   **151 of 151 accepted, no warning** (none on the pristine tree). The
   edit is silent.
2. **Kiln.** `slip parse --db <the copy> --store <store> --only
   /llvm/lib/Support/ -j 6`: selected 151, `ok` 151 (pristine: `ok`
   151).
3. **The pristine tree is untouched**: `git -C <src> status --short`
   empty before and after (`-c core.trustctime=false`, see
   `01-relocate`), and every file an `edit.txt` names hashes to its blob
   at `HEAD`; `01-relocate`'s `Hashing.h` still has the old value.
4. **The change of meaning**, `clang++-19 … -Xclang -ast-dump -Xclang
   -ast-dump-filter=k3` on `StringRef.cpp` in the pristine tree and in
   `<tree>`: the variable, under its old and its new name, is

   ```
   pristine: Dumping llvm::hashing::detail::k3:  …  `-IntegerLiteral <col:32> 'unsigned long long' 14504361325974414679
   <tree>:   Dumping llvm::hashing::impl::k3:    …  `-IntegerLiteral <col:32> 'unsigned long long' 14504361325974414711
   ```

   (both `VarDecl <llvm/include/llvm/ADT/Hashing.h:156:1, col:32> …
   static constexpr cinit`, with `value: Int` the same two numbers).
