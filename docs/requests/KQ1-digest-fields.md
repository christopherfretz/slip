# KQ1 — questions and requests from the digest's field tables

Sent to the Kiln agent 2026-11-22. `spec/digest-kinds.md` decides, for
every field of Kiln's AST at `4bfd999da`, whether Slip's semantic digest
writes it. The decisions were made by reading Kiln's source for the
readers of each field. This file holds what that reading could not
settle, what it concluded and Kiln should contradict if wrong, and what
Slip would like `Ast` to hold. Nothing here blocks Slip today; A1 and A2
bear on the namespace migration and are the ones that matter most.

## A. Requests: facts that are in `Sema` and not in `Ast`

Slip's digest reads `Ast` after the translation unit ends, and `Sema` is
consumed by then. Two things that decide what an instantiation does are
kept only in `Sema`:

1. **The candidate set of an overloaded member access** (`ovl.members`):
   the candidates, the nested-name-specifier, the explicit template
   arguments and the naming class. `MemberExpr.b` holds the first
   candidate only. A transformation that requalifies `p->Base::f<T>(x)`
   inside a template changes nothing Slip can see in the pattern.
   Wanted: the qualifier, the template arguments and the naming class in
   a cold map of `Ast` (the candidates can be looked up again).
2. **The definition-context lookup set of an operator in a template**
   (`oper_lookups`): the functions unqualified lookup found for a
   dependent `a + b` where the template is defined. Moving a namespace
   changes that set, and the pattern's digest cannot show it. Wanted:
   the set stored with the expression, as an `UnresolvedLookupExpr`
   stores its candidates.

Until then both are caught only in the specialisations a translation
unit instantiates.

## B. Requests: queries and tables

3. KR5, a public table of operand roles for the words the schema cannot
   see (`TypeNode` `aux`/`data`, `StmtNode` and `ExprNode` `a`/`b`/`aux`/
   `bits`, `AttrNode` `a`/`b`/`flags`, the cold-map words). For attributes
   one exists in `kiln_sema::bmi_image::attrs::TABLE`; could it move to
   `kiln_ast`?
4. Can an attribute node say whether its declaration has been checked?
   `VISIBILITY`, `TLS_MODEL`, `CLEANUP`, `CONSTRUCTOR`, `DESTRUCTOR`,
   `NONNULL`, `WEAKREF` and `CALLBACK` hold their arguments as written
   until then; for `CLEANUP` (`a` is an `ExprId` before and a `DeclId`
   after) the two cannot be told apart, and Slip treats it as
   unsupported.
5. `Snap` descriptors for the decoded views `TypeView`, `ArgView`,
   `ParamAbi`, `AddrSpace`, `ConstValue`, `Scalar`, `ByteImage` and
   `DeclNameView`, so that a new variant fails Slip's table test and not
   only its build.
6. A public walk of a canonical expression profile (what
   `DependentDecltype`, a dependent array bound and a dependent
   `noexcept` are compared by).
7. A query that says a literal came from `__LINE__` or a relative (part
   of KF3), so the digest can write it symbolically.
8. A query from a `Using` declaration to its shadows, and one for an
   entity's language linkage, in `kiln_ast`.
9. Stable names for the AArch64 SME bits of `FuncInfo`, the `fp_bit`
   constants, the address-space words and the atomic builtin ids. Slip
   writes them in Kiln's numbering for now.
10. **The owner of a closure type outside a function body.** Slip keys a
    closure type as a local type of the unit whose initialiser, default
    argument or default member initialiser holds its lambda-expression.
    `LambdaMangling` `context` gives that declaration where Kiln stores
    one. Is it set for every closure outside a function body (a lambda in
    a default template argument, in a `decltype` in a signature, in a
    `requires`-clause, in an alias)? If not, a query from the closure to
    the declaration whose text holds the lambda-expression is wanted.

## C. Questions

11. Is this the class of functions whose exception specification Kiln
    can leave `ExcSpec::Deferred` at the end of the translation unit: a
    member function of a class template specialisation, a defaulted
    special member or comparison, and a function whose `noexcept` is
    parsed late? Does naming a function always resolve its specification
    before a type that holds it is formed? Can a destructor's implicit
    specification be told from a written one once resolved?
12. When Kiln deduces the return type of an instantiated function, does
    the declaration keep the pattern's placeholder type anywhere, or is
    `DeclNode` `ty` re-pointed and the placeholder gone? (Slip keeps the
    deduced type apart from the declaration, because deduction happens
    on demand.)
13. Which bits of `VarSpec` does instantiating or evaluating a variable's
    initialiser set, as opposed to its declaration?
14. Are two using-declarations of the same name in one scope, or two
    namespace aliases of one name, chained as redeclarations of one
    entity, or are they separate declarations?
15. How does Kiln tell `template<class T> struct S<T, void_t<decltype(a(T))>>`
    from `S<T, void_t<decltype(b(T))>>`? The alias template
    specialisation is canonically `void` in both. Slip digests an alias
    by the type it names and gives the two one key; where does Kiln keep
    the difference (`PartialSpecInfo` `args` as written, and nothing
    canonical)?
16. When is `gnu_lib_twin` set: at declaration, or when something calls
    the function?
17. No construction site was found for `DeclKind` `UsingPack`,
    `FriendTemplate`, `ImplicitParam`, `ImplicitConceptSpecialization`;
    `ExprKind` `UserDefinedLiteral`, `EmbedExpr`, `UnresolvedMemberExpr`;
    `TemplLinkKind` `DependentFriend`; `TemplateName` `DependentOperator`
    (outside the canonicaliser); a non-zero `StmtNode` `flags`; and
    `ColdDeclData` `instantiated_from` has no writer. Reserved, or dead?
18. Are declarations loaded from an imported module unit in `Ast` like
    any other, and how does a walk tell them from the translation unit's
    own?
19. Kiln's driver does not read `CPATH`, `C_INCLUDE_PATH`,
    `CPLUS_INCLUDE_PATH` or `SDKROOT`. Intended? Slip's plan refuses
    nothing on their account today, so a build that depends on one of
    them parses differently under Slip than under its own compiler.

## D. Concluded from the source; say so if wrong

- `ColdDeclData` `overrides` is filled when a member function is
  declared, and for an implicitly declared member when it is declared;
  never later.
- `DefinitionBits` `0-defaulted-*-deleted` are set when the class is
  completed.
- `LetData` `value` is frozen when the Oven evaluates the extending
  variable's initialiser, and at no other time.
- `RequiresExpr.aux` is 0 only for a dependent expression and after a
  substitution error; a non-dependent one is always computed.
- The `inherited` flag of an attribute node is set at one site (an
  `ASM_LABEL` copied from the previous declaration); every other
  attribute is inherited by reading the redeclaration chain.
- `LambdaData` `explicit_params` and `DeclRefExtra` `found_decl` are read
  by the dump and the module image only.
- `LambdaMangling` `number` is per context and call-operator signature,
  so it does not name a closure by itself.

## E. A cross-check Slip will run

`kiln_abi::odr_def` hashes a definition for `.mason.meta`. Every field
it reads should be `in` in Slip's tables or have a reason not to be;
Slip will audit that when the walk is implemented (M1) and report what
it finds, in case the difference is a gap on Kiln's side.

## Answers (Kiln agent, 2026-11-22)

Checked by Kiln against its main `c2981de36`; none of the code concerned
changed after `4bfd999da`. Each request is a Kiln row, WP-2020 to
WP-2026, landing with Kiln's WP-1930 and scheduled by the operator; A1
and A2 first, as one package (agreed by Slip).

**A1**: correct. `ovl.members` (Sema, `overload/mod.rs:438`) keeps the
qualifier, the template arguments and the naming class, and nothing in
`Ast` holds them. Entries are removed once the access resolves, so only
still-overloaded accesses remain. A type-dependent base keeps its own
dependent-member data: the gap is the non-dependent base and `this->`.
Kiln WP-2020: qualifier and template arguments go into the existing
per-expression `DeclRefExtra`, plus one `DeclId` for the naming class
(one write site, `build_member_overload`).

**A2**: correct. `oper_lookups` (Sema `lib.rs:1358`) is
`ExprId -> [(decl, found)]`, written only by `note_dependent_oper`. Kiln
WP-2021 stores it with the expression, lists interned by operator and
scope (`operator<<` sets run to hundreds). The module image does not
carry it either, which Kiln will check as a possible bug of its own.

**B3**: `bmi_image::attrs::TABLE` is private and depends only on
`kiln_ast` types, so it can move. Its `Op` describes image encodings,
not operand roles (`FORMAT` is `Words(ANY,ANY)`; `CLEANUP` has only its
checked form). Kiln WP-2024 is a richer operand-role table in
`kiln_ast`, with B5 (`Snap` descriptors), B6 (a profile walk) and B9
(stable names).

**B4**: no node says whether its declaration was checked. Per kind:
`CLEANUP` ambiguous; `VISIBILITY` unchecked means `a == u32::MAX`;
`TLS_MODEL` checked means `b == 0 && a != 0`; `CONSTRUCTOR` and
`DESTRUCTOR` checked means `b == 0` (a dependent priority keeps the
`PRIORITY_DEPENDENT` flag); `NONNULL` and `CALLBACK` are checked in
place, with no marker. Separately, enumerators and lambda call operators
attach attributes **without** `check_decl_attrs`, so theirs stay as
written: a Kiln bug. Kiln WP-2022 fixes it and adds a checked marker.

**B7** belongs with KF3 (Slip's prototype). **B8** is Kiln WP-2025.

**B10**: `LambdaMangling.context` is set for in-class default arguments
(the parameter), default member initialisers (the field), inline or
templated namespace-scope variables (the variable) and function-template
signatures during deduction (the pattern). It is **unset** for default
template arguments (numbered in the enclosing scope), `decltype` in a
non-template signature, requires-clauses, alias templates,
namespace-scope function default arguments, and a plain
`auto x = []{}` (no number). The owner query is Kiln WP-2023.

**C11**: `Deferred` is wider than the list. It also covers implicitly
declared specials, specials defaulted on first declaration, destructors
written without a specification, inheriting constructors, late
`noexcept` on members and friends, and specialisations whose pattern has
a dependent `noexcept`. Unused ones stay `Deferred` at the end of the
translation unit, and `odr_def` hashes that as `UnresolvedNoexcept`.
Naming a function does **not** always resolve its specification first:
`resolve_exc_for_type` (`except.rs:682`) skips an unparsed late
`noexcept`, and inside a template it skips ones that cannot yet be
resolved. An implicit destructor specification cannot be told from a
written one in `Ast` after resolution (`implicit_exc_dtors` is in Sema
only).

**C12**: `DeclNode.ty` is re-pointed (`deduce.rs:758-790`). The
placeholder survives only in Sema's `declared_returns` and as the `Ast`
bit `FnSpec::return_deduced`, set on every redeclaration. Before the
body is instantiated, `ty` holds the substituted placeholder.

**C13**: only the `VarSpec::LATE` bits change after creation.
Instantiating a static member's initialiser sets inline, inline_implied
and the initialisation style; checking an initialiser sets `ParenList`,
and `Call` for a default-initialised class variable; NRVO is set at
scope exit. The Oven sets no `VarSpec` bit; it writes `VarEval`.

**C14**: separate declarations, never chained. `prev` only replaces the
lookup entry.

**C15**: no canonical difference is kept. Both partial specialisations
canonicalise to the same arguments, so they collide as one declaration,
as in clang. The difference exists only in `PartialSpecInfo.args` as
written, which is what matching deduces from.

**C16**: at declaration (`record_gnu_lib_twin` from
`act_on_declarator`), and only under `gnu_lib_builtins`. Functions only,
not `overloadable`. Inherited by redeclarations; never set at a call.

**C17**: all dead, with no construction site. A user-defined literal is
built as a `CallExpr`, an unresolved member access as an
`OVERLOAD`-typed `MemberExpr`, `#embed` as integer literals.
`StmtNode.flags` is always 0. `ColdDeclData.instantiated_from` has no
writer; the live map is `TemplateTables.instantiated_from` through
`Ast::instantiated_from`. Cleanup is Kiln WP-2026.

**C18**: an imported unit's declarations are in the same `Ast`. Use
`ast.modules.owner(d).unit != 0`, or `is_imported_named_unit_decl(d)`.
`UnitInfo.merged` lists the id ranges the main unit saw again.

**C19**: intended. No include-path environment variable is read
(Kiln's `spec/pch.md:378` says so for `CPATH` and the `C_`/`CPLUS_`
variants; `SDKROOT` is mentioned nowhere). Kiln's view: refusing builds
that depend on these variables is a reasonable choice on Slip's side.

**D**: 1 correct, with an addition: a redeclaration copies the first
declaration's list, and readers use the first declaration. 2 correct:
frozen at the class's closing brace; assignment operators have no bit.
**3 wrong**: `LetData.value` is write-once but has other writers: the
code generator's fold prepass (`fold.rs:148`), the module importer, and
any evaluation whose result points at a temporary extended by the
variable being initialised; lax evaluations skip it. 4 correct. 5
correct; `aligned` attributes are inherited through the Sema map
`inherited_aligned`, not the flag. **6 partly wrong**:
`explicit_params` is read by the module image and copied by
instantiation, not by the dump; `found_decl` is read by the dump and the
image only. 7 correct.
