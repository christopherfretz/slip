# 05-shadow: a local `Length` hides `StringRef::Length`

The hazard of `spec/pitfalls.md` E12: a declaration inserted in an
inner scope of a function body gives a name used later in that scope
another meaning. In `llvm::StringRef::compare_numeric(llvm::StringRef)
const` (`llvm/lib/Support/StringRef.cpp`, not a template), the loop
header at line 62

```cpp
for (size_t I = 0, E = std::min(Length, RHS.Length); I != E; ++I) {
```

gains a declarator, a local copy of the other string's length:

```cpp
for (size_t I = 0, Length = RHS.Length, E = std::min(Length, RHS.Length); I != E; ++I) {
```

Nothing is declared. The edit adds no line, so line 62 is the line in
both images.

## What it changes

- **Before**: `Length` in `std::min(Length, RHS.Length)` (line 62) and
  in `J < Length` (line 69) is found by unqualified lookup in the class
  scope: the data member `StringRef::Length`, read through `this`.
- **After**: the declarator `Length` is a variable of the `for`
  statement's scope, declared before the declarator `E` and before the
  body, so both uses find it: `E` is `std::min(RHS.Length, RHS.Length)`
  and `J < Length` tests against the other string's length. Comparing
  `"abc"` with `"ab"` now reads `RHS.Data[2]`, past the end. The uses
  after the loop (lines 86–88) are outside the `for` scope and still
  find the member.

The finding is `changed` that function at line 62, which is the first
use: the inserted declaration and the first use are on one line, so
the place is the use however the site of the difference falls among
the loop header's marks (the `ForStmt`, its `DeclStmt`, the
initialisers: all on line 62). An inserted declaration on a line of its
own would put the first difference, and so the site, at the
declaration's line rather than at a use.

Shadowing a member by a local in an inner scope is legal and silent
under the corpus's flags: `-Wshadow` and its variants are not among
them (check 1 finds no warning).

## Checks

On `<tree>` = `<corpus>/trees/wp043/05-shadow` (hard links of
`<src>/llvm`, `StringRef.cpp` written as a new file by a throwaway
script that applies `edit.txt` under the rules of §10.1), and a copy of
the database with `<src>/llvm/` replaced by `<tree>/llvm/`.

1. **T2** (`clang++-19`, the command of the edited file, which is also
   the named function's, with `-fsyntax-only` in place of `-c` and
   `-o`): `lib/Support/StringRef.cpp` **accepted, no warning** (none on
   the pristine tree). The edit is silent. No other unit includes the
   file.
2. **Kiln.** `slip parse --db <the copy> --store <store> --only
   /llvm/lib/Support/ -j 6`: selected 151, `ok` 151 (pristine: `ok`
   151).
3. **The pristine tree is untouched**: `git -C <src> status --short`
   empty before and after (`-c core.trustctime=false`, see
   `01-relocate`), and every file an `edit.txt` names hashes to its blob
   at `HEAD`.
4. **The change of meaning**, `clang++-19 … -Xclang -ast-dump -Xclang
   -ast-dump-filter=compare_numeric` on `StringRef.cpp` in the pristine
   tree and in `<tree>`: the first argument of `std::min` in the loop
   header (line 62) is

   ```
   pristine: MemberExpr <col:35> 'const size_t':'const unsigned long' lvalue ->Length
   <tree>:   DeclRefExpr <col:56> 'size_t':'unsigned long' lvalue Var 'Length' 'size_t':'unsigned long'
   ```

   (addresses left out; the pristine `MemberExpr` has `CXXThisExpr …
   implicit this` as its base), and the `<tree>` dump has the new
   `VarDecl … used Length 'size_t':'unsigned long' cinit` initialised
   from `RHS.Length`. The use `J < Length` (line 69) changes the same
   way.
