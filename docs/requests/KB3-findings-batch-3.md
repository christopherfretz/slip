# KB3 — findings for Kiln, batch 3 (2026-11-22)

From Slip's packages WP-045, WP-049, WP-050, WP-084, WP-104, WP-105 and
WP-109 and their independent reviews, at Kiln pin `913a490bb`. As in
[KB1](KB1-findings-batch-1.md) and [KB2](KB2-findings-batch-2.md): none
blocks Slip today, each has a workaround or a pinned test on Slip's
side, and Slip asks only for an answer per item (a work package number,
"as designed", or "not now").

**Kind**: **I** = implementation against behaviour that is already
specified or obvious; **Q** = a question, where the answer may be "as
designed"; **D** = needs a design decision without prior art in Kiln
(hand to whoever does Kiln's design); **R** = a request for something
Kiln does not have (a feature, not a defect: Slip's operator sees these
before Slip presses for any).

Paths are Kiln's unless they start with `slip:`. A `slip:` path under
`spec/landed/api/WP-084.md` or `spec/landed/digest/WP-104.md` is on
Slip's main once that package merges (both are in their fix rounds).

## 1. Semantic analysis

| # | Kind | Finding | Where |
|---|---|---|---|
| 1.1 | I | **A global `operator new` is chained to a friend declared in a class template's pattern.** `act_on_global_new_delete_decl` and `find_global_op` take candidates from `lookup_local(tu, …)`, which holds a friend declared in a class template's pattern; ordinary redeclaration lookup does not. A later global `operator new` is then chained to the pattern's friend, no implicit one is declared, and a friend instantiated before that declaration is left unchained: two entities for one function. Source: `template <class T> struct A { friend void *operator new(decltype(sizeof 0)); }; A<int> a; void *operator new(decltype(sizeof 0));`. Slip's corpus invariant "one key, one entity" found it. | `kiln_sema/src/new_delete.rs`; `slip:spec/landed/model/WP-050.md` "For Kiln" |
| 1.2 | I | **An attribute written on an explicit instantiation is recorded nowhere.** `extern template __attribute__((weak)) void f<int>(int);`: Kiln has no declaration for an explicit instantiation (the specialisation's `SpecKind` stands for it), so nothing can hold the attribute, and a client cannot see that an edit removed it. With KB1 1.1 to 1.3 (the extent rows of explicit instantiations) this is the second thing that wants an explicit instantiation to be a recorded construct of its own. | `slip:spec/landed/digest-kinds/WP-045.md` "Kiln findings" K3 (on merge) |
| 1.3 | I | Addition to KB1 5.2 with the sites: **each level of a nested designated list in a template gets a written form of its own, built afresh**, so `n` levels have `(n + 1)(n + 2) / 2` written lists. `init.rs:2114` calls `note_syntactic_list` for every semantic list built from a designated list, and `dependent_init_list` recurses into an inner list building a new `InitListExpr` and `DesignatedInitExpr` rather than reusing the written form already recorded for it. Instantiation reads only the outermost (`templ/subst/exprs.rs:2935`). Reproducer (n = 2: 3 semantic lists, 6 written; n = 12: 104 `InitListExpr` nodes): `struct N0 { int x; }; struct N1 { N0 a; }; struct N2 { N1 a; }; template <class T> struct A { static inline N2 w = {.a = {.a = {.x = 1}}}; };`. Reusing the inner list's written form, or recording one for the outermost list only, makes it linear. | `kiln_sema/src/templ/actions.rs:2087`, `:2092`; `init.rs:2114`, `:2162` |

## 2. Tokens, ranges and the preprocessing record (`slip:spec/landed/api/WP-084.md` "Until Kiln answers")

Slip's edit ranges are Kiln's (`SourceManager::editable_file_range`,
`kiln_lex::token_end`); where Kiln does not record what a range needs,
Slip refuses the range. Each item below is such a refusal.

| # | Kind | Finding | Where |
|---|---|---|---|
| 2.1 | I | **`kiln_lex::token_end` cannot see a split `>`.** The parser splits `>>` in `split_greater` and records the location in `Sema::split_greaters` (`pub(crate)`), which only Sema's private `token_end` reads. In `B<vt<1>> y;` the expression `vt<1>` ends at the first byte of `>>`, and the public function gives the end of `>>`: a range one byte into `B`'s argument list. Wanted: the set on `Ast`, or a split-aware end of a token. Meanwhile Slip gives no range to any span that ends at `>>`, `>=` or `>>=`. | `kiln_parse/src/parser.rs:1750`; `kiln_sema/src/lib.rs:716`, `impcast.rs:552`; `kiln_lex/src/token_end.rs:32` |
| 2.2 | I | **The call of a user-defined literal begins inside its token.** In `auto a = 12_k + 1;` the literal operator's call, its callee and the operator expression that begins with it all begin at the suffix `_k`, not at `12_k`. Slip checks that a span begins at a token's start and refuses these. Expected: the call begins where the literal token does. | — |
| 2.3 | I | **`#embed`'s tokens have no include link.** Kiln gives the tokens file locations in a fresh `<scratch space>` buffer that the unit never entered, so the buffer is a root: it has no place in the unit's order and no way back to the directive. Wanted: the `#embed` directive as its include location (or as an expansion location). | `kiln_pp/src/directive.rs:1932` |
| 2.4 | R | **The tokens of the unit.** The front end keeps only their count; there is no table of the tokens the preprocessor delivered, nor a query "does a token start here". Slip lexes each buffer again with Kiln's lexer to answer it, which is right only as far as a second lex agrees with the first (a skipped group is the known difference). | `kiln_driver::front_end::FrontEnd` (`tokens: u64`) |
| 2.5 | R | **Directive lines and skipped groups.** `PpRecord` records macro definitions and expansions only; `PpCallbacks` has `source_range_skipped`, `inclusion_directive`, `macro_defined`, `macro_undefined` and `pragma`, but nothing for a taken `#if` family line, `#line`, `#error`, `#warning` or a null directive, and `FrontEndOptions` has no way to install callbacks. Wanted: the span of every directive line and every skipped range of the unit, kept on request like the macro record, or a query "is there a directive or a skipped group inside this range". Slip must refuse an edit whose range holds one, and today finds them by scanning bytes. This is close to KR4's record: if a stage of KR4 already gives it, the answer is that stage's number. | `kiln_pp/src/record.rs`, `callbacks.rs`; `kiln_driver/src/front_end.rs:43` |

## 3. Names and literals (`slip:spec/landed/digest/WP-104.md`)

| # | Kind | Finding | Where |
|---|---|---|---|
| 3.1 | R | **A literal does not say that it came from a builtin macro.** The `StringLiteral` of `__FILE__` and the `IntegerLiteral` of `__LINE__` or `__COUNTER__` are plain literals. Slip therefore writes them by value, and a moved file or a line inserted above changes every unit that holds one; that errs to a spurious difference, never to a wrong equal. A mark on the literal (which builtin macro), or a location from which it can be read, would let Slip treat them as it treats `__func__`. | — |
| 3.2 | Q | **A local class or a closure type as a template argument prints without its function**: `legacy::store<Local>` for a `Local` of `f()` and for a `Local` of `g()` alike. Intended (clang prints `(anonymous class)::` forms and the function for some)? Slip's names for two such specialisations cannot be told apart. | `kiln_ast` type printing |

## 4. Diagnostics: text and count (verdicts agree with clang 19 everywhere)

Each from a small file at global scope; reproducers in
`slip:spec/landed/pitfalls/WP-105.md` "Kiln" and, for 4.7, in
`slip:spec/landed/pitfalls/WP-109.md` (on merge).

| # | Kind | Finding |
|---|---|---|
| 4.1 | Q | `template <> int id<int>(int) { return 1; }` with no `id`: Kiln `extraneous 'template<>' in declaration of variable 'id'` and `expected ';' after top level declarator` (it reads a variable `id` followed by `<`); clang `no function template matches function template specialization 'id'`. |
| 4.2 | Q | `template <> constexpr int width<int> = 4;` with no `width`: Kiln `extraneous 'template<>' in declaration of variable 'width'`, `constexpr variable 'width' must be initialized by a constant expression`, `expected ';'`; clang `no variable template matches specialization`. |
| 4.3 | Q | Addition to KB2 4.1: the same text for `template <> struct hash<int> {};` (clang 19: `explicit specialization of undeclared template struct 'hash'`). |
| 4.4 | Q | `namespace a { template <class T> struct w { template <class U> w(U); }; } namespace b { struct M {}; w(M) -> w<M>; }`: Kiln `no template named 'w'`, `a type specifier is required for all declarations` and `function with trailing return type must specify return type 'auto', not 'int'`; clang `unknown type name 'w'` and `expected ';' after top level declarator`. |
| 4.5 | I | `namespace a { template <class T> struct v {}; } namespace b { struct M {}; template struct v<M>; }` (and with `extern`): Kiln `explicit specialization of non-template struct 'v'` and `explicit instantiation of non-templated type 'b::v'`, so **it declares a struct `b::v`** on the way; clang `no template named 'v'; did you mean 'a::v'?` and `explicit instantiation of 'a::v' must occur in namespace 'a'`. The declared struct is the part that is more than text. |
| 4.6 | Q | `namespace b { class M { int s = 1; }; } int f() { return b::M().s; }`: Kiln names the class unqualified, `'s' is a private member of 'M'`; clang `'s' is a private member of 'b::M'`. |
| 4.7 | Q | The number of errors differs on three rejected trees of Slip's pitfall suite (rows R14 `bad`: clang 2, Kiln 1; R16 `bad`: clang 1, Kiln 2; R19 `bad`: clang 2, Kiln 4). And `#pragma weak f` for an `f` never declared: clang warns ("weak identifier 'f' never declared"), Kiln is silent. |

## Not in this batch

What the fix round of WP-040 leaves standing (the owner of a lambda in
a lambda parameter's default argument; the number of a lambda in a
non-type default template argument) comes with batch 4 when it merges.
