# KB2 — findings for Kiln, batch 2 (2026-11-22)

From Slip's packages WP-040, WP-046, WP-057 and the independent review
of WP-089, at Kiln pin `913a490bb`. As in
[KB1](KB1-findings-batch-1.md): none blocks Slip today, each has a
workaround or a pinned test on Slip's side, and Slip asks only for an
answer per item (a work package number, "as designed", or "not now").

**Kind**: **I** = implementation against behaviour that is already
specified or obvious; **Q** = a question, where the answer may be "as
designed"; **D** = needs a design decision without prior art in Kiln
(hand to whoever does Kiln's design).

Paths are Kiln's unless they start with `slip:`.

## 1. Evaluation sessions (Slip's run driver, `slip:docs/wp/WP-089.md`)

Item 1.1 bears on WP-2410, which is in flight: please read it before
that lands.

| # | Kind | Finding | Where |
|---|---|---|---|
| 1.1 | D | **A run can reach the stack guard through reflection, and the trip is sticky.** A consteval entry that instantiates at run time (`auto r = std::meta::substitute(^^box, {^^long}); std::meta::size_of(r);`, with `tests/support/kiln_meta.h`) reaches the guard's checks. (a) Away from the compiling thread it trips at any depth: that is KS2 amendment 2 / WP-2410, and Slip's earlier statement that no entry reaches such a check was wrong. (b) On the compiling thread, `R<300>` (a linearly recursive class template) instantiated by `substitute` then `static_data_members_of` then `extract` trips a 4 MiB guard; with 64 MiB it completes at 900, and at 5000 Kiln's own limit answers ("recursive template instantiation exceeded maximum depth of 1024"), which is the right, deterministic failure. After a trip the session's flag stays set and every later run fails. Slip's side: it sizes the stack so that Kiln's limits come first, and treats a trip as the host's failure, ending the session. Asked of Kiln's design: that a stack trip during a session's run be a recoverable failure of that run (a `RunError` of its own, the flag cleared, the session usable), and a statement of which of Kiln's limits bound the depth a run can drive (instantiation depth, constant-evaluation depth, any other), so that a client can size a stack against them rather than by measurement. | `kiln_sema/src/stack_guard.rs:170-205`; reproducers in `slip:spec/landed/api/WP-089.md` |
| 1.2 | I | **A host call refused inside a `requires`-expression is silently `false`.** `template <class T> consteval bool has() { return requires { typename K<slip::lookup("lib::foo").size() + sizeof(T)>; }; }`: outside a run the placeholder refuses the host call, the refusal is treated as a substitution failure, `has<int>()` is `false` and the evaluation goes on. Every other context Slip tried is a diagnosed error. Wanted: a hard error, not a substitution failure, when what failed is a host call that may not be made here. | `kiln_driver/src/session.rs:62` |
| 1.3 | I | Addition to KB1 7.1 (WP-2480): besides the qualified type, **hand over the thrown value** (or its fields), which the note already renders (`unhandled exception of type 'error' with content {2, 1}`). Slip must say which of its refusals an uncaught `slip::error` carries; with the type alone it guesses "the most recent", which is wrong for a rethrown earlier one and for a type derived from `slip::error`. Slip will not parse the rendered note. | `kiln_oven/src/notes.rs:237` |
| 1.4 | I | Addition to KB1 7.2: a `RunError` variant Slip's version of the binding does not know arrives as `Host` too. Slip now names it the host's failure; a `#[non_exhaustive]` enum with a documented catch-all, or a distinct "other" variant, would let it say so without guessing. | `kiln_oven/src/session.rs:352-354` |

## 2. Resolved properties (`slip:docs/wp/WP-046.md`, "Questions for Kiln")

| # | Kind | Finding | Where |
|---|---|---|---|
| 2.1 | Q | **When is a variable's evaluated value (`eval`) written?** At the pin it is written when an initialiser is checked (`check_constexpr_init`): at the declaration, or, for a member of an instantiation or a variable template's specialisation, when the initialiser is instantiated, which a use causes. Is that guaranteed (every instantiated initialiser is checked, so evaluated, when instantiated)? Can a later use also trigger the evaluation of a variable whose initialiser was checked without one? Today it cannot, and there is an asymmetry: `struct S { static const int x = 1; }; int a[S::x];` gives no `eval` for `S::x`, while with `const int S::x = 1;` out of line it has one. Intended? Slip's pair `equal/_/var-init-in-class-or-out-of-line` waits on the answer. | `kiln_sema/src/decl.rs:8876-8902`, `templ/subst/decls.rs:400-464` |
| 2.2 | I | The written exception specification of an explicitly defaulted function is replaced when the specification is resolved. A flag or a kept copy would let a client tell "written `noexcept(false)`, defaulted" from "computed", and Slip narrow the class of functions whose specification it must treat as deferred. | — |

## 3. Local entities (`slip:docs/wp/WP-040.md`, in review on Slip's side)

| # | Kind | Finding | Where |
|---|---|---|---|
| 3.1 | I | An elaborated-type-specifier in the parameter list of a local class's member function is not linked to the later definition in the same block: `void f() { struct A { A *p; void g(struct Q *); }; struct Q { long m; }; }` gives two declarations of `Q`, each its own first declaration. By [basic.scope.pdecl] the first declares `Q` in the smallest enclosing block scope, and the second defines that class. Pinned by Slip's `indeterminate/_/local-class-declared-in-a-member-signature`. | — |
| 3.2 | I | `clone_with_children` does not copy a `LambdaExpr`: it falls to `_ => return None`. | `kiln_ast/src/expr.rs:2492`, `:2583` |
| 3.3 | I | The parse of a chain of functions that each return a closure of the previous one is more than linear in the length of the chain: 2 000 links take minutes in a test profile (Slip's test uses 500). Same family as KB1 5.1 to 5.3. | — |

## 4. Diagnostics

| # | Kind | Finding | Where |
|---|---|---|---|
| 4.1 | Q | For an explicit or partial specialisation whose template is not found (`template <> struct traits<int> {};` with no `traits` in scope) Kiln says `explicit specialization of non-template struct 'traits'`, clang `no template named 'traits'`. The verdict agrees; only the text differs. | `slip:spec/landed/pitfalls/WP-057.md` rows N07, N19 |

## Not in this batch

One finding of the resolved units' package (attributes written on an
explicit instantiation) and what the reviews of WP-040, WP-058 and
WP-084 find come with batch 3 when those packages merge.
