# KE1 comparison — the differential test of `written_extent` against clang 19

- From: Slip
- To: Kiln
- Status: **accepted** (2026-11-22). Kiln owns the script: its WP-2327
  writes it in Kiln as `cargo xtask extents` from this file, so that it
  gates Kiln's own CI. §1's line format and §3's matching without an
  enclosing function are accepted as written
- Belongs to: [KE1](KE1-expression-extents.md) §3

| Part | Nature |
|---|---|
| §1 the dump Kiln writes | **Implementation against a settled specification** (this file) |
| §2 to §5 the normalisation of clang's dump, the matching, the lists, the verdict | **Settled here**; implementation is a script, which Kiln writes (its `cargo xtask extents`) |
| §6 macros | **Design that follows Kiln's rule**: it restates the rule of `kiln_sema/src/macro_fixit.rs` as KE1 §2.2 asked for it; if WP-2328 decides the rule differently, §6 follows WP-2328, not the reverse |

The test answers one question: when `written_extent` says `Some`, is it
the range a second front end reports for the same expression? It does
not test what `None` should be, only that the share of `None` does not
grow.

## 1. Kiln's side: one line per expression of a written body

A way to make the front end print, for one translation unit, every
expression of every **written** body and initialiser: the bodies of
functions, the initialisers of variables, fields, enumerators and
parameters, constraint and `noexcept` expressions, in declarations that
are not instantiations (a template's pattern is written; its
specialisations' bodies, which repeat the pattern's locations, are left
out, and so are implicit members). How it is asked for is Kiln's choice
(an option of the `kiln` executable; a function Slip calls through the
front-end entry is as good).

One line per expression node reached from those roots, each node once,
in any order:

```
<kind> TAB <file> TAB <begin> TAB <end>
```

- `kind`: Kiln's `ExprKind` name.
- `file`: the real path of the file both offsets are in.
- `begin`, `end`: byte offsets in that file, `end` exclusive (one past
  the last byte of the last token), in decimal.
- `written_extent` `None`: the line has `-` for `end`, and for `begin`
  the offset of the node's own location if that is file text, else `-`
  with `file` `-`. These lines feed the `None` count only.
- A `Some` whose two locations are not file text of one file after
  the rule of §6 cannot be written in this form: print
  `<kind> TAB ! TAB <what> TAB -` and the test fails on it (a `Some` must
  always be expressible as a range of bytes of one file; KE1 §2.2).

## 2. Clang's side, normalised to the same lines

`clang++-19 <the unit's command> -fsyntax-only -Xclang -ast-dump=json`.
From the JSON, every node whose `kind` ends in `Expr`, `Operator` or
`Literal`, or is one of the kinds of the table in §4, under a written
body as §1 defines it. What clang prints that §1 leaves out is skipped
by subtree:

- every `FunctionDecl`, `CXXMethodDecl` and their kin, `VarDecl`,
  `CXXRecordDecl` printed as a specialisation: a child of a
  `FunctionTemplateDecl`, `VarTemplateDecl` or `ClassTemplateDecl` other
  than its first templated declaration, and every
  `ClassTemplateSpecializationDecl` and
  `VarTemplateSpecializationDecl` that has no written definition of its
  own (clang prints the instantiated members; an explicit specialisation
  is written and is kept);
- every declaration with `"isImplicit": true`.

A node's range is `range.begin` and `range.end`. Clang elides `file`
and `line` from a location when they equal those of the location it
printed last, in document order: the reader carries the last file
forward, over every location of the dump, not only those of kept nodes.
A location object is *file text* when it has `offset` at its top level
and neither `spellingLoc` nor `expansionLoc`. For a node whose two ends
are file text of one file:

```
begin = range.begin.offset
end   = range.end.offset + range.end.tokLen
```

A node with an end in a macro goes through §6 or is counted as
`macro-skipped`. A node with an invalid end (no `offset`) is skipped
and counted.

The dump of a real translation unit is gigabytes: the reader streams it
(no whole-document parse) and keeps only the lines. For the first
stage's inputs (§5) the dumps are small.

## 3. Matching

Both sides become multisets of `(file, kind', begin, end)`, where
`kind'` is the kind after the table of §4. For each `(file, kind',
begin)`:

- `K`: the multiset of Kiln's `end`s (the `Some` lines);
- `C`: the multiset of clang's `end`s.

The key **passes** when `K ⊆ C` as multisets. Each element of `K` not
in `C` is a **mismatch**, reported with both sides' lines for the key
and the source text of the two ranges. Elements of `C` not in `K` are
not failures (Kiln said `None`, or does not build the node); they are
counted per kind as `clang-only`.

Two nodes of one kind that begin at one byte (`a + b + c`: both
additions begin at `a`) are told apart by their ends, which is what the
multiset does; nothing needs an enclosing function, because §1 and §2
visit each written body once.

## 4. Kinds

Kiln's `ExprKind` names follow clang's, and a kind is compared with the
clang kind of the same name unless this table says otherwise. The table
is a checked-in text file, one line per entry, and every entry carries
its reason:

```
map   <KilnKind> <ClangKind>[,<ClangKind>…]     the node Kiln builds corresponds to these
skip  <KilnKind>                                 not compared: modelled differently (reason)
none  <KilnKind>                                 `written_extent` is `None` by decision (reason)
```

Entries that the three decisions of KE1 §2.3 give:

| Entry | Reason |
|---|---|
| `none CXXDefaultArgExpr`, `none CXXDefaultInitExpr` | nothing is written at the use; clang reports the location of the call or constructor, which is not an extent of the node |
| implicit conversions, materialised temporaries, bound temporaries, `ExprWithCleanups`, `ConstantExpr`: compared, no entry | clang gives them the range of the written expression inside, which is what KE1 §2.3 row 1 asks of Kiln |
| `CXXRewrittenBinaryOperator`: compared, no entry | clang's range is the written `a != b` (row 2). The nodes inside it (the call of `operator==`, the synthesised comparison with 0) are `None` on Kiln's side and `clang-only` here |
| `MemberExpr` with an implicit `this`: compared, no entry | both begin at the member's name (row 3); the implicit `CXXThisExpr` is `None` |

A `skip` entry is a debt: the list is printed with the result, and an
entry is added only with a three-line source that shows the two front
ends building different nodes.

A `none` line on Kiln's side whose kind has no `none` entry counts
toward the `None` share of its kind.

## 5. Inputs, stages and the verdict

| Stage | Inputs | Gate |
|---|---|---|
| 1 | Kiln's conformance inputs that both front ends accept without error | no mismatch |
| 2 | the translation units of `llvm/lib/Support` of LLVM 19.1.7 with their database commands (151 units; Slip's Demo 1 selection), and one unit that includes most of libc++ | no mismatch outside macros |
| 3 | the whole LLVM database | reported, not gated, until stage 2 has been clean for a pin |

The result of a run is text, sorted, the same for the same inputs:

```
extents: <units> units, <n> compared, <m> mismatches, <s> macro-skipped
kind <Kind>: some <a> none <b> clang-only <c> mismatch <d>
…
mismatch <file>:<begin>-<end> <KilnKind> kiln "<text>" clang <begin>-<end> "<text>"
…
```

- **Pass**: no `mismatch` line, no `!` line of §1.
- **The `None` share**: `b / (a + b)` per kind is recorded with the
  Kiln revision; a later revision passes only if no kind's share is
  larger on the same inputs. Kiln's table of `None` kinds in
  `kiln/spec/ast.md` (KE1 §2.1) lists exactly the kinds with `b > 0`.
- A unit that either front end rejects is listed and not compared.

## 6. Macros

Compared only where both front ends can name file text:

| Case | Clang's locations | The range compared |
|---|---|---|
| both ends in the **same argument** of one macro expansion, and the argument's tokens are file text | each end has `spellingLoc` with `offset` in a file and `expansionLoc` in the same expansion | the spelling offsets, `end` + its `tokLen` |
| the node is the **whole** expansion: its first token is the expansion's first and its last the expansion's last | `expansionLoc` of both ends equal | the expansion's range in the file (the macro's name through its closing parenthesis, or the name alone for an object-like macro) |
| anything else (an end inside a macro's body, ends in different arguments, a token pasted or stringised) | | `macro-skipped` on clang's side; on Kiln's side `written_extent` should be `None`, and a `Some` there is reported as `macro-some` (not a failure in stage 2; a failure once WP-2328 states the rule) |

Whether clang's elided `expansionLoc` identifies "the same expansion"
is decided by the offsets of `expansionLoc`: two ends are of one
expansion when their `expansionLoc.offset` are equal and both are
marked or both unmarked `isMacroArgExpansion`.

## 7. What Slip uses it for

The same run is Slip's check at every move of its Kiln revision
(`spec/api.md` §5.4: a transform edits only ranges of tier 0 or 1, and
a wrong extent is a wrong edit that verification catches late and at a
cost). Slip runs Kiln's script over stage 2's inputs when it moves its
revision, once `written_extent` is on the revision it pins.
