# 04-adl: a template in `llvm` captures a call inside `llvm::sys::path`

The hazard of argument-dependent lookup (`spec/pitfalls.md` N03 and E02
are its relatives): a function added to a namespace associated with an
argument's type becomes a candidate for unqualified calls far from it,
and wins if it matches better. Added to namespace `llvm`, at the end of
`llvm/include/llvm/Support/Path.h` (after `} // end namespace sys`):

```cpp
/// Whether \p Path begins with a separator of style \p S.
template <typename T = void>
bool has_root_directory(StringRef Path, sys::path::Style S) {
  return !Path.empty() && sys::path::is_separator(Path.front(), S);
}
```

Declared: `created` the template. Nothing else; the finding the case
expects is on a unit nobody declared.

## What it changes

The caller is `llvm::sys::path::is_absolute(const llvm::Twine &,
llvm::sys::path::Style)` (`llvm/lib/Support/Path.cpp`, not a template).
Its call `has_root_directory(p, style)` is at line 676, with `p` a
`StringRef` and `style` a `sys::path::Style` (the file is not edited, so
the line is the same in both images).

- **Before**: ordinary unqualified lookup from inside `llvm::sys::path`
  finds `llvm::sys::path::has_root_directory(const Twine &, Style)` and
  stops. Argument-dependent lookup adds the namespaces of `StringRef`
  (`llvm`) and of `Style` (`llvm::sys::path`), where there is nothing
  more of that name. One candidate; `p` binds to `const Twine &` through
  the converting constructor `Twine(const StringRef &)` (user-defined).
- **After**: argument-dependent lookup through `StringRef`'s namespace
  `llvm` also finds the template; `T` takes its default, `void`. Its
  first parameter is an exact match, better than the user-defined
  conversion to `const Twine &`, the second is equal: the call names
  `llvm::has_root_directory<void>(StringRef, Style)`. (The rule that
  prefers a non-template applies only when the conversions are equally
  good.)

The new function is not the old one: it tests the first character for
a separator, where the real one asks for a root directory after the
root name, so `C:\foo` in the Windows style was absolute and is not
now. That is the change of meaning; even a template that forwarded to
the old function would make the call name another function.

Why a template: a call whose argument is not a `StringRef` exactly (a
`const char *`, a `std::string`, a `SmallString`) needs a user-defined
conversion to either parameter type, and then the non-template wins
the tie. So the edit captures exactly the calls that pass a
`StringRef`, and makes no call ambiguous; the selection compiles (check
1). The other call of the name in `Path.cpp`
(`path::has_root_directory(p)` at line 911) is qualified, so there is no
argument-dependent lookup, and passes one argument: not affected.

## Checks

On `<tree>` = `<corpus>/trees/wp043/04-adl` (hard links of
`<src>/llvm`, `Path.h` written as a new file by a throwaway script that
applies `edit.txt` under the rules of §10.1), and a copy of the
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
   -ast-dump-filter=is_absolute` on `Path.cpp` in the pristine tree and
   in `<tree>`: the callee of `has_root_directory(p, style)` at line 676
   is

   ```
   pristine: DeclRefExpr <col:18> 'bool (const Twine &, Style)' lvalue Function 'has_root_directory' 'bool (const Twine &, Style)'
   <tree>:   DeclRefExpr <col:18> 'bool (StringRef, sys::path::Style)' lvalue Function 'has_root_directory' 'bool (StringRef, sys::path::Style)' (FunctionTemplate 'has_root_directory')
   ```

   (addresses left out), the call is marked `adl` in `<tree>` only
   (`CallExpr <col:18, col:45> 'bool' adl`), and its first argument is a
   `Twine` made by `CXXConstructExpr 'void (const StringRef &)'` in the
   pristine tree and a copy of the `StringRef` in `<tree>`.
