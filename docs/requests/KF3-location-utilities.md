# KF3 — location utilities: from a Kiln location to bytes of a file

A request from Slip to Kiln. Status: **draft, not sent.**
Context: [05-kiln-interface.md](../design/05-kiln-interface.md) §3 (row KF3)
and §5 (Slip prototypes KF3 on a Kiln branch, Kiln reviews and lands it);
[spec/edits.md](../../spec/edits.md) §2.2, §2.3 and §9.3 are the consumers.
Kiln owns the design of everything inside Kiln; this states what Slip
needs, proposes a shape, and gives acceptance tests. Names below are
placeholders.

Line references are to Kiln `main` at `0295475a3` unless marked **pin**
(Slip's pin `62d190007`, the head of `slip/kf1`).

§7 is not part of KF3: it reads Kiln's written-reference table (WP-1930,
WP-1931) against what a rename of functions and variables needs, and
lists the follow-ups that belong to KR1.

## 0. What kind of work each part is

| Part | Kind | Why |
|---|---|---|
| U1 The exact extent of the token at a location (§3.1) | **Implementation against a settled specification**: clang's `Lexer::MeasureTokenLength`, by lexing again. Kiln's lexer is public (`kiln_lex/src/lexer.rs:114`, `:134`, `:488`). | Today's `token_end` is a byte heuristic (§2). |
| U2 A location range as a range of file characters, or none (§3.2) | **Implementation against a settled specification**: clang's `Lexer::makeFileCharRange`, which `kiln_sema/src/macro_fixit.rs` already follows. The work is making it public and independent of `Sema`. | |
| U3 The origin of a token: file, macro argument, macro body, pasted or stringised, builtin, command line (§3.3) | **Design, small, with prior art for the pieces** (`isMacroArgExpansion`, `isWrittenInScratchSpace`, `getImmediateMacroCallerLoc`). What has no prior art is the classification itself: Slip's tiers (D8), and in particular when an argument token counts as "reached the parser only by substitution". The table of §3.3 is the proposal; Kiln's review of its edge rows is the design step. | A wrong row makes Slip edit a macro body it believes is an argument, which verification then has to catch. |
| U4 Offsets in the file's bytes (§3.4) | **Design, small.** Kiln's buffer is the decoded text; Slip addresses the bytes on disk. The two differ after an invalid sequence or a byte-order mark. | |
| U5 Enumerating the expansion entries of a unit (§3.5) | **Implementation**; the entries exist (`manager.rs:152`). | Needed by M4, wanted by M2's reports. Not in the prototype. |
| U6 More than one preprocessor observer (§3.6) | **Implementation**, and probably superseded by the preprocessing record (KR4). | Not in the prototype. |

## 1. Need

Slip edits a name where it is written. Kiln gives it the name as a
location range (`WrittenRef.range`, `DeclNode.loc`). Slip must turn that
into `(file, start byte, end byte)`, know whether those bytes are text of
the file that the parser read directly, text of a macro argument, or
something that cannot be edited there, and it must be exact: a range one
byte short corrupts a file that thousands of translation units include.

## 2. Kiln today

| Fact | Where |
|---|---|
| The fix-it logic that maps a range to file characters is private to `Sema`: `file_char_range`, `macro_start_file_loc`, `macro_end_file_loc`, `at_immediate_start`, `at_immediate_end`, `insert_file_loc`, `file_fixits`, `file_token_end` | `kiln_sema/src/macro_fixit.rs:131`, `:91`, `:104`, `:56`, `:73`, `:116`, `:34`, `:27` |
| `token_end` measures a token from the buffer's bytes with ASCII rules: a word is `[A-Za-z0-9_.']`, a punctuator is matched against a table of 24. It is wrong for an identifier with a universal-character-name or a non-ASCII character, for a token with a line splice, and for raw strings | `kiln_sema/src/impcast.rs:552` to `:600` |
| The public form of it, `token_end_of`, is a method of `Sema`, which is consumed before KF1's closure runs | `kiln_sema/src/written_refs.rs:284`; **pin** `kiln_driver/src/front_end.rs:73` (the closure gets `sm`, `ast`, `pp`, no `Sema`) |
| A written reference's `range` is the start of its first token to the end of its last, so for those the end is already exact | `spec/ast.md:1690` (§18.2), `kiln_ast/src/written_refs.rs:144` |
| A declaration has the location of its name and no end | `spec/ast.md:155` (`DeclNode.loc`) |
| Expansion entries are public: kind (`Macro { name }` or `MacroArg`), spelling, expansion range | `kiln_source/src/manager.rs:131`, `:152`, `:1396` |
| So are the walks: `immediate_spelling_loc`, `spelling_loc`, `expansion_loc`, `is_macro_loc`, `is_written_in_scratch_space`, `offset_of`, `entry_bounds` | `manager.rs:1418`, `:1430`, `:1440`, `:1388`, `:1080`, `:1160`, `:1361` |
| There is no iteration over expansion entries | — |
| The preprocessor has one callbacks object | `kiln_pp/src/preprocessor.rs:303`, `:531` |
| A file is decoded on load; invalid UTF-8 becomes U+FFFD and the load report records it | `kiln_source/src/manager.rs:763`, `:1595`, `:1135` |

## 3. Proposed shape

A proposal. Kiln owns the final form. Everything is a function of the
source manager (and, for U1, the lexer options of the unit), so that a
tool holding KF1's `FrontEnd` can call it after the unit is finished.

```rust
// kiln_lex (it has the lexer; kiln_source does not)
/// The token that starts at `at`, lexed again where it is spelled.
/// `None` if no token starts there (end of file, the middle of a token).
pub fn token_at(sm: &SourceManager, at: Loc, opts: &LexOptions) -> Option<Span>;

// kiln_source
pub struct FileRange { pub file: FileId, pub lo: u32, pub hi: u32 }   // offsets in the buffer's text

pub enum Origin {
    /// Written in a file and read by the parser directly.
    File,
    /// Written in a file as (part of) a macro argument and substituted
    /// for a parameter, at every level. `invocation` is the name token
    /// of the outermost invocation.
    Argument { invocation: Span },
    /// Written in a macro's replacement list (in a file).
    Body { invocation: Span },
    /// Made by `#` or `##`, or by a builtin macro.
    Scratch { invocation: Span },
    /// Written in a buffer that is not a file: the command line, the
    /// predefines, a virtual buffer.
    NotAFile,
}

impl SourceManager {
    /// The origin of the token at `at` (a token start).
    pub fn origin(&self, at: Loc) -> Origin;
    /// clang `makeFileCharRange`: the characters of one file that the
    /// range `[lo, hi)` is written as, or `None` when it is not one
    /// contiguous piece of one file.
    pub fn file_char_range(&self, lo: Loc, hi: Loc) -> Option<FileRange>;
    /// The offset in the file's original bytes of a buffer offset, or
    /// `None` if the buffer is not the file's bytes up to there.
    pub fn original_offset(&self, file: FileId, offset: u32) -> Option<u64>;
}
```

### 3.1 U1: `token_at`

Lex one token at the spelling location of `at`, in raw mode, with the
unit's `LexOptions`, and return its span in the terms of `at` (for a macro
location, that far into the same expansion entry, as `token_end` does
today, `impcast.rs:556`). Splices, trigraphs, universal-character-names,
raw strings and user-defined-literal suffixes follow the lexer, because
it is the lexer. The parser's split `>>` (`impcast.rs:553`) stays `Sema`'s
business; `token_at` reports the lexer's token.

Kiln may then implement `token_end` with it; that is Kiln's choice.

### 3.2 U2: `file_char_range`

`macro_fixit.rs:131` made a function of the source manager, with its
callees. Behaviour unchanged; Kiln's fix-it tests keep passing through
the moved code.

### 3.3 U3: `origin` (the design point)

For the token whose first character is at `at`:

| `at` | Result |
|---|---|
| A file location in a file read from disk or through an overlay | `File` |
| A file location in any other buffer | `NotAFile` |
| A macro location whose every enclosing entry, from the token outward, is `MacroArg` until a file location is reached, and that file location's buffer is a file | `Argument` |
| A macro location with an entry of kind `Macro` on that walk whose spelling is in scratch space (`manager.rs:1080`) | `Scratch` |
| A macro location with an entry of kind `Macro` on that walk whose spelling is in a file | `Body` |
| Any macro location whose spelling ends in a buffer that is not a file | `NotAFile` |

Edge rows to settle in review, each with a test in §4:

1. **An argument that names a macro.** `F(G)` where `G` is object-like and
   expands to the token: the token's spelling is in `G`'s body. Proposed:
   `Body` (the walk meets a `Macro` entry before the file).
2. **An argument passed through several macros** (`A(x)` defined as
   `B(x)`): every entry is `MacroArg`. Proposed: `Argument`, `invocation`
   the outermost name (`A`).
3. **An argument inside another argument** (`A(B(x))`): `Argument`.
4. **`__VA_ARGS__` and `__VA_OPT__`**: tokens of the variadic arguments are
   `Argument`; tokens written inside `__VA_OPT__( … )` in the definition
   are `Body`.
5. **An invocation that begins in one macro's body and ends in the file**
   (a function-like macro name from a body, its parenthesis from the
   file): the argument tokens are written in the file. Proposed:
   `Argument`, since their bytes are file text and reached the parser by
   substitution only.
6. **A token both substituted and stringised** (`#x` and `x` in one
   body): two tokens. The substituted one is `Argument`; the string is
   `Scratch`. `origin` answers for one token and does not relate them.
7. **A name of two tokens from different expansions** (`a::b` with `a::`
   from a body and `b` from an argument): `origin` is per token; the
   caller sees two origins, and `file_char_range` is `None`.

### 3.4 U4: `original_offset`

Slip addresses the file's bytes as they are on disk (or in the overlay).
Kiln's `offset_of` (`manager.rs:1160`) is an offset in the decoded
buffer. They are equal unless the load replaced an invalid sequence or
dropped a byte-order mark before the offset. `original_offset` returns
the byte offset when it can be known exactly and `None` otherwise; Slip
refuses to edit past a `None` (`file_recoded`, `spec/edits.md` §2.3).
The cheapest correct form is: `Some(offset)` when the load report shows
no replacement and no mark, `None` otherwise; anything finer is Kiln's
choice.

### 3.5 U5: expansion entries (stage 2)

`SourceManager::expansions() -> impl Iterator<Item = ExpansionInfo>` in
creation order. Slip's M2 reports group refusals by macro and can do it
from `Origin::invocation`; M4 needs the full list and will ask for it
with KR4 if the preprocessing record does not already carry it.

### 3.6 U6: several preprocessor observers (stage 2)

`Preprocessor::set_callbacks` (`preprocessor.rs:531`) takes one object. A
tool that wants its own beside the driver's needs a fan-out. Slip does
not need it for M2 and expects KR4 to replace the need.

## 4. Acceptance tests

Kiln-side, single-file tests over small sources; the expected values are
byte offsets.

1. `token_at` on: an ASCII identifier; an identifier with `é` and
   with a UTF-8 `é`; an identifier split by a backslash-newline; a
   keyword; each punctuator of three, two and one characters; a number
   with an exponent and digit separators; a raw string; a
   user-defined-literal; the last token of a file with no newline; the
   end-of-file location (`None`); the middle of a token (`None`); a macro
   location in a body and in an argument.
2. `token_at` agrees with the `range` of every written reference of
   Kiln's written-reference tests (`kiln_parse/tests/written_refs.rs`)
   whose name is one token.
3. `file_char_range`: the cases of Kiln's fix-it tests through the moved
   function; a range over a whole macro invocation; a range inside one
   argument; a range from an argument into the body (`None`).
4. `origin`: one test per row of the table of §3.3 and per edge row 1 to
   7; a command-line macro (`-DX=f`); a builtin macro (`__LINE__`); a
   token in a header included inside a macro argument list (file, not
   argument).
5. `original_offset`: a plain file (identity); a file with a byte-order
   mark; a file with an invalid byte before and after the offset; an
   overlaid file (KF2).
6. Nothing in §3 changes a diagnostic or a fix-it: Kiln's suites pass
   unchanged.

## 5. Staging

| Stage | Content | Needed by |
|---|---|---|
| 1 | U1, U2, U3, U4 and the tests of §4 | Slip M2: every claim of every transform (`slip_kiln::sites`, `docs/wp/WP-068.md`) |
| 2 | U5, U6 | Slip M4, unless KR4 covers them |

## 6. The prototype

Slip prototypes **stage 1**, on a Kiln branch `slip/kf3`; Kiln reviews
and lands it as a work package.

- **Base.** A commit that has KF1 and the written-reference table
  (WP-1930, WP-1931): test 2 of §4 needs the table and Slip's use needs
  both. KF1 is on `slip/kf1` only and the table on `main` only, so the
  base is `main` once KF1 lands there, or the commit Kiln names. Slip
  does not rebase `slip/kf1` itself.
- **Scope.** `kiln_lex` (`token_at`), `kiln_source` (`Origin`, `origin`,
  `file_char_range`, `original_offset`), `kiln_sema` (`macro_fixit.rs`
  calling the moved functions), tests. Expected under 800 lines with
  tests. The table of §3.3 goes to Kiln's `spec/source.md` in the form
  Kiln's review settles.
- **Order.** The design step is the review of §3.3's rows; the code
  follows it. Slip sends this request, takes Kiln's corrections to the
  table, and only then writes the branch.

## 7. The written-reference table and a rename of functions and variables

What `spec/edits.md` §9.3 must find, against what Kiln records
(`spec/ast.md` §18, `kiln_ast/src/written_refs.rs:44` for the roles,
`:144` for the entry). Stages 1 and 2 are on `main` (WP-1930, WP-1931);
stages 3 to 5 are open (WP-1932, WP-1933, WP-1934,
`docs/BACKLOG.md:1104` to `:1106` of Kiln).

| Reference | Source in Kiln | State |
|---|---|---|
| The name at each declaration and at the definition | Not in the table by design (`spec/ast.md:1764`). `DeclNode.loc` of every redeclaration (`redecls_of`, `kiln_ast/src/ast.rs:784`) plus U1 for the end | **Enough, with KF3.** KR3 (declaration extents) is not needed for a rename |
| An id-expression, an enumerator, `&f`, a member access | Stage 1 roles `Expr`, `Member`, with the function overload resolution selected | **Enough** |
| A qualified name's last component, and its qualifier chain | Stage 1 role `Nns` and `qualifier` | **Enough** (a rename edits no qualifier) |
| A type, template, base or member-initialiser name | Stage 2 | Not needed for functions and variables |
| A using-declaration's name | Stage 3 role `UsingDecl`: **open**. `DeclNode.loc` of the using-declaration is its name's location if Kiln confirms it | **Question for Kiln**: is the `loc` of a `Using` declaration the last name of its declarator? If yes, enough with KF3 |
| The target of an explicit instantiation (`template void f<int>(int);`) | Stage 3 role `SpecTarget`: **open**. No declaration node carries the location | **Missing.** Slip refuses the rename when the AST shows an explicit instantiation of a subject (`unreachable`) |
| An explicit specialisation's name | It is a declaration: `DeclNode.loc` | **Enough, with KF3** |
| A friend declaration that names the function | A redeclaration: `DeclNode.loc`; a friend that only refers (stage 3 `Elaborated`) is for classes | **Enough** for functions |
| A designator, an attribute argument that names an entity | Stage 3 `Designator`; attributes are not in the table at all | **Missing.** Designators concern fields (not renamed in M2). An attribute that holds a subject (`cleanup`) makes Slip refuse (`unreachable`) |
| A dependent callee of a call in a template | Stage 1 records the name with `DEPENDENT` and no entity (`spec/ast.md:1882`); the declarations found at the definition are in the `UnresolvedLookupExpr`, and the entry does not say which expression it belongs to | **Missing link** (follow-up K-a) |
| Any other dependent name, per instantiation | Stage 4 (WP-1933): **open** | **Missing.** Slip claims `unresolved` and reports residue; a rename that needed it does not verify |
| Names in a PCH or a module | Stage 5 (WP-1934): **open** | Not needed for the corpora Slip has |
| Overriding functions | Not a written reference: the cold `overrides` list of a method (`spec/ast.md:324`) | **Enough** |

**Can rename stand on WP-1930 now?** For a function or variable at
namespace scope or in a class that is not a template, used outside
dependent contexts: yes, on stages 1 and 2 with KF3. That covers both
accepted targets of Demo 2 (`docs/demos/demo-2.md`). It cannot yet serve
a member of a class template referred to through a dependent object
(stage 4), and it loses the link from a dependent call to its lookup set
(K-a). Two conditions are outside the table: Slip's pin has neither
stage (it is on `slip/kf1`, which predates WP-1930), so the pin must move
to a commit that has KF1 and both stages; and the table is opt-in
(`ParseOptions::written_refs`, `kiln_parse/src/lib.rs:140`), which KF1's
entry must let a caller set.

Follow-ups for KR1, in the order Slip needs them:

| Id | Ask | For |
|---|---|---|
| K-a | From a `DEPENDENT` entry of role `Expr` to its expression (an `ExprId` in the entry's spare bytes, or a query `Ast::written_ref_expr`), so that a tool reads the definition-context lookup set of the call it names | M2: `spec/edits.md` §9.4 row 1. Demo 2's function target has no such use; the first corpus sample with one (`llvm::PowerOf2Ceil` in `llvm/ADT/DenseSet.h`) needs it |
| K-b | KF1's entry sets `ParseOptions::written_refs` on request (an option of `FrontEndOptions`) | M2, with the pin move |
| K-c | Confirmation of the `Using` declaration's `loc`, or stage 3's `UsingDecl` role first | M2: using-declarations of a renamed function |
| K-d | Stage 3 `SpecTarget` for function and variable templates | lifts the `unreachable` refusal for explicit instantiations |
| K-e | Stage 4 (WP-1933) | rename of members of class templates; M4's namespace migration needs it regardless |
