# 01-relocate: `llvm::hashing::detail` becomes `llvm::hashing::impl`

The correct change the demo stands on: the namespace that holds the
implementation of `llvm/ADT/Hashing.h` is renamed, every declaration of
it and every use of its name in `llvm/` together, so that the tree is a
consistent program and not only one that the selection happens to
accept. Declared: the one relocation, written as the rule that undoes
it (`declared.txt`). Expected: `verified`.

## The edit

`edit.txt`, eighteen replacements in four files; no line is added or
removed, so every line number of the post-image is that of the
pre-image.

| File | What |
|---|---|
| `llvm/include/llvm/ADT/Hashing.h` | the three blocks `namespace detail { … } // namespace detail` (lines 134–460, 478–579, 602–617) become `impl`; the four qualified uses `::llvm::hashing::detail::` (lines 472, 595, 624, 631) become `::llvm::hashing::impl::` |
| `llvm/include/llvm/ADT/StableHashing.h` | its block (lines 28–48, `FNV_PRIME_64`, `FNV_OFFSET_64`, `stable_hash_append`) and its eighteen uses `hashing::detail::` (lines 52–106) |
| `llvm/lib/IR/StructuralHash.cpp` | the use `hashing::detail::hash_16_bytes` (line 29) |
| `llvm/unittests/ADT/HashingTest.cpp` | its block (lines 43–45), which holds the explicit specialisation `is_hashable_data<LargeTestInteger>` |

`docs/wp/WP-043.md` and `spec/harness.md` §10.2 say "both blocks" of
`Hashing.h`: there are three, and the namespace has five blocks in
`llvm/` in all. The closing comments are renamed with the blocks; they
change no meaning, and a rename that left them would not be one a
person would commit.

## The search

Over the whole of `<src>` (every directory of the monorepo, not only
`llvm/`), for every way the namespace or its members can be named:

- `hashing::detail`, `hashing\s+::`, `namespace hashing`,
  `using namespace (::)?(llvm::)?hashing`, `hashing;` (a using-directive
  or a namespace alias): the files above, `bolt/lib/Profile/StaleProfileMatching.cpp` and the IR
  test `llvm/test/CodeGen/PowerPC/pr15031.ll` (below), and comments;
- every identifier the namespace declares (`fetch64`, `fetch32`,
  `shift_mix`, `hash_16_bytes`,
  `hash_1to3_bytes` … `hash_33to64_bytes`, `hash_short`, `hash_state`,
  `get_execution_seed`, `is_hashable_data`, `get_hashable_data`,
  `store_and_advance`, `hash_combine_range_impl`,
  `hash_combine_recursive_helper`, `hash_integer_value`, `FNV_PRIME_64`,
  `FNV_OFFSET_64`, `stable_hash_append`; not `rotate` and `k0`…`k3`,
  too common to search for alone, which the first search covers when
  qualified) as a whole word, outside the two headers: the uses above; `llvm/lib/IR/LLVMContextImpl.cpp:195`, a
  function `llvm::get_hashable_data(const MDOperand &)` that is not a
  member of the namespace (below); a comment in
  `clang/test/CodeGenCXX/catch-undef-behavior.cpp`;
- the generated headers of the build tree (`build/include`,
  `build/lib`, `build/unittests`): no use;
- the new name: `namespace impl` and `impl::` in `llvm/include` and
  `llvm/lib/Support`, and the identifier `impl` in the two headers: none,
  so no namespace `llvm::hashing::impl` exists to merge with and no
  lookup of `impl` from inside `llvm::hashing` found anything before.

Left as they are, and why:

- `bolt/lib/Profile/StaleProfileMatching.cpp:316` and `:329` name
  `hashing::detail::hash_16_bytes`. `bolt` is outside the subject root
  `llvm` (an `edit.txt` path may not leave the roots, §10.1) and is not
  a project of this configuration (`LLVM_ENABLE_PROJECTS=clang`): no
  entry of the database compiles it. A rename of the monorepo would
  edit it too.
- `llvm/test/CodeGen/PowerPC/pr15031.ll:262–263` spell
  `struct.llvm::hashing::detail::…` as LLVM IR type names: test input
  text, not C++, and not compiled.

## Why it is correct: every lookup the rename touches

The members of the namespace do not move: the namespace that held them
keeps its place (a member of `llvm::hashing`) and every member, and
only its name changes. What could change is a lookup that goes through
that name, or that finds the name `impl` or no longer finds `detail`.

1. **Qualified names** `::llvm::hashing::detail::X` (`Hashing.h`) and
   `hashing::detail::X` (`StableHashing.h` inside `namespace llvm`;
   `StructuralHash.cpp` in an unnamed namespace under `using namespace
   llvm`): the lookup of `hashing` is untouched; `impl` in
   `llvm::hashing` finds the renamed namespace and nothing else (no other
   member of `llvm::hashing` is called `impl`); `X` in it finds what `X`
   found in `detail`, since every block was renamed and the namespace has
   the same members. Each use is renamed, so none is left to find
   nothing.
2. **Unqualified names inside the blocks** (`fetch64`, `hash_16_bytes`,
   `k1`, `install_fatal_error_handler`, …): unqualified lookup walks the
   enclosing scopes, `detail`/`impl`, then `llvm::hashing`, `llvm`, the
   global namespace; the scopes are the same entities with the same
   members in both trees. No block names `detail` or `impl` unqualified.
   (One that did would be the danger of an incomplete rename: from
   inside `llvm::hashing`, `detail::X` would now find `llvm::detail`,
   which exists.)
3. **The names the rename adds and removes in `llvm::hashing`**: `impl`
   was found by no lookup before (it is declared nowhere in the scopes
   that see `llvm::hashing`); `detail` is still named, after the edit,
   only by the comments; every reopening of `llvm::hashing::detail` in
   `llvm/` was renamed, so no file reopens a namespace `detail` that
   would now be a new, different one (which for `HashingTest.cpp`'s
   explicit specialisation would also be ill-formed).
4. **`using ::llvm::hash_value; return hash_value(value);`** in
   `get_hashable_data` (`Hashing.h:366–367`): the using-declaration is
   fully qualified and names `::llvm::hash_value`, which the edit does
   not touch. The call is unqualified with a dependent argument: its
   candidates are what ordinary lookup finds at the definition (the
   using-declaration, in the block scope, which stops the search there,
   unchanged) and what argument-dependent lookup finds at the point of
   instantiation in the namespaces associated with the argument's type.
   Those are namespaces as entities (the innermost enclosing namespaces
   of the class and of its template arguments), never looked up by name:
   for a type declared in the renamed namespace (`hash_state`,
   `hash_combine_recursive_helper`, the specialisations of
   `is_hashable_data`) the associated namespace is the renamed one, with
   the same members, and no `hash_value` is declared in it. For the
   types the selection hashes (`StringRef`, `hash_code`,
   `std::pair`, …) the associated namespaces are `llvm` and `std`,
   untouched.
5. **The other unqualified calls with dependent arguments**
   (`get_hashable_data(*first)`, `get_hashable_data(arg)`,
   `store_and_advance(…)`, `combine_data(…)`): the same argument. The
   customisation the design relies on, `llvm::get_hashable_data(const
   MDOperand &)` in `LLVMContextImpl.cpp`, is found by
   argument-dependent lookup through `MDOperand`'s namespace `llvm`,
   before and after; the candidates found at the definition are the
   members of the renamed namespace, the same functions.
6. **What the rename does change, and is not meaning**: the names of the
   entities (so the mangled names of the inline functions and of the
   instantiations, and the spelling of the types in IR and debug
   information). Every one is declared by the relocation; no source in
   `llvm/` spells a member's name as a string or uses `__func__` or
   `__PRETTY_FUNCTION__` in the namespace.

## Checks

On `<tree>` = `<corpus>/trees/wp043/01-relocate` (hard links of
`<src>/llvm`, the four edited files written as new files by a
throwaway script that applies `edit.txt` under the rules of §10.1:
every old text found exactly once, or at least once with `all`, as
bytes and as whole lines alike), and a copy of the database with
`<src>/llvm/` replaced by `<tree>/llvm/`.

1. **T2, the production compiler** (`clang++-19`, each command of the
   copy with `-fsyntax-only` in place of `-c` and `-o`, from the entry's
   directory, 6 at a time): the 151 units of `/llvm/lib/Support/`,
   `lib/IR/StructuralHash.cpp` and `unittests/ADT/HashingTest.cpp`:
   **153 of 153 accepted, no warning** (the pristine tree gives none
   either, `00-control`). No unit of the selection includes
   `StableHashing.h`, so its edit was also compiled in every unit of the
   database that includes it or a header that does
   (`lib/Passes/StandardInstrumentations.cpp`,
   `lib/CodeGen/MachineOperand.cpp`, `MIRVRegNamerUtils.cpp`,
   `MachineStableHash.cpp`, `lib/CodeGenData/OutlinedHashTree.cpp`,
   `OutlinedHashTreeRecord.cpp`, and the two unit tests of
   `unittests/CodeGenData/`): **8 of 8 accepted, no warning**.
2. **Kiln.** `slip parse --db <the copy> --store <store> --only
   /llvm/lib/Support/ -j 6`: selected 151, `ok` 151 (`diagnosed`,
   `rejected`, `failed` 0); on the pristine database, `ok` 151. Every
   observer is compared.
3. **The pristine tree is untouched**: `git -C <src> status --short`
   empty before and after (run with `-c core.trustctime=false`: a hard
   link changes a file's ctime, not its content, and git would otherwise
   re-read every linked file), and each file any case's `edit.txt` names
   hashes, in `<src>`, to its blob at `HEAD` (`git hash-object` against
   `git rev-parse HEAD:<path>`), before and after.
4. Not applicable: the case changes no meaning.
