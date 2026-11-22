# KF3 — location utilities: from a Kiln location to bytes of a file

A request from Slip to Kiln. Status: **draft, never sent. Stage 1 is
delivered by Kiln WP-2150 at `913a490bb`** ("location utilities for
tooling clients (KF3)"), and so is what this draft called stage 2.
Context: [05-kiln-interface.md](../design/05-kiln-interface.md) §3 (row
KF3); [spec/edits.md](../../spec/edits.md) §2.2, §2.3 and §9.3 are the
consumers. Kiln owns the design of everything inside Kiln.

Line references in §2 are to Kiln `main` at `913a490bb`, where the
delivery is. Those of §5 are to `83566cc47`, where Slip's pin goes with
`docs/wp/WP-044.md`. Slip's pin is before both: nothing of §2 can be
called until the pin move after WP-044 (the package that moves the pin
to Kiln `main` at or after `913a490bb`).

**What changed since the first draft.** Stage 1 asked Kiln for four
functions (the extent of a token, a range as file characters, the origin
of a token, offsets in a file's bytes), and stage 2 for two more (the
expansion entries, several preprocessor observers). The second draft
withdrew stage 1 and had Slip compute it. That was wrong twice over:
Kiln has delivered it, and Slip depends on Kiln's implementations and
does not write them a second time. `slip_kiln` calls Kiln's functions
(`docs/wp/WP-084.md`, `docs/wp/WP-068.md`). What this document is now:

| Part | Kind | State |
|---|---|---|
| §2 What Kiln delivered, under which names, and where its rule differs from Slip's | A reading | **Delivered**: U1, U2, U5, U6. **Not delivered, and Slip's own**: U3 (the classification, §3), U4 (offsets in the file's bytes) |
| §2.2 Two follow-ups Slip may send | **Implementation against a settled specification**, small | Not asked. Slip derives both from Kiln's public functions |
| §3 The classification of a token's origin, Slip's tiers | **Design, small, with prior art for the pieces** (`isMacroArgExpansion`, `isWrittenInScratchSpace`, `getImmediateMacroCallerLoc`) | **Not asked.** Offered for review; becomes a request only if Slip's implementation finds a row the public functions cannot decide |
| §4 Stage 2 (U5, U6) | — | **Delivered** with stage 1 |
| §5 The written-reference table and the declaration extents against a rename | A reading, not a request. Its follow-ups are KR1's and are sent or scheduled there | — |

Nothing in this document is design without prior art.

## 1. Need

Slip edits a name where it is written. Kiln gives it the name as a
location range (a written reference's `range`, a declaration extent's
`name`). Slip must turn that into `(file, first byte, end byte)`, know
whether those bytes are text of the file that the parser read directly,
text of a macro argument, or something that cannot be edited there, and
it must be exact: a range one byte short corrupts a file that thousands
of translation units include.

## 2. What Kiln delivered

Kiln WP-2150, merged at `913a490bb`. The normative text is Kiln's:
`spec/source.md` ("Expansion entries", "Editable file ranges"),
`spec/lex.md` ("End of a token"), `spec/pp.md` ("Several observers").

| Asked as | Kiln's name | Difference from what the draft asked |
|---|---|---|
| U1, the exact extent of the token at a location | `kiln_lex::token_end(sm, &Interner, loc, LexOptions) -> Option<Loc>` (`kiln_lex/src/token_end.rs:32`) | It gives the end and takes the start as given: `None` if `loc` is not the first character of a token (white space or a comment is there), at the end of the buffer, or off a character boundary. For a macro location the token is measured where it is spelled and the end is in the location's own entry. It is not clang's `getLocForEndOfToken`; that is `file_loc_at_macro_end(token_end(..))`. It needs the unit's interner and lexer options, which the front-end entry gives (`FrontEnd.interner`, `kiln_driver/src/front_end.rs:103`; `FrontEnd.pp`, `:113`, with `Preprocessor::lex_options`, `kiln_pp/src/preprocessor.rs:614`) |
| U2, a location range as a range of file characters, or none | `SourceManager::editable_range(Span) -> Option<Span>` (`kiln_source/src/editable.rs:151`) and `editable_file_range(Span) -> Option<(FileId, u32, u32)>` (`:164`); the two rules it is made of are public too: `file_loc_at_macro_start` (`:68`), `file_loc_at_macro_end` (`:83`) | clang's `makeFileCharRange` and `canInsert`, and in addition the result must be one buffer range, in order. It does not say **which rule answered** (Slip's tier), it prefers the **whole invocation** to the argument when a range is both (§2.1), an empty range is an insertion point, and there is no system-header check (the client's policy: Slip's ownership) |
| U3, the origin of a token | Not a function. The facts are public: `expansion_info` (`kiln_source/src/manager.rs:1532`), `immediate_spelling_loc` (`:1588`), `is_written_in_scratch_space` (`:1170`), `file_origin` (`:1236`), and now the entry list with parents (U5) | Slip's classification (§3), in `slip_kiln::ranges` and `slip_kiln::sites` |
| U4, offsets in the file's bytes | Not delivered: `editable_file_range` and `offset_of` (`manager.rs:1296`) give offsets in `text(file)`, the text after loading | Slip's check: the offsets are the file's when the load report is clean (`load_report`, `:1225`); a file that was recoded on load is refused (`file_recoded`, `spec/edits.md` §2.3). A byte-order mark is part of Kiln's text and shifts nothing (`original_bytes`, `:1253`) |
| U5, the expansion entries | `SourceManager::expansion_count()` (`manager.rs:1553`), `expansion_entries(from)` (`:1562`) giving `ExpansionEntry { index, info, parent }` (`:178`), `ExpansionInfo::spelling_range` (`:171`) | More than asked: creation order is promised, the table is append-only, and each entry names its parent |
| U6, several preprocessor observers | `Preprocessor::add_callbacks` (`kiln_pp/src/preprocessor.rs:565`); `set_callbacks` (`:556`) replaces all | As asked |

Kiln's own fix-its now stand on the same functions
(`kiln_sema/src/macro_fixit.rs:11`, `:34`), so a fix-it and a tool cannot
place an edit differently. That is the property the draft wanted.

### 2.1 Where Kiln's rule and Slip's tier rule differ

Kiln's rule answers one question about a range of tokens: which
characters of one file, when edited, edit exactly what the range stands
for. `spec/api.md` §5.4 defines tiers 0 and 1 by the same question, and
`spec/edits.md` §2.2 classifies a *name token* by where its bytes are
written. Read against Kiln's `spec/source.md`:

1. **The tier is not in the answer.** Slip derives it from Kiln's own
   two rules: the answer is tier 0 when it came from Kiln's rules 1 to 3
   on the range as given (each end that is a macro location passes
   `file_loc_at_macro_start` or `file_loc_at_macro_end`), and tier 1
   when it came through rule 4 (both ends in macro-argument entries of
   one use of one parameter). With that reading every answer of Kiln's
   function is meant to satisfy the row of `spec/api.md` §5.4 for its
   tier, and the property test of `docs/wp/WP-084.md` (test 4) holds it
   to that. No difference in which ranges exist is known.
2. **A whole expansion that is also one use of one argument.** `ID(foo)`
   with `#define ID(x) x`: Kiln applies rule 3 before rule 4 and gives
   the whole invocation, as clang does. Both rows of `spec/api.md` §5.4
   hold there (the invocation at tier 0, the argument at tier 1) and
   the table does not say which range is meant. For an expression's
   extent Kiln's answer is the one Slip takes. For a **name** it cannot
   be: `spec/api.md` §6.6 rule 1 makes a name's range "the identifier
   token", and a rename writes an identifier over it. `spec/edits.md`
   §2.2 puts the token in tier 1 with the argument's bytes.
3. **A whole expansion made of other bytes.** `NAME` with `#define NAME
   foo`, `S(abc)` with `#define S(x) #x`, and `A(B)` with `#define B
   foo`, where rule 4 leads to `B`: Kiln gives the invocation (`NAME`,
   `S(abc)`, `B`). Right for an extent (`spec/api.md` §5.4, tier 0 or
   1). For the name token `foo` those bytes are not the name:
   `spec/edits.md` §2.2 says tier 2 (`macro_body`) or 3 (`pasted`), and
   no range.
4. **What "file" means.** For Kiln a file location is any location that
   is not a macro location: the buffer may be the predefines, the
   command line's definitions or a virtual buffer. Slip's `not_a_file`
   (tier X) is its own check, with `file_origin`.
5. **Offsets and ownership** are rows U4 and U2 above: the text's
   offsets, and no policy.

So Slip has two functions over Kiln's one (`docs/wp/WP-084.md`):

- **the range of a span** (an extent): Kiln's `editable_file_range`,
  unchanged, with the tier of item 1;
- **the range of a name token**: the classification of §3 decides the
  tier from where the token is written (while the location is in a
  macro-argument entry it is replaced by its immediate spelling, with
  Kiln's `expansion_info` and `immediate_spelling_loc`); for tier 0
  or 1 the end is Kiln's `token_end` at the location reached, and the
  range is Kiln's `editable_file_range` of that span, which is then
  rule 1 with the check of rule 5. Slip never moves an end by
  arithmetic of its own.

The two agree wherever Kiln's function answers through rule 4 alone,
and differ exactly in items 2 and 3; a test of `docs/wp/WP-084.md`
holds that over every name token of the snippets. The alternative for
item 2 was to take Kiln's order and refuse such a name. It is safe and
loses a site that is editable by Slip's own definition; and a macro
that is the identity in this configuration may not be in one no
observer has, so replacing the invocation is the larger claim.

### 2.2 Two follow-ups Slip may send (not asked)

- **The rule that answered.** `editable_file_range` returning also
  whether the argument step was taken would replace the derivation of
  §2.1 item 1 (two more walks per range).
- **The argument first, on request.** A variant that, for a range inside
  one use of one argument, never widens to the invocation: §2.1 item 2
  for ranges of more than one token (a qualified name, `a::b`, written
  whole in an identity macro's argument). Slip's name rule handles one
  token and has no use for more in M2.

Neither blocks anything. Both are implementation against Kiln's settled
rule.

## 3. The origin of a token: the classification Slip implements

Offered for Kiln's review because Kiln knows its expansion entries
better than Slip does. For the token whose first character is at `at`:

| `at` | Slip's tier (`spec/edits.md` §2.2) |
|---|---|
| A file location in a file read from disk or through an overlay | 0, file |
| A file location in any other buffer (the command line, the predefines, a virtual buffer) | X, `not_a_file` |
| A macro location whose every enclosing entry, from the token outward, is a macro-argument entry until a file location is reached, in a file | 1, macro argument |
| A macro location with a macro entry on that walk whose spelling is in scratch space | 3, `pasted` |
| A macro location with a macro entry on that walk whose spelling is in a file | 2, `macro_body` |
| Any macro location whose spelling ends in a buffer that is not a file | X, `not_a_file` |

Edge rows, each a test of `crates/slip_kiln/tests/sites.rs`:

1. **An argument that names a macro.** `F(G)` where `G` is object-like
   and expands to the token: the token's spelling is in `G`'s body:
   `macro_body`.
2. **An argument passed through several macros** (`A(x)` defined as
   `B(x)`): every entry is a macro-argument entry: tier 1, the
   invocation being the outermost name (`A`).
3. **An argument inside another argument** (`A(B(x))`): tier 1.
4. **`__VA_ARGS__` and `__VA_OPT__`**: tokens of the variadic arguments
   are tier 1; tokens written inside `__VA_OPT__( … )` in the
   definition are `macro_body`.
5. **An invocation that begins in one macro's body and ends in the
   file** (a function-like macro name from a body, its parenthesis from
   the file): the argument tokens are written in the file: tier 1.
6. **A token both substituted and stringised** (`#x` and `x` in one
   body): two tokens. The substituted one is tier 1; the string is
   `pasted`. The classification is per token and does not relate them
   (that is the preprocessing record's, KR4).
7. **A name of two tokens from different expansions** (`a::b` with
   `a::` from a body and `b` from an argument): per token; a range over
   both is `crossing`.

A wrong row makes Slip edit a macro body it believes is an argument,
which verification then has to catch. If a row cannot be decided from
the public functions, that row becomes a request.

## 4. Stage 2: delivered

Both were "not asked before M4" in the earlier drafts, and Kiln
delivered them with stage 1 (§2, rows U5 and U6).

- **U5, expansion entries.** Not used by M2: `sites` finds the outermost
  invocation of one token with `expansion_loc` (the `MacroSite` of a
  claim), and M2's reports group refusals by macro from that. M4 reads
  the whole list.
- **U6, several preprocessor observers.** Not used by M2. The
  preprocessing record (KR4, Kiln WP-2110 stage 1 on the same commit)
  is expected to serve what Slip wanted an observer for.

## 5. Kiln's tables and a rename of functions and variables

What `spec/edits.md` §9.3 must find, against what Kiln records at
`main`: the written-reference table (Kiln `spec/ast.md` §18,
`kiln_ast/src/written_refs.rs:44` for the roles, `:144` for the entry;
stages 1 and 2, Kiln WP-1930 and WP-1931) and the declaration-extent
table (Kiln `spec/ast.md` §19, WP-2090; the row is
`kiln_ast/src/decl_extents.rs:57` to `:84`). Slip reads both through
`slip_kiln::written` (`docs/wp/WP-067.md`).

| Reference | Source in Kiln | State |
|---|---|---|
| The name at each declaration and at the definition | The declaration extent's `name` (`decl_extents.rs:68`), one row per redeclaration | **Enough** |
| An id-expression, an enumerator, `&f`, a member access | Written references, roles `Expr` and `Member`, with the function overload resolution selected | **Enough** |
| A qualified name's last component, and its qualifier chain | Role `Nns` and `qualifier` | **Enough** (a rename edits no qualifier) |
| A type, template, base or member-initialiser name | Stage 2 | Not needed for functions and variables |
| A using-declaration's name | The declaration extent's `name` of the using-declaration ("a using-declaration's member name", Kiln `spec/ast.md` §19.2). The written-reference role `UsingDecl` (stage 3) is still open and is not needed for this | **Enough** |
| An explicit specialisation's and an explicit instantiation's name | Written declarations with a row of their own, `name` being the template-id (Kiln `spec/ast.md` §19.1 "No row", last paragraph) | **Enough** for function and variable templates |
| A friend declaration that names the function | A redeclaration with its own row | **Enough** for functions |
| A designator, an attribute argument that names an entity | Stage 3 `Designator`; attributes are in neither table | **Missing.** Designators concern fields (not renamed in M2). An attribute that holds a subject (`cleanup`) makes Slip refuse (`unreachable`) |
| A dependent callee of a call in a template | Stage 1 records the name with `REF_DEPENDENT` (`written_refs.rs:138`) and no entity; the declarations found at the definition are in the unresolved-lookup expression, joined to the entry by location | **Waits for KR1 amendment 1 §2** (sent), which asks for that join to be promised |
| Any other dependent name, per instantiation | Stage 4 (Kiln WP-1933): open | **Missing.** Slip claims `unresolved` and reports residue; a rename that needed it does not verify |
| Names in a PCH or a module | Stage 5 (Kiln WP-1934): open | Not needed for the corpora Slip has |
| Overriding functions | Not a written reference: the `overrides` list of a method | **Enough** |

**Can a rename stand on Kiln's `main` now?** For a function or variable
at namespace scope or in a class that is not a template, used outside
dependent contexts: yes. That covers both accepted targets of Demo 2
(`docs/demos/demo-2.md`). It cannot yet serve a member of a class
template referred to through a dependent object (stage 4), and the link
from a dependent call to its lookup set is by a location equality that
is not yet promised. One condition is outside the tables: both are
opt-in (`kiln_parse/src/lib.rs:141`, `:144`), and the only switches are
driver flags, of which `-fkiln-dump-written-refs`
(`kiln_driver/src/args.rs:2049`) also prints the table. Slip sets the
flags on the front end's command line until the entry has an option
(`docs/wp/WP-067.md`).

Where each follow-up of the first draft stands:

| Id | Ask | Now |
|---|---|---|
| K-a | From a dependent entry to its expression | [KR1 amendment 1](KR1-amendment-1.md) §2, sent 2026-11-22. `docs/wp/WP-073.md` part A waits for it |
| K-b | The entry records the written references on request, printing nothing | KR1 amendment 1 §1, sent; Kiln WP-2320. Until it lands Slip pays for the dump |
| K-c | The name of a using-declaration | **Answered by Kiln WP-2090**: the declaration extent's `name` |
| K-d | The target of an explicit instantiation or specialisation | **Answered by Kiln WP-2090** for function and variable templates. Stage 3's `SpecTarget` is still wanted for members of class templates (`docs/wp/WP-073.md` part B) |
| K-e | Stage 4 (Kiln WP-1933) | Scheduled by Kiln; KR1 amendment 1 §4 gives Slip's order of value |
