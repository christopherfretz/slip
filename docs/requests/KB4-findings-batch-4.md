# KB4 — findings for Kiln, batch 4 (2026-11-23)

From Slip's packages WP-040, WP-041, WP-048, WP-052, WP-068, WP-085,
WP-089, WP-104 and WP-113, their independent reviews, and section 4 of
KF5, at Kiln pin `913a490bb`. As in [KB1](KB1-findings-batch-1.md),
[KB2](KB2-findings-batch-2.md) and [KB3](KB3-findings-batch-3.md): none
blocks Slip today, each has a workaround or a pinned test on Slip's
side, and Slip asks only for an answer per item (a work package number,
"as designed", or "not now"). **One item is pressing: 4.2.** It is the
only one for which Slip has no workaround, and it leaves a known wrong
`verified` in Slip's verify task until the log records the lookup.
Items 1.9, 4.3, 4.4, 4.5, 5.1, 5.2 and 5.3 are requests (**R**): Slip's
operator sees them before Slip presses for any.

**Kind**: **I** = implementation against behaviour that is already
specified or obvious; **Q** = a question, where the answer may be "as
designed"; **D** = needs a design decision without prior art in Kiln
(hand to whoever does Kiln's design); **R** = a request for something
Kiln does not have (a feature, not a defect: Slip's operator sees these
before Slip presses for any).

Paths are Kiln's unless they start with `slip:`. A `slip:` path of
WP-041, WP-052, WP-068, WP-085 or WP-113 is on Slip's main once that
package merges (each is in review or its fix round); the others are on
main.

## 1. Semantic analysis

| # | Kind | Finding | Where |
|---|---|---|---|
| 1.1 | I | **`Ast::linkage` gives external linkage to a block-scope `extern` function of a function in an unnamed namespace.** A block-scope function declaration (or `extern` variable) that is its entity's first declaration is `External`. By [basic.link]/6 it is a member of the innermost enclosing namespace, here the unnamed one, so it has internal linkage ([basic.link]/4); clang and Kiln's own mangler agree (`t _ZN12_GLOBAL__N_11fEv`). Source: `namespace { int g() { extern int f(); return f(); } int f() { return 1; } } int u() { return g(); }`: `Ast::linkage(f)` is `External` for both declarations, `in_anonymous_namespace(f)` is true. Reflection's `has_external_linkage(^^f)` reads it. Slip writes both values, so is not affected. | `kiln_ast/src/linkage.rs:349-362`; `slip:spec/landed/digest/WP-113.md` "For Kiln" 1 |
| 1.2 | I | **`Ast::linkage` and the mangler disagree on `extern "C"` in an unnamed namespace.** `namespace_scope_linkage` returns `Internal` for anything in an unnamed namespace; `has_internal_linkage` keeps a function or variable first declared in an `extern "C"` context external, as clang's `getLVForNamespaceScopeDecl` does. Source: `namespace { extern "C" int cf() { return 1; } }`. One of the two is wrong, for reflection or for the symbol; which one is Kiln's to decide. Slip writes both. | `kiln_ast/src/linkage.rs:377-385`; `kiln_abi/src/mangle.rs:750-759`; `slip:spec/landed/digest/WP-113.md` "For Kiln" 2 |
| 1.3 | I | **A fold-expression records no operator set.** A dependent binary or unary operator keeps the non-member operators that unqualified lookup finds at the template's definition (`note_dependent_oper`, `templates.oper_lookups`, [temp.dep.candidate]); a `CXXFoldExpr` does not: `act_fold` builds the node and records nothing, and instantiation combines the elements with `act_on_binary` with no recorded set. Two templates that differ only in which `operator+` was visible at the definition are the same tree. Source (the `long` overload declared on side B only): `namespace N { struct X {}; } int operator+(N::X, int); long operator+(N::X, long); template <class... T> auto g(T... t) { return (t + ... + 0L); }`; also with `-> decltype((t + ... + 0L))` and the unary `(... + t)`. Expected: the fold keeps its definition's set as a dependent operator does (clang keeps it as the fold's callee). Slip makes such a unit indeterminate (`spelling/fold-operator`). | `kiln_sema/src/templ/actions.rs:1930-1954`; `templ/pack.rs:649`, `:722-723`; `overload/oper.rs:288-312`; `slip:spec/digest-kinds.md` §4.4 "Spellings" rule 9 (WP-041) |
| 1.4 | I | **Code generation's namespace visibility starts at the semantic context, which for an explicit specialisation written in another opening of its namespace is the first opening** (found by reading; Kiln's code generation was not run). `ns_lv` starts its walk at `parent_ctx(d)`, `d.ctx()` for a function. Source: `namespace d { template <class T> int g() { return 0; } } namespace __attribute__((visibility("hidden"))) d { template <> int g<int>() { return 1; } } int u() { return d::g<int>(); }`: the definition's `ctx` is the first, plain opening and its `lexical_ctx` the hidden one, so Kiln finds no attribute and makes the symbol `DEFAULT`, where clang makes it `HIDDEN _ZN1d1gIiEEiv`. Either the specialisation's `ctx` is the opening it is written in (clang's semantic context is that `NamespaceDecl`), or `ns_lv` starts at the lexical context when that is a namespace. Slip starts at the lexical context. | `kiln_codegen/src/visibility.rs:239`, `:453-458`; `slip:spec/landed/digest/WP-113.md` "For Kiln" 3 |
| 1.5 | I | **The owner of a lambda in a lambda parameter's default argument is the enclosing function.** `spec/ast.md` §5.9 makes the owner the innermost declaration whose text holds the lambda-expression: the parameter, for one in its default argument (or the call operator, for one in its signature). Source: `int f() { return [](int a = [] { return 1; }()) { return a; }(); }`: Kiln's owner of the inner closure is `f`. At creation the site owner is the open function `f` (no delayed default argument is set for a lambda's parameter), and the claim by `a` does not narrow it, because a function whose body holds the closure is never narrowed. Slip's numbering of a function enters a call operator's parameters' default arguments (`local::call_op_prototype`), so the closure is numbered; tests `differ/_/lambda-in-lambda-default-argument`, `equal/_/lambda-in-lambda-default-argument`. | `kiln_sema/src/lambda/owner.rs:16-24`, `:81-109`; `slip:spec/landed/digest/WP-040.md` "A lambda in a lambda's default argument" |
| 1.6 | I | **A lambda in a non-type default template argument gets mangling number 0 and no context.** `ManglingState::template_default` is documented as set while "a default template argument is being parsed or substituted", but only substitution sets it (`act_on_start_template_param_default` has two callers, both in sema); the parser's non-type default calls `act_on_template_param_default` with the flag off, so at namespace or class scope `lambda_numbering_context` finds no context and the closure keeps `number` 0. Shape (Slip's report gives no smaller source): `template <int N = [] { return 1; }()> struct S {};`. Is a type parameter's default (`parse_type_param_default_arg`) the same? Slip writes the number and the context as facts of the closure, not as its key (`slip:spec/digest-kinds.md` §10.4), so it is not affected beyond those facts. | `kiln_sema/src/lambda/mod.rs:99-103`, `:766-777`, `:905-910`; `templ/args.rs:2246`, `templ/member.rs:653`; `kiln_parse/src/templ.rs:1225-1238`; `slip:docs/wp/WP-040.md` "Supervisor's handback (merge)" |
| 1.7 | Q | **Two spellings that differ only in found sets are one canonical type, and substitution reads the first spelling's expression.** A profile names an `UnresolvedLookupExpr` by its name and qualifier, not its found set, and reads no `oper_lookups`; a canonical, profile-only expression type is substituted from its first expression ("whose first expression stands for it as when substituting"). So `decltype(f(t))` written where lookup found nothing and again after `f(long)` is declared is one type, and an instantiation from the canonical type gets the first spelling's set. Intended, given [temp.over.link]/5 (equivalent only if the two definitions would satisfy the one-definition rule)? Slip writes each unit's own spelling's sets and has not reproduced a substitution from a canonical operand. | `kiln_ast/src/types/profile_expr.rs:511-523`; `kiln_sema/src/templ/subst/types.rs:58-75`; `slip:spec/landed/digest-kinds/WP-041.md` "Hazards" |
| 1.8 | Q | **The floating-point rounding state is not in a profile.** Two spellings of one dependent expression under different `#pragma STDC FENV_ROUND` states are one type: the profile does not read `fp_round`. Intended (clang's profile does not read its floating-point features either)? Slip writes each unit's rounding states itself (`slip:spec/digest-kinds.md` §4.4 "Spellings" rule 8). | `kiln_ast/src/types/profile_expr.rs` (no `fp_round`); `slip:spec/landed/digest-kinds/WP-041.md` "Hazards" |
| 1.9 | R | **A `PredefinedExpr` does not record its function.** `act_on_predefined_expr` finds the innermost enclosing function and keeps only the string it computes (or no literal in a dependent context). Slip finds the function again by definition extents and checks that the bytes are that function's name of that kind; a link on the node (or a side table) would replace both. | `kiln_sema/src/predefined.rs:19-42`; `slip:spec/landed/digest/WP-104.md` "What is written" |
| 1.10 | Q | Addition to KB3 3.2: **closures at namespace scope used as template arguments also print alike**, `store<legacy::(lambda)>` for two different closures (their keys differ; their names collide). Slip leaves them out of its name tests. | `kiln_ast` type printing; `slip:spec/landed/digest/WP-104.md` "Left indeterminate" |
| 1.11 | Q | Addition to KB1 4.2: **the `ModuleOwnership` field of `DeclMisc` is never set**, with or without modules (ownership is kept in `ModuleTable`); its one reader refuses a declaration whose field is not `Unowned`. Keep it, set it, or remove it? Slip lists its values as untested rows. | `kiln_ast/src/decl.rs:512`; `kiln_sema/src/bmi_image.rs:4157-4161`; `slip:spec/landed/harness/WP-048.md` "Rows still untested" |

## 2. The declaration-extent table (`slip:spec/landed/api/WP-085.md` "For Kiln")

Each item makes a written declaration "not written", so its entity is
nowhere: a transform does not see it declared, and a rename leaves it,
which verification then reports. Item 2.1 is pinned by a `kiln_hole_*`
test; items 2.2 to 2.7 come from comparing Slip's structural rule with
the table over every declaration of Slip's pair tests (129 944
declarations), each traced to the action that makes it.

| # | Kind | Finding | Where |
|---|---|---|---|
| 2.1 | I | Addition to KB1 1.3: **a qualified elaborated-type-specifier in a declaration overwrites the row of the class's definition.** `namespace n { struct S {}; } struct n::S *p;`: one `CXXRecord` row for `S`, whose name is line 2's `S` and whose `qual` is `n::`. Expected: the row stays the definition's; a specifier that declares nothing records nothing. Test `kiln_hole_k3_a_qualified_elaborated_specifier_overwrites_the_row`. | `kiln_parse/src/decl.rs:2869-2874`, `extents.rs:271` (location not confirmed: at `:2870` a reference records a row only when its `DeclId` is new, so the qualified form apparently makes one) |
| 2.2 | I | **A range-for's declaration has no row.** `void f(int (&a)[2]) { for (int x : a) {} }`: `x`. | `kiln_parse/src/stmt.rs:1401`, `:1444` |
| 2.3 | I | **An exception handler's variable has no row.** `void f() { try {} catch (int e) {} }`: `e`. | `kiln_parse/src/stmt.rs:1022` |
| 2.4 | I | **An expansion statement's variable has no row.** `template for (auto x : {1, 2}) {}` in a function: `x`. | `kiln_parse/src/stmt.rs:1618` |
| 2.5 | I | **A requires-expression's parameters have no row.** `template <class T> concept C = requires (T t) { t; };`: `t`. | `kiln_parse/src/templ.rs:2474` |
| 2.6 | I | **A contract's result name has no row.** `int f() post (r: r > 0);`: `r`. | `kiln_parse/src/contracts.rs:130` |
| 2.7 | I | **A label has no row.** `void f() { l: goto l; }`: `l`. | `kiln_parse/src/stmt.rs:568` |

## 3. Locations (`slip:spec/landed/edits/WP-068.md`)

| # | Kind | Finding | Where |
|---|---|---|---|
| 3.1 | I | **Scratch space is recognised by the presumed file name.** `is_written_in_scratch_space` compares the presumed name with `<scratch space>`, so a real file with `#line 1 "<scratch space>"` has its tokens taken for pasted ones, and nothing else tells a scratch buffer from a file. Expected: the buffer's own identity (a flag or `FileOrigin` of the scratch buffers), not a name `#line` can set. Slip refuses such tokens as `pasted`, so it errs to a refusal. | `kiln_source/src/manager.rs:1167-1172` |

## 4. The driver, modules and the lookup log (`slip:spec/landed/verify/WP-052.md`)

Slip's guard L1 reads Kiln's lookup log to prove that the edited
program's preprocessing looked at no file of the user's tree; what the
log does not record, L1 cannot see. Tests in
`slip:crates/slip_kiln/tests/lookups.rs`.

| # | Kind | Finding | Where |
|---|---|---|---|
| 4.1 | I | **An in-memory header imported as a header unit is looked up on the host.** `header_unit_file` computes `header_unit_key(sm.file_name(file))` before it looks in `module_files`, and `header_unit_key` calls `std::fs::canonicalize` on the name against the working directory. For a buffer from `add_virtual` the name is not a host path: the read has no meaning, and if a file of that name exists in the working directory, a header-unit module file given for it would be taken for the in-memory header. Reproducer: Slip's snippet front end with `main.cpp` = `import "h.h";` `int y = X;` and `h.h` = `#define X 1` in memory: no error, and `<cwd>/h.h` is looked up. Proposed: for a virtual buffer the key is `"` and the buffer's name as it is, with no file-system call (or `header_unit_file` returns `None` at once when no module file and no replay names a header unit). The rest of KF5 is a separate feature request. | `kiln_pp/src/modules.rs:1472-1477`, `:1636-1641`; `slip:docs/requests/KF5-module-sources-in-memory.md` §4 |
| 4.2 | I | **The lookup log does not record a lookup of an absolute name** (as `spec/pp.md` "Lookup log" says today). The result of a failed `__has_include("/abs/gone.h")` depends on the file system: if the file appears, the program changes, and neither the log nor its `check` sees it. That is a stale answer for the PCH snapshot the log was made for, and for Slip a **wrong `verified`**: source `#if __has_include("<tree>/new.h")`, the edit adds `new.h`; Slip parses the edited program from a copy of the tree, the absolute name still points at the unedited one, the lookup fails as before, and L1, which would refuse a post-image that looked into the unedited tree, never hears of it (the review's F7). Wanted: every lookup of an absolute name in the log, found or not (`#include`, `#include_next`, `__has_include`, `__has_include_next`, `#embed`, `__has_embed`, `-include`), with the name as written; `check` treats it like any other. Slip can do nothing on its side without scanning source bytes for paths, which it does not do. The test `absolute_name_not_recorded` pins today's answer so that it fails when Kiln records them; Slip's guard then covers the case with no new code. | `kiln_pp/src/resolver.rs:132`, `:161`; `slip:spec/verify.md` §5.1 "What remains outside" (on merge of WP-052) |
| 4.3 | R | **The `--embed-dir` list is not in the log, nor which of its directories an `#embed` tried.** `LookupLog::check` takes the list from its caller. Slip takes it from the driver (`Args::embed_dirs`) and counts every directory of it as tried. Wanted: the list in the log, and per lookup the directories tried up to the result. | `kiln_pp/src/resolver.rs:158-177`, `lookups.rs:341-344`; test `embed_and_has_embed` |
| 4.4 | R | **`-imacros` is not accepted**: the driver rejects the command, so a unit built with it is not parsed at all (clang and GCC accept it). Slip's post-image is then `not_parsed`, never verified. Test `dash_imacros_is_not_accepted`. | `kiln_driver` (no `-imacros` in its options) |
| 4.5 | R | **`rsp::expand` reads relative to the process's directory, reports nothing it read and has no read hook.** A response file that cannot be read stays a word, silently. Slip cannot give it the edited tree nor learn which files it read, so a command with an `@` word is `unverifiable` (Slip's plans expand response files when they are made, `slip:spec/cdb.md` §4). Wanted: `expand_with(argv, read_hook)`, or an expansion that returns the files it read with the words. | `kiln_driver/src/rsp.rs:15`, `:34-39`; test `g4_response_file` |

## 5. Evaluation sessions (`slip:spec/landed/api/WP-089.md` "Findings for Kiln")

KN1, KN2, K5 and K6 of that list were sent as KB2 1.1, KB2 1.2, KB1 7.1
and KB1 7.2.

| # | Kind | Finding | Where |
|---|---|---|---|
| 5.1 | R | **A unit that fails to compile gives no structured diagnostics and no flag for a stack trip** (KN4): `SessionRefused::TuHasErrors` carries a count; the diagnostics go to standard error. Slip recognises a stack trip by a whole line `fatal error: kiln: stack exhausted during <phase>` in the rendering. Wanted: the diagnostics as data, or at least a flag that the guard tripped. | `kiln_driver/src/session.rs:37-47` |
| 5.2 | R | **An uncaught exception's diagnostic has no call stack** (KN5): the note is at the throw, without the "in call to" notes other failures get, as clang's P3068 notes are. An uncaught `slip::error` is thrown in `<slip/meta.h>`, so Slip cannot place the refused call in the transform and reports the entry's place (test `s2_8_pending_the_place_of_a_refusal_is_the_call`). Wanted: the call stack of an uncaught exception, as notes or as data (with KB2 1.3's thrown value). | `kiln_oven/src/notes.rs:336-341` |
| 5.3 | R | **No limit bounds expression nesting** (KN3; no `-fbracket-depth`, by design: `spec/sema-templates.md` "stack guard"). The guard is the only bound, at about 0.45 KiB a parenthesis in substitution, so the failure depends on the stack given rather than on a deterministic limit. Slip treats a trip as the host's failure and ends the session (test `stack.rs` `nested_body`). Wanted: a nesting limit that answers before the guard, as the template depth does. | `spec/sema-templates.md:8750-8757` |

## Not in this batch

- WP-068's other location items are already asked: which uses of an
  argument feed `#` or `##`, and a builtin macro's token against a
  paste, are KR4's (sent 2026-11-22: acceptance cases 1, 5, 6 and 23);
  a buffer given as an overlay is KF2's `FileOrigin::Overlay`.
- A specialisation whose name a macro spells (`template <> struct
  NAME<int> {};`) is absent from Slip's declaration names by Slip's own
  rule (the first token is not the identifier), not by Kiln.
- The rest of KF5 (a named module from an in-memory source) is a
  feature request and goes on the operator's word.
