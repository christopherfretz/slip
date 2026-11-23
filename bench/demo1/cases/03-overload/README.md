# 03-overload: `StringRef::starts_with(const char *)` wins for a literal

The hazard of an added overload that an existing call now prefers
(`spec/pitfalls.md` R01 and E05 are its relatives: a call that chose one
member of an overload set now chooses another). `StringRef` gains

```cpp
[[nodiscard]] bool starts_with(const char *Prefix) const {
  return starts_with(StringRef(Prefix));
}
```

after `starts_with(char)` (`llvm/include/llvm/ADT/StringRef.h`, lines
257–259 of the post-image). It is declared `created`: the new member
may exist in the post-image only. Nothing else is declared, and the
finding the case expects is on a unit nobody declared.

## What it changes

The caller is `llvm::consumeSignedInteger(llvm::StringRef &, unsigned
int, long long &)` (`llvm/lib/Support/StringRef.cpp`, not a template);
its call `Str.starts_with("-")` is at line 461 (the file is not edited,
so the line is the same in both images).

- **Before**: the candidates are `starts_with(StringRef) const` and
  `starts_with(char) const`. `"-"` (`const char[2]`) does not convert to
  `char`; it converts to `StringRef` by the converting constructor
  `StringRef(const char *)`, a user-defined conversion sequence. The call
  names `starts_with(StringRef)`.
- **After**: `starts_with(const char *) const` is viable with an exact
  match (array-to-pointer conversion, an lvalue transformation), which is
  better than any user-defined conversion sequence. The call names the
  new member.

The new member forwards to the old one, so the program computes the
same value: the change of meaning is that the call names another
function, and that is what the case shows. A forwarding overload is how
the hazard looks in practice (the overload is added "for speed" or "for
convenience"), and it is exactly what a verifier must not wave through:
whether the new function does the same thing is a fact about its body,
which another edit can change without touching the caller.

Every other call with a string literal or a `const char *` that has
`StringRef` in scope changes the same way (`APFloat.cpp:3182`,
`CommandLine.cpp:425`, `DebugCounter.cpp:58`, …): the report has more
findings, and `expect.txt` makes no claim about them. A call with a
`StringRef`, a `std::string` or a `SmallString` is unchanged (they do
not convert to `const char *`).

## Checks

On `<tree>` = `<corpus>/trees/wp043/03-overload` (hard links of
`<src>/llvm`, `StringRef.h` written as a new file by a throwaway script
that applies `edit.txt` under the rules of §10.1), and a copy of the
database with `<src>/llvm/` replaced by `<tree>/llvm/`.

1. **T2** (`clang++-19`, each command with `-fsyntax-only` in place of
   `-c` and `-o`, 6 at a time): the 151 units of `/llvm/lib/Support/`:
   **151 of 151 accepted, no warning** (none on the pristine tree). No
   call becomes ambiguous; the edit is silent.
2. **Kiln.** `slip parse --db <the copy> --store <store> --only
   /llvm/lib/Support/ -j 6`: selected 151, `ok` 151 (pristine: `ok`
   151).
3. **The pristine tree is untouched**: `git -C <src> status --short`
   empty before and after (`-c core.trustctime=false`, see
   `01-relocate`), and every file an `edit.txt` names hashes to its blob
   at `HEAD`.
4. **The change of meaning**, `clang++-19 … -Xclang -ast-dump` of
   `StringRef.cpp` in each tree (the whole unit, so that one dump holds
   both the call and the members it may name): the callee of
   `Str.starts_with("-")` (line 461) is

   ```
   pristine: MemberExpr <col:8, col:12> '<bound member function type>' .starts_with 0xaaaae0c0ebc8
   <tree>:   MemberExpr <col:8, col:12> '<bound member function type>' .starts_with 0xaaab0694ae80
   ```

   where, in the same dumps, `0xaaaae0c0ebc8` is `CXXMethodDecl …
   line:250:24 used starts_with 'bool (StringRef) const'` and
   `0xaaab0694ae80` is `CXXMethodDecl … line:257:24 used starts_with
   'bool (const char *) const'`. The argument differs to match: in the
   pristine tree `"-"` is converted by `CXXConstructExpr 'void (const
   char *)'` (`<ConstructorConversion>` to `StringRef`), in `<tree>` it is
   passed as `'const char *' <ArrayToPointerDecay>`.
