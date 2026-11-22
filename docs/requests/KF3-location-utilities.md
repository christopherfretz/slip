# KF3 — location utilities: from a Kiln location to bytes of a file

A request from Slip to Kiln. Status: **draft, not sent; stage 1
withdrawn.**
Context: [05-kiln-interface.md](../design/05-kiln-interface.md) §3 (row
KF3); [spec/edits.md](../../spec/edits.md) §2.2, §2.3 and §9.3 are the
consumers. Kiln owns the design of everything inside Kiln.

Line references are to Kiln `main` at `83566cc47`, which is where Slip's
pin goes (`docs/wp/WP-044.md`).

**What changed since the first draft.** Stage 1 asked Kiln for four
functions (the extent of a token, a range as file characters, the origin
of a token, offsets in a file's bytes). Read against Kiln's `main`
([kiln-landed-2026-11.md](../design/notes/kiln-landed-2026-11.md) §5),
every one of them can be computed outside Kiln from what the source
manager and the lexer already make public, and Slip now does so
(`docs/wp/WP-084.md`, `docs/wp/WP-068.md`). The first two are also
asked of Kiln already, in [KE1](KE1-expression-extents.md) §2.2 (sent
2026-11-22): this document does not ask a second time. A request for
something Kiln already gives, or has already been asked for, is not a
request: **stage 1 is withdrawn**, and with it the prototype branch
`slip/kf3`, which was never cut. What is left is small and is not
needed by M2:

| Part | Kind | State |
|---|---|---|
| §2 What Slip computes itself, and from what | Nothing asked. Recorded so that Kiln knows which public functions a tool now stands on | — |
| §3 One rule Kiln may want to review (the classification of a token's origin) | **Design, small, with prior art for the pieces** (`isMacroArgExpansion`, `isWrittenInScratchSpace`, `getImmediateMacroCallerLoc`); what has no prior art is the classification itself, Slip's tiers | **Not asked.** Offered for review; becomes a request only if Slip's implementation finds a row the public functions cannot decide |
| §4 Stage 2: enumerating expansion entries (U5), several preprocessor observers (U6) | **Implementation against a settled specification**; both probably superseded by the preprocessing record (KR4) | Not asked before M4 |
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

## 2. What Slip computes itself

| Was asked as | Slip does it in | From Kiln's public functions |
|---|---|---|
| U1, the exact extent of the token at a location | `slip_kiln::ranges` (`docs/wp/WP-084.md`): one token lexed again at its spelling location; no answer for a token that cannot be lexed again exactly (one made by `##`). For a written reference and a declaration name the end is already exact in the table (`kiln_ast/src/written_refs.rs:144`; Kiln `spec/ast.md` §19.2) | `kiln_lex`'s lexer in raw mode; `spelling_loc` (`kiln_source/src/manager.rs:1490`) |
| U2, a location range as a range of file characters, or none | `slip_kiln::ranges::file_range`, after clang's `makeFileCharRange`, with the property test that defines it (WP-084, test 4) | `expansion_info` (`manager.rs:1456`), `immediate_macro_caller_loc` (`:1527`), `entry_bounds` (`:1421`), `offset_of` (`:1220`) |
| U3, the origin of a token | `slip_kiln::sites` (`docs/wp/WP-068.md`): the tier, and for a token that has no range, why | the same, with `is_written_in_scratch_space` (`:1094`) and `file_origin` (`:1160`) |
| U4, offsets in the file's bytes | `slip_kiln::sites`: the buffer's offsets are the file's bytes' offsets when the load report is clean (`load_report`, `manager.rs:1149`); a file that was recoded on load is refused (`file_recoded`, `spec/edits.md` §2.3). A byte-order mark is part of Kiln's text and shifts nothing | `load_report`, `original_bytes` (`:1177`) |

Kiln's own fix-it code has a private version of the second
(`kiln_sema/src/macro_fixit.rs`), and `Sema::token_end_of` of the first,
neither reachable after the unit is built. Two implementations of one
rule can drift, which is why KE1 §2.2 asks Kiln to make its own public.
When that lands, `slip_kiln::ranges` calls Kiln's and keeps its property
test; nothing of M2 waits for it.

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

## 4. Stage 2 (not asked before M4)

- **U5, expansion entries.** `SourceManager::expansions()`, the entries
  in creation order. M2's reports group refusals by macro from the
  entries they reach; M4 needs the full list and will ask for it with
  KR4 if the preprocessing record does not already carry it.
- **U6, several preprocessor observers.** `Preprocessor::set_callbacks`
  (`kiln_pp/src/preprocessor.rs:531`) takes one object. A tool that
  wants its own beside the driver's needs a fan-out. Slip does not need
  it for M2 and expects KR4 to replace the need.

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
