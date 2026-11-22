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
