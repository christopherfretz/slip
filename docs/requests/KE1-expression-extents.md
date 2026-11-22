# KE1 — the extent of an expression, exact or absent

A request from Slip to Kiln. Status: **draft, not sent.**
Context: `spec/api.md` §6.6 (`extent_of`) and §6.7 (`arguments_as_written`);
the reading of Kiln main `0295475a3` is in
[kiln-landed-2026-11.md](../design/notes/kiln-landed-2026-11.md) §5.3.

## Design or implementation

| Part | Kind |
|---|---|
| §2.1 An end location that is exact or absent, for every expression kind | **Implementation against a settled specification**: the specification is the C++ grammar, and the oracle is clang's `SourceRange` on the same input. |
| §2.2 The end of a token and a file range, from the source manager alone | **Implementation.** Part of row KF3 of [05-kiln-interface.md](../design/05-kiln-interface.md); repeated here because the two are needed together. |
| §2.3 What "the extent" is for an expression whose tokens are not contiguous in the grammar | **Design, small.** Three cases listed below; Slip proposes an answer for each and Kiln decides. |

## 1. Need

A Slip transform moves expressions as text: it copies the bytes of one
argument to the place of another. For that the host must know the first
and the last token of an expression exactly. A range that is one token
short produces code that still parses, means something else, and sits in
a declaration the transform has told verification to expect a change in.
So Slip's API says: `extent_of` returns the exact range or nothing, never
an approximation.

Kiln's tree does not promise that today. `Ast::end_loc`
(`kiln/crates/kiln_ast/src/ast.rs:122`) "falls back to the node's own
location when no closing token is stored" (comment at `:116-121`). For a
call the `)` is stored (`kiln/crates/kiln_sema/src/expr.rs:7486`). For
other kinds it depends on the kind, and nothing says which kinds are
exact.

## 2. Request

### 2.1 Exact or absent

A function on the finished tree,

```rust
/// The first and last token of `e` as written, or `None` when the tree
/// does not record enough to know.
pub fn written_extent(&self, e: ExprId) -> Option<(Loc, Loc)>;   // names are Kiln's
```

with the guarantee that a `Some` is exact for every expression kind, and a
table in `kiln/spec/ast.md` of the kinds for which it is `None` today.
Slip treats `None` as "do not edit here". The table is expected to shrink.

Kinds Slip needs first, because they are what call arguments are made of
in LLVM and libc++: names and member accesses; literals of every kind,
including user-defined and concatenated string literals; calls and
construction expressions; unary and binary operators, including
overloaded ones written as operators; subscripts; casts of every
spelling; `this`; lambdas; parenthesised expressions; conditional
expressions; `new` and `delete`; braced initialiser lists; pack
expansions; `sizeof` and friends; fold expressions.

### 2.2 From the source manager alone

`Sema::token_end_of`
(`kiln/crates/kiln_sema/src/written_refs.rs:284`) exists while the unit is
being analysed. Slip runs after. Requested, on the source manager or
beside it: the end of the token that starts at a location; and "this pair
of locations as a range of bytes of one file, or none", with the rule for
macro arguments and whole expansions that `kiln_sema/src/macro_fixit.rs`
already implements for fix-its.

### 2.3 Three small design questions

| Case | Slip's proposal |
|---|---|
| An implicit node wrapped around a written one (an implicit conversion, a materialised temporary, a default argument) | The extent of the written node inside; `None` for a node with nothing written (a default argument) |
| An expression built by the compiler from a written operator (a rewritten comparison `a != b` calling `operator==`, a synthesised three-way comparison) | The extent of what was written (`a != b`) on the outermost built node; `None` on the inner ones |
| A postfix expression whose callee is a member access with an implicit `this` (`f(x)` inside a member function) | The extent starts at `f` |

## 3. Acceptance: a differential test

For each expression of a written body, compare `written_extent` with the
range clang 19 reports for the corresponding node
(`clang -Xclang -ast-dump=json` gives `range.begin` and `range.end` with
offsets, expansion and spelling locations).

- Inputs: Kiln's conformance inputs, then libc++'s headers and a slice of
  LLVM's sources, as for the identifier accounting of KR1 §4.2.
- Matching: by expression kind and begin offset within one enclosing
  function; expressions clang and Kiln model differently are listed and
  skipped by kind, the list checked in.
- Pass: every `Some` equals clang's range; the fraction of `None` is
  printed per kind and does not grow.

Slip can supply the comparison tool. It is also how Slip would validate
each move of its Kiln revision.

## 4. Until this lands

Slip's host returns an extent only for the kinds it can prove exact from
the tree as it is (a call, a name, a literal, a parenthesised expression,
and compositions of those whose last token is one of them), and null for
the rest. The first demonstration's call sites have names as arguments.
