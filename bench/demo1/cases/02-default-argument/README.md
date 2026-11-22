# 02-default-argument: `StringRef::find(char)` now starts at 1

The hazard of `spec/pitfalls.md` E10, a default argument changed on its
one declaration: `StringRef::find(char C, size_t From = 0) const`
(`llvm/include/llvm/ADT/StringRef.h:282`) gets the default `1`. Nothing
is declared. Every call that leaves `From` out now skips the first
character, and no compiler says so: a default argument is not part of
the function's type, and the call is the same call with another
argument.

## What it changes

- **The member** `llvm::StringRef::find(char, unsigned long) const`:
  its parameter's default argument, the expression `0`, is now `1`. Its
  unit is `changed`, at its line (282, the line of its one declaration).
- **A caller that relies on the default**:
  `CommaSeparateAndAddOccurrence(llvm::cl::Option *, unsigned int,
  llvm::StringRef, llvm::StringRef, bool)`, a static function of
  `llvm/lib/Support/CommandLine.cpp` (not a template), which splits a
  `cl::CommaSeparated` option's value at its commas. Its call
  `Val.find(',')` at line 610 is, in the pre-image, `Val.find(',', 0)`
  (the default argument is instantiated at the call: clang's
  `CXXDefaultArgExpr`), and in the post-image `Val.find(',', 1)`. A value
  that begins with a comma (`-opt=,a`) gave the empty first element
  before and does not now. Overload resolution is the same (of
  `find(char, size_t)` and `find(StringRef, size_t)` only the first is
  viable for a `char`); only the argument differs. The
  call at line 619 changes the same way; the expected line is the first.

The other calls in `lib/Support` that leave `From` out change too
(`CommandLine.cpp:501`, in `CommandLineParser::LookupOption`, and
`Regex.cpp:174`, in `Regex::sub`), and so does any inline function of a
header that does (none of `StringRef.h`'s own members does: `contains`
and `split` take other routes): the report may have more findings than
the two lines, and `expect.txt` makes no claim about them. The caller
named is the one whose name has no class of an unnamed namespace and no
`std` type in it.

## Checks

On `<tree>` = `<corpus>/trees/wp043/02-default-argument` (hard links of
`<src>/llvm`, `StringRef.h` written as a new file by a throwaway script
that applies `edit.txt` under the rules of §10.1), and a copy of the
database with `<src>/llvm/` replaced by `<tree>/llvm/`.

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
   at `HEAD`.
4. **The change of meaning**, `clang++-19 … -Xclang -ast-dump -Xclang
   -ast-dump-filter=CommaSeparateAndAddOccurrence` on `CommandLine.cpp`
   in the pristine tree and in `<tree>`: the call `Val.find(',')` at
   line 610 has the argument `CXXDefaultArgExpr` →
   `ImplicitCastExpr <StringRef.h:282:53> <IntegralCast>` →

   ```
   pristine: `-IntegerLiteral <col:53> 'int' 0
   <tree>:   `-IntegerLiteral <col:53> 'int' 1
   ```

   and the same for the call at line 619; the dumps differ in nothing
   else (addresses and the tree's path aside).
