# KB1 — findings for Kiln, batch 1 (2026-11-22)

From Slip's packages up to WP-089, at Kiln pin `913a490bb`. None
of these blocks Slip today: each has a workaround or a pinned test on
Slip's side, named in its row. They are collected so that Kiln can
schedule them as it sees fit; Slip asks only for an answer per item
(a work package number, "as designed", or "not now").

**Kind** says what each item is, as the operator asked every request to
Kiln to state: **I** = implementation against behaviour that is already
specified or obvious (a bug, a missing row, a visibility change);
**Q** = a question, where the answer may be "as designed";
**D** = needs a design decision without prior art in Kiln (hand to
whoever does Kiln's design).

Paths are Kiln's unless they start with `slip:`; a `slip:` path is in
Slip's repository at `main`.

## 1. The declaration-extent table and the written-reference table

Found by `slip_kiln::written` (`slip:spec/landed/edits/WP-067.md` has a
reproducer and a `kiln_hole_*` test for each; the tests state what is
seen today and fail when Kiln changes it).

| # | Kind | Finding | Where |
|---|---|---|---|
| 1.1 | I | No extent row for an explicit instantiation of a function template, a variable template, a member function (`template void S<long>::m();`), or the form without template arguments (`template void t(short);`). Class templates get one. | `kiln_parse/src/templ.rs:157-167` |
| 1.2 | I | An `extern template` declaration and the later explicit instantiation definition of one specialisation keep one row: the second overwrites the first. | the same |
| 1.3 | I | A later construct overwrites an earlier declaration's row: `struct S;` followed by `friend struct S;`, a second `class K;`, a repeated `template <> struct Q<int>;`. The earlier declaration then has no row, so it looks unwritten. | `kiln_parse/src/extents.rs:68-79`, `kiln_ast/src/decl_extents.rs:164-171` |
| 1.4 | I | An init-capture (`[x = f()]`) has no extent row. | — |
| 1.5 | I | The member designator of `offsetof(S, m)` is not a written reference. (KQ1 amendment 1 gives the digest its cold row; this is the reference to `m` as written.) | — |
| 1.6 | D | A dependent written-reference entry carries no found set: for an unqualified dependent call `f(t)` in a template, the declarations found at the definition are linked nowhere, whether one overload is visible or several, and in the current instantiation too (`this->a(t)`, `C::a(t)`). Slip's rename of dependent names (`slip:docs/wp/WP-073.md`, `slip:spec/edits.md` §9.4 row 1) needs that set: which declarations, at which point, how it is stored, is a design question for KR1. | `kiln_ast/src/written_refs.rs:270-277` |

## 2. What a lambda-expression declares

| # | Kind | Finding | Where |
|---|---|---|---|
| 2.1 | D | Everything a lambda-expression declares (body locals, init-captures, local classes and their members, structured bindings, the contents of lambdas in default member initialisers and default arguments) has a context chain through the implicit closure class and call operator. A client that asks "which written declarations does this function hold" must special-case the closure. Slip reads the extent table instead (`slip:docs/wp/WP-085.md`); the question is whether Kiln wants to state the rule in `spec/ast.md` (what the context of such a declaration is, and that the extent table is the way to enumerate what is written). | `slip:spec/landed/api/WP-083.md` decision 19; test `review2_lambda_declares` |
| 2.2 | I | In `void f(){ struct L { int fl = ({ int tf = 2; tf; }); }; }` the statement-expression's local `tf` gets the class `L` as its context, and the declaration is rejected. | `slip:spec/landed/api/WP-083.md` "For Kiln" 4 |
| 2.3 | I | A template parameter's owner (its template) is not a field: Slip scans every template parameter list once to find it. A field or a side table would do. | `slip:spec/landed/api/WP-083.md` "For Kiln" 2 |

## 3. Parameter attributes (`slip:spec/landed/digest-kinds/WP-100.md`, with reproducers)

| # | Kind | Finding | Where |
|---|---|---|---|
| 3.1 | I | Parameter attributes are not instantiated (only the Swift ABI ones are). | `kiln_sema/src/templ/subst/decls.rs:513-626`, `:532` |
| 3.2 | I | The prolog reads `nonnull`, `align_value` and a parameter's `annotate` on the definition's own parameter only, not on the other declarations. | `kiln_codegen` `class_b.rs:715`, `:718`, `annotate.rs:327` |
| 3.3 | Q | Differing `noescape` across declarations is accepted; and a nested declarator's parameter attributes are kept nowhere. Slip's test `differ/_/param-attr-nested-declarator` waits on the second (`# needs: kiln`). | — |
| 3.4 | Q | Code generation reads a block-scope redeclaration's parameter attributes. Intended? | `kiln_codegen` `class_b.rs:591`, `ubsan.rs:2150` |

## 4. Printing and small API

| # | Kind | Finding | Where |
|---|---|---|---|
| 4.1 | I | `write_arg_list` prints a null pointer template argument as `nullptr` without its type, so `S<(int*)nullptr>` and `S<(char*)nullptr>` print alike. Slip wraps the printer (`TypedNull`). | `kiln_ast` `types` printing |
| 4.2 | Q | `with_module_ownership` has no caller. | `kiln_ast/src/decl.rs:512` |
| 4.3 | Q | Members of an anonymous union named `_` are marked name-independent. Intended? | `kiln_sema/src/decl.rs:7000` |
| 4.4 | I | `oper_lookup::SetSpan` is not public; Slip's digest of an operator's found set needs to name it. | `kiln_sema` `oper_lookup` |

## 5. Cost

| # | Kind | Finding | Measured |
|---|---|---|---|
| 5.1 | I | Parse time quadratic in the length of: a deep member-call chain, an else-if chain, a run of case labels, a run of plain labels. | 100 000 labels 69 s; 100 000 case labels 19 s (`slip:spec/landed/digest-kinds/WP-034.md`) |
| 5.2 | I | Each level of nested designated initialiser lists in a template gets a fresh written copy of the levels below (`syntactic_list`), though instantiation reads only the outermost (`templ/subst/exprs.rs:2935`). The AST, and anything that walks it, grows quadratically in the depth. | `kiln_sema/src/templ/actions.rs:2162`; bytes for 1, 2, 3, 4, 8, 12 levels in `slip:spec/landed/digest-kinds/WP-035.md` "Nesting" |
| 5.3 | I | 2 000 nested classes parse in 16.9 s. | `slip:spec/landed/api/WP-083.md` |

## 6. A question on reproducible time

| # | Kind | Finding |
|---|---|---|
| 6.1 | Q | Slip passes `-fkiln-reproducible-time` on every parse so that `__DATE__`, `__TIME__` and `__TIMESTAMP__` do not move a digest. What does the flag mean for a precompiled header or a module image built without it (or with another `SOURCE_DATE_EPOCH`): is the recorded text's time the image's or the consumer's? Slip has neither in M1; the answer decides what Slip must refuse when it does. |

## 7. From packages still in review on Slip's side

Their text of record is on a Slip branch until the package merges; the
findings are about Kiln either way.

| # | Kind | Finding | Where |
|---|---|---|---|
| 7.1 | I | The type of an uncaught exception is reported unqualified: `RunError::Uncaught.type_name` comes from `Oven::tname`, which writes the type as spelled, so `slip::v0::error` is `error` and a client cannot tell its own exception type from a transform's class of the same name. Wanted: the qualified name, or the class's `DeclId`. | `kiln_oven/src/session.rs:356`, `notes.rs:237` |
| 7.2 | I | `RunError::Host` joins two causes: the handler's error and Kiln's own check of the primitive's arguments. A distinct kind for the argument check. Small. | `kiln_oven/src/session.rs:352-354`, `vm/host.rs:32`, `:137` |
| 7.3 | D | An attribute Kiln copies onto an instantiation at definition (`WEAK`; the section kinds) is not marked as a copy, so a client cannot tell it from one written on the declaration. A mark on the copy, as `inherited` marks an inherited attribute. | `kiln_sema` `templ/class.rs:1603-1615`, `section.rs:204` |
| 7.4 | I | `FnSpec` `0-implicit-inline` is set at declaration and at definition with one bit; two bits would let a client keep the declaration's. | `templ/subst/decls.rs:1044`, `class/members.rs:906`, `:921` |
| 7.5 | Q | A unit that parses with zero errors can hold error nodes: `llvm/lib/Support/MathExtras.cpp` (LLVM 19.1.7) has two `RecoveryExpr`-class nodes in an AST whose error count is 0. Slip marks the units indeterminate, so it is safe; is an error node without a counted error intended (for example in a discarded branch)? | Slip's map task counter `digest.indeterminate.error` |

## Not in this batch

The resolved properties' questions (one on a static data member's
in-class initialiser against its out-of-line definition) come with batch
2 when that package merges. One earlier candidate is withdrawn: Slip
does not ask that declarations be numbered in parse order (they are not,
and Slip sorts by unit order).
