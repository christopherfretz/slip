# digest-kinds — the digest's tables

Status: draft (M1). Implemented by `slip_digest`. This file is the
per-kind and per-field companion of [digest.md](digest.md): for every
enumeration Kiln's AST uses and every stored field of every AST record,
payload, side record and cold map it says whether the fact enters the
digest and how. `digest.md` defines everything the tables plug into (units,
keys, streams, references); section numbers written "digest §N" are
sections of that file. The Kiln revision described is the one the `kiln`
submodule pins (`4bfd999da`); the facts were first collected in
`research/kiln-ast-inventory.md` of the context repository and then checked
against the Kiln source, which wins where the two differ (§13).

The tables are read by a test (digest §13 obligation 4), so their form is
fixed by §1. Prose is between tables, never inside a parsed cell.

## 1. The form of the tables

### 1.1 Headings and property lines

A level-3 heading whose whole text is one code span holding a Rust path
that begins `kiln_` introduces a **Kiln type**. The path is the type's name
as Kiln's schema descriptor writes it (`std::any::type_name`, the first
word of `kiln_snap::Schema::descriptors()`), with generic arguments left
off: a heading without arguments covers every instantiation. No other
heading has that form. The last segment of every such path is unique in
this file and is the type's **short name**.

The first non-blank line after the heading is the **property line**: one
code span of `key=value` pairs separated by `; `.

| Key | Values | Meaning |
|---|---|---|
| `kind` | `struct`, `enum`, `root` | the descriptor's kind |
| | `view` | a decoded form Kiln's API returns; it may have no descriptor |
| | `consts` | a newtype whose values are associated constants; the rows are the constants |
| `pin` | `schema` | the type is reached from `kiln_ast::ast::Ast`'s descriptor |
| | `extra` | the type derives `Snap` but is decoded from a packed word, so `Ast`'s descriptor does not reach it; the test registers it by name, and `slip_digest` pins its `SCHEMA_HASH` like any other |
| | `none` | the type has no descriptor; nothing pins it and it is listed in "Open questions" |
| `selector` | a tabled enumeration's path | the record has per-kind words; rows may be prefixed by a variant of this enumeration |
| `base` | a number | added to the row number to give the variant's tag (§1.4) |

The values of `pin` were computed from the source by hand; the first run
of the test corrects any `schema` that should be `extra` or the reverse.

The first table after the property line is the type's table. Descriptors
of kind `id` (every arena index and `QualType`) and of the container kinds
(`int`, `float`, `bool`, `char`, `nonzero`, `unit`, `phantom`, `tuple`,
`array`, `option`, `result`, `box`, `seq`, `str`, `path`, `cell`, `once`,
`lock`, `atomic`, `map`, `set`, `ordmap`, `ordset`, `shared`) have no
table: a container holds what its element's rows say, and an id is covered
by §2.2. Every other descriptor reached from `Ast`, and every type a row
names, must have a heading. Types of `kiln_source` that `Ast` does not
hold (the source manager) are outside this file.

### 1.2 Columns

Every table has the columns `Row`, `Class`, `Decision`, `Encoding`, `Note`.

**Row** is one code span:

```
row     := [ kind "." ] member [ "-" fact ]
kind    := a variant of the selector enumeration
member  := a field name as the descriptor writes it (`0`, `1` for tuple
           fields)
         | a variant name          (tables of kind enum, view, consts)
         | variant "." field       (a field of an enum variant)
fact    := lower-case words joined by "-"
```

A field packing several facts has one row per fact (`spec-inline`) and no
row of its own. A per-kind word has one row per kind and fact
(`IfStmt.aux-kind`). A kind with no row for a per-kind word does not read
that word; nothing in Kiln's types shows that this stays true (§1.6).
A fact row whose fact is `rest` covers every bit or value of the word that
no other row names.

The test checks, both ways: every field and variant of the descriptor has
a row (for a field, a row of its own or at least one fact or per-kind row);
every row's member exists; every `kind` prefix is a variant of the
selector.

**Class** is what kind of fact the row is: `entity` (names a declaration),
`type`, `value` (a constant or a number that is program meaning), `child`
(a statement or expression owned by the node), `name`, `flag` (a boolean
or a small enumeration), `written` (how the source spelled something),
`location`, `bookkeeping` (storage, caches, indices, state of the parse).

**Decision** is one of:

- `in`: the fact is written to the stream as `Encoding` says.
- `out`: the fact is not written. `Note` gives the reason unless the class
  is `location` or `bookkeeping`; when the fact is stored twice the note
  names the row that carries it.
- `symbolic`: written as which builtin or macro produced it, not as its
  value (digest §9).
- `demand`: Kiln fills the field only when the translation unit asks; §11
  says where each goes so that the treatment does not depend on what was
  asked.
- `unsupported`: the first implementation does not digest it. A unit whose
  walk meets the variant, or a set bit or non-default value of the field,
  is indeterminate (digest §10). `Note` gives the reason.

**Encoding** is `-` for a row that is not `in`, except that an
`unsupported` row may say `unbuilt` (no construction site was found in Kiln;
the row needs no test) and an `out` variant may say `as V` (§1.4). For an
`in` row it is one of the forms of §1.3.

### 1.3 Encodings

The primitives are those of `spec/model.md` §2.

| Encoding | What is written |
|---|---|
| `tag N` | variant rows only: the number that stands for the variant (§1.4) |
| `bit` | one bit of the record's flag word (§2.1) |
| `bool` | `bool` |
| `uv`, `sv` | the number |
| `bytes` | `bytes` |
| `sym` | `bytes`: the spelling of a `Symbol`, never its index |
| `string` | a string literal by content: the rows of `Entry` (§10.3) |
| `name` | a `Name` (§3.2) |
| `ref` | a `Ref` (digest §4.1); an absent entity is `Ref::none` |
| `key` | `digest`: the mapped `KeyId` of the entity (digest §3.4) |
| `keyed` | nothing in the stream: the fact is part of the unit's key (§3) |
| `type` | `digest`: the type digest of a `QualType` (§4.1) |
| `value` | a constant by the rows of §8 |
| `child` | the statement or expression node, walked here |
| `decl` | a declaration walked in place (§2.4) |
| `enum T` | `uv`: the tag of the value's variant in the table of `T`, then the `in` rows of that variant's fields in table order |
| `record T` | the `in` rows of `T`'s table, in table order |
| `attrs` | the attribute list by the rule of §10.1 |
| `profile` | a canonical expression operand by the rule of §4.4 |

`T` is a short name. A suffix `?` makes an encoding optional
(`opt<…>`: presence byte first) and a suffix `*` makes it a list (count
first, elements in stored order). `*sorted` is a list sorted by the
elements' encoded bytes, with duplicates removed. Where a row needs a rule
the forms cannot say, the encoding is the nearest form and the prose
directly under the table gives the rule, citing the row.

### 1.4 Tags

Every variant of every enumeration has a number of Slip's own; the row of
a variant that can reach a stream shows it as `tag N`. Tags are never
Kiln's discriminants.

- In a table without `base`, a variant's number is its position among the
  table's variant rows, from 1, whatever the rows' decisions. With
  `base=B` it is `B` plus that position: the node enumerations have
  disjoint ranges (`DeclKind` 256, `StmtKind` 512, `ExprKind` 768,
  `TypeView` 1024, `ValueKind` 1280), so a statement can never be read as
  an expression.
- 0 is never a tag.
- Rows are in Kiln's declaration order at the revision this file describes.
  That is how the numbers were first assigned and nothing more: a variant
  Kiln adds takes the next unused number of its table wherever Kiln
  declares it, and its row is appended.
- A variant Kiln removes keeps its row with decision `unsupported`,
  encoding `unbuilt` and the note `retired`; its number is not reused. The
  test accepts a `retired` row that names no variant.
- A variant decided `out` that is the written form of another variant has
  the encoding `as V`: its values are written with `V`'s tag.
- Any change of a tag, an encoding or a row order changes `scheme`
  (digest §11).

### 1.5 Tests named by rows

A row's **test name** is `<short name>/<row>`. The directories the row
obliges (digest §13 obligation 5), all under `crates/slip_digest/tests/`:

| Row | Directory | The test shows |
|---|---|---|
| `in` | `differ/<short name>/<row>/` | two sources differing in this fact have different digests, in the named unit |
| `out`, class `written` | `equal/<short name>/<row>/` | two sources differing only in this spelling have equal digests |
| `symbolic` | `equal/<short name>/<row>/` | moving the construct to another line or function name leaves the digest equal |
| `demand` | `demand/<short name>/<row>/` | a source, and the same source with an added use that makes Kiln fill the field: every unit in both is equal |
| `unsupported`, not `unbuilt` | `indeterminate/<short name>/<row>/` | the unit holding the construct is flagged indeterminate |

The convention needs no column: the tables test lists the rows, derives
the names and fails for a missing directory or a directory no row names.
`demand/` and `indeterminate/` are directories `digest.md` does not have
yet ("Open questions").

### 1.6 What the test cannot see

The descriptors show fields and variants. They do not show the meaning of
a per-kind word, a bit of a flag word, or a value of a `consts` type. Every
row with a `kind` prefix or a `fact` suffix is therefore outside
obligations 3 and 4 of digest §13, and the words concerned are listed in
"Open questions" as obligation 6 requires.

## 2. Reading a table as a stream

### 2.1 Order

A node or record is written as:

1. the `in` rows of its record's table that have no `kind` prefix, in
   table order. The row of the record's kind field comes first and writes
   the node's tag;
2. the `in` rows prefixed with its kind, in table order. This is the
   node's walk order (digest §4.3): the tables are ordered for it, not by
   the position of the words in Kiln's record. A prefixed row replaces,
   for its kind, an unprefixed row of the same member, at the unprefixed
   row's position;
3. for a declaration or expression, the `in` rows of the maps keyed by
   its id, in table order: for a declaration what §5.1 lists, for an
   expression the rows of `ColdExprData` and then the rows of
   `TemplateTables` whose map is keyed by `ExprId` (§9). Every such row is
   written whatever the node's kind. A map with no entry for the node
   writes the **empty form** of the row's encoding: an absent optional, an
   empty list, `Ref::none`, `false`.

The rows of one record that are encoded `bit` are gathered, in the order
just given, into one flag word written as `uv` at the position of the first
of them; the first row is bit 0. A record nested with `record T` gathers
its own.

A set bit, or a value, that no row of the word's table accounts for makes
the unit indeterminate: the walk has no "other" case (digest §1 rule 6).

### 2.2 Ids

An id (`DeclId`, `ExprId`, `StmtId`, `TypeId`, `QualType`, `ListId`,
`AttrId`, `ConstValueId`, `StringLitId`, `NnsId`, `TemplateNameId`,
`ArgListId`, `ProfileId`, `LambdaId`, `ReflectionId`, `AnnId`, and every
raw index into a side table) is never written. The row of the field that
holds it says what is written instead: the entity's `Ref`, the type's
digest, the node walked in place, the record's rows.

A `DeclId` written as `ref` is first resolved to its entity: through a
`UsingShadow` to its target, through an alias to what it names where the
row says so, and from any redeclaration to the entity's key.

### 2.3 Redeclarations

A unit is an entity, not a declaration (digest §2.1). Its stream is
written from the entity's **defining declaration** if it has one, else
from its first declaration, with these facts merged over every
declaration of the chain: attributes (the union, as §10.1 orders it),
default arguments and default template arguments (each from the
declaration that gives it), the exception specification (from the
declaration the walk reads, §11.2), `inline`, `constexpr`
and `constinit` (set if any declaration sets them), language linkage and
module ownership (from the first declaration). A row merged in another way
says so in its note.

### 2.4 Declarations walked in place

`decl` applies digest §4.3 to a declaration met inside a unit:

- a variable, parameter, binding or label has the ordinal it got when the
  walk first met it, by this declaration or by an earlier reference (a
  `goto` before its label). At its declaration, and not at a reference, its
  ordinal and then its rows are written;
- a local class, enumeration or closure type is written whole, members
  included, under the key `local(n)`;
- a local typedef or alias, a using-declaration, a using-directive and an
  empty declaration write nothing: their effect is in the references
  resolved through them;
- a block-scope declaration of a function or of a variable with linkage
  (`DeclMisc` `misc-local-extern`) is written as a `ref`: the entity is a
  unit of the namespace it belongs to;
- a local `static_assert` is written as its rows.

## 3. Keys

These are Slip's own enumerations; the tables in this section are not
parsed against Kiln. Which key kind each `DeclKind` has is in §5.2.

### 3.1 KeyKind

| Tag | KeyKind | Entities |
|---|---|---|
| 1 | `namespace` | a namespace, all its declarations together |
| 2 | `class` | a class, struct or union that is not a specialisation |
| 3 | `enumeration` | an enumeration |
| 4 | `enumerator` | an enumerator; its parent is the enumeration |
| 5 | `function` | a function, member function, constructor, destructor or conversion function that is not a template |
| 6 | `variable` | a variable or static data member that is not a template |
| 7 | `field` | a non-static data member |
| 8 | `alias` | a typedef name or alias |
| 9 | `class_template` | |
| 10 | `function_template` | |
| 11 | `variable_template` | |
| 12 | `alias_template` | |
| 13 | `concept` | |
| 14 | `specialisation` | an explicit or implicit specialisation; its parent is the template |
| 15 | `partial_specialisation` | its parent is the template |
| 16 | `deduction_guide` | |
| 17 | `using_declaration` | a using-declaration, resolved or dependent |
| 18 | `using_enum` | keyed by content |
| 19 | `namespace_alias` | |
| 20 | `binding` | a structured binding at namespace scope |
| 21 | `decomposition` | the variable of a structured binding declaration at namespace scope |
| 22 | `using_directive` | keyed by content |
| 23 | `static_assert` | keyed by content |
| 24 | `explicit_instantiation` | not used: Kiln records an explicit instantiation as the kind of the specialisation (§9, `SpecKind`) |
| 25 | `friend` | keyed by content |
| 26 | `file_scope_asm` | keyed by content |
| 27 | `consteval_block` | keyed by content |
| 28 | `resolved` | a resolved property (digest §2.2); its parent is the unit it belongs to |
| 29 | `order` | the order unit (digest §2.1) |
| 30 | `default_argument` | an instantiated default argument or default member initialiser; its parent is the function or field |
| 31 | `template_param_object` | a class-type constant used as a template argument; keyed by content |
| 32 | `using_shadow` | one name a using-declaration brings into a scope |
| 33 | `unresolved_if_exists` | the placeholder of a `using_if_exists` declaration whose name does not exist |

### 3.2 Name

| Tag | Name | Then |
|---|---|---|
| 1 | `identifier` | `bytes`: the spelling |
| 2 | `operator` | `enum OverloadedOperator` (§10.2) |
| 3 | `constructor` | |
| 4 | `destructor` | |
| 5 | `conversion` | `type`: the type converted to |
| 6 | `literal_operator` | `bytes`: the suffix |
| 7 | `deduction_guide` | |
| 8 | `anonymous` | the unnamed namespace |
| 9 | `content` | nothing; `disc` is the unit's digest (digest §3.3) |
| 10 | `local` | `uv`: the ordinal among the unit's local types (digest §4.3) |
| 11 | `unnamed_by` | a `Name`: the name that names an unnamed class or enumeration (digest §3.3) |
| 12 | `unnamed_ordinal` | `uv`: the ordinal among the unnamed types of the scope |
| 13 | `property` | `uv`: a resolved property (§11.1) |
| 14 | `order` | |
| 15 | `parameter` | `uv`: the parameter's index (a default argument unit) |
| 16 | `closure` | `uv`, `uv`: a closure type declared outside a function body, by the lambda's mangling context and number (§10.4, `LambdaMangling`) |

A class's constructors and destructor are named `constructor` and
`destructor`, never by the class name: renaming the class changes their
parent key and nothing else.

### 3.3 Discriminators

| KeyKind | `disc` is the digest of |
|---|---|
| `deduction_guide` | the `Ref` of the template deduced, then as `function` |
| `function` | parameter types in order (`type*`), variadic, cv- and ref-qualifiers, explicit-object flag, trailing requires-clause (`profile?`); absent for a function with C language linkage |
| `function_template` | the template head (§9, `TemplateParamList`), then the function's discriminator, then its return type |
| `class_template`, `variable_template`, `alias_template`, `concept` | the template head |
| `specialisation` | the canonical template arguments (`enum ArgView*`) |
| `partial_specialisation` | the template head, then the argument pattern (`enum ArgView*`) |
| `using_declaration` | the key of the scope its qualifier names; for a dependent one, the qualifier by structure (`enum Nns`) |
| `using_shadow` | the key of the entity the name stands for |
| every content-keyed kind, `template_param_object` | the unit's own stream |
| every other kind | absent |

A discriminator is a stream of its own, hashed with the tag
`slip.disc.v1`; the entities in it are `Ref`s and its types are type
digests, so it maps under relocation with everything else (digest §3.4).

## 4. Types

### 4.1 The type digest

The type digest of a `QualType` `q` is `H("slip.type.v1", stream)` where
the stream is, for `c = TypeContext::canonical(q)`: the rows of `Quals` for
`c`'s qualifiers, then `enum TypeView` for `TypeContext::canonical_view(c)`.
A bare `TypeId` is digested as the unqualified `QualType` of it.

The walk reads types only through `TypeView`, `FuncProto`'s accessors,
`ArgView`, `Nns` and `TemplateName`: the decoded forms. The stored records
behind them (`TypeNode`, `TemplateArg`, `KeyedEntry`) have rows so that a
change of their fields is noticed, and those rows are `out`, naming the
view. A view has no descriptor; the walk destructures every variant of a
view without a rest pattern, so a field or variant Kiln adds does not
compile, which stands in for the pin.

A variant marked "sugar" never appears in a canonical type. If
`canonical_view` returns one, or a field noted "sugar only" is present in
a canonical node, the unit is indeterminate.

### 4.2 Adjustments to Kiln's canonical form

Kiln's canonical type is the structure the digest wants, except where it
keeps a spelling or leaves a fact outside the type. Each is a rule here:

1. **First spellings.** A dependent `TemplateSpecialization`, `decltype`
   or array bound is canonical by profile; `TypeContext::arg_origins` and
   `expr_origins` keep the expressions of the first spelling for mangling
   and printing. They are `out`: the profile is the fact (§4.4).
2. **Alias template specialisations** are sugar; `alias_converted` holds
   their converted arguments and is `out`. The type the alias names is what
   is digested.
3. **Exception specifications before C++17.** With
   `TypeOptions::noexcept_in_type` false a canonical function type has
   `ExcSpec::None` and `exc_protos` keeps the written specification of each
   non-canonical function node. A type digest is taken from the canonical
   type in every language mode. The exception specification of a
   *function declaration* is a field of the function's unit (§5.3,
   `DeclNode`, `ty-exception-spec`), read from the declared type with
   `TypeContext::proto` and `exc_spec`, which answer with the written
   specification in every language mode, so it is compared whether or not
   the type holds it.
4. **Deferred exception specifications.** `ExcSpec::Deferred(d)` is
   canonical and distinct per declaration, and Kiln re-points the
   declaration's type when the specification is resolved. Whether it has
   been resolved depends on what the translation unit used. §11.2 gives
   the rule; in short, a type digest writes a deferred specification as the
   tag of `Deferred` and the `Ref` of `d`, and a function's own unit never
   writes the specification inside its type.
5. **Calling conventions.** Kiln canonicalises a convention that is the
   target's default to `CallConv::Default`; nothing further is done.
6. **Trailing return types.** `FuncInfo` bit 11 is spelling; Kiln clears
   it in the canonical type and the digest ignores it everywhere.
7. **Parentheses in `decltype`.** A non-dependent `decltype((x))` has a
   different canonical type from `decltype(x)`, so nothing is needed. In a
   `DependentDecltype` the operand is walked by §4.4, where a `ParenExpr`
   that is the whole operand is written (§7, `ExprNode`,
   `ParenExpr.a`).

### 4.3 The type records

### `kiln_ast::types::kinds::Quals`

`kind=struct; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `0-const` | flag | in | bit |  |
| `0-volatile` | flag | in | bit |  |
| `0-restrict` | flag | in | bit |  |

### `kiln_ast::types::kinds::TypeFlags`

`kind=struct; pin=schema`

Derived from a node's operands by Kiln when the node is built.

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `0-contains-error` | flag | unsupported | - | an error type: digest §10 |
| `0-rest` | bookkeeping | out | - | dependent, instantiation-dependent, variably modified, unexpanded pack, undeduced, sugar: each follows from the operands |

### `kiln_ast::types::node::TypeNode`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `kind` | flag | out | - | stored discriminant; the walk reads `TypeView` |
| `flags-contains-error` | flag | unsupported | - | an error type: digest §10 |
| `flags-rest` | bookkeeping | out | - |  |
| `aux` | bookkeeping | out | - | per-kind packed word; decoded by `TypeView` |
| `data` | bookkeeping | out | - | per-kind packed word or side index; decoded by `TypeView` |

### `kiln_ast::types::kinds::TypeKind`

`kind=enum; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Invalid` | flag | out | - | stored discriminant; `TypeView` has a variant of the same name |
| `Builtin` | flag | out | - | stored discriminant; `TypeView` has a variant of the same name |
| `BitInt` | flag | out | - | stored discriminant; `TypeView` has a variant of the same name |
| `DependentBitInt` | flag | out | - | stored discriminant; `TypeView` has a variant of the same name |
| `Pointer` | flag | out | - | stored discriminant; `TypeView` has a variant of the same name |
| `LValueRef` | flag | out | - | stored discriminant; `TypeView` has a variant of the same name |
| `RValueRef` | flag | out | - | stored discriminant; `TypeView` has a variant of the same name |
| `MemberPointer` | flag | out | - | stored discriminant; `TypeView` has a variant of the same name |
| `ConstantArray` | flag | out | - | stored discriminant; `TypeView` has a variant of the same name |
| `IncompleteArray` | flag | out | - | stored discriminant; `TypeView` has a variant of the same name |
| `VariableArray` | flag | out | - | stored discriminant; `TypeView` has a variant of the same name |
| `DependentSizedArray` | flag | out | - | stored discriminant; `TypeView` has a variant of the same name |
| `Function` | flag | out | - | stored discriminant; `TypeView` has a variant of the same name |
| `Record` | flag | out | - | stored discriminant; `TypeView` has a variant of the same name |
| `Enum` | flag | out | - | stored discriminant; `TypeView` has a variant of the same name |
| `Complex` | flag | out | - | stored discriminant; `TypeView` has a variant of the same name |
| `Atomic` | flag | out | - | stored discriminant; `TypeView` has a variant of the same name |
| `Vector` | flag | out | - | stored discriminant; `TypeView` has a variant of the same name |
| `DependentVector` | flag | out | - | stored discriminant; `TypeView` has a variant of the same name |
| `ExtQual` | flag | out | - | stored discriminant; `TypeView` has a variant of the same name |
| `Typedef` | flag | out | - | stored discriminant; `TypeView` has a variant of the same name |
| `Using` | flag | out | - | stored discriminant; `TypeView` has a variant of the same name |
| `Elaborated` | flag | out | - | stored discriminant; `TypeView` has a variant of the same name |
| `QualifiedArray` | flag | out | - | stored discriminant; `TypeView` has a variant of the same name |
| `TemplateTypeParm` | flag | out | - | stored discriminant; `TypeView` has a variant of the same name |
| `CanonTemplateTypeParm` | flag | out | - | stored discriminant; `TypeView` has a variant of the same name |
| `SubstTemplateTypeParm` | flag | out | - | stored discriminant; `TypeView` has a variant of the same name |
| `SubstTemplateTypeParmPack` | flag | out | - | stored discriminant; `TypeView` has a variant of the same name |
| `TemplateSpecialization` | flag | out | - | stored discriminant; `TypeView` has a variant of the same name |
| `InjectedClassName` | flag | out | - | stored discriminant; `TypeView` has a variant of the same name |
| `DependentName` | flag | out | - | stored discriminant; `TypeView` has a variant of the same name |
| `DependentTemplateSpecialization` | flag | out | - | stored discriminant; `TypeView` has a variant of the same name |
| `PackExpansion` | flag | out | - | stored discriminant; `TypeView` has a variant of the same name |
| `PackIndexing` | flag | out | - | stored discriminant; `TypeView` has a variant of the same name |
| `Decltype` | flag | out | - | stored discriminant; `TypeView` has a variant of the same name |
| `DependentDecltype` | flag | out | - | stored discriminant; `TypeView` has a variant of the same name |
| `TypeOf` | flag | out | - | stored discriminant; `TypeView` has a variant of the same name |
| `UnaryTransform` | flag | out | - | stored discriminant; `TypeView` has a variant of the same name |
| `Auto` | flag | out | - | stored discriminant; `TypeView` has a variant of the same name |
| `DeducedTemplateSpecialization` | flag | out | - | stored discriminant; `TypeView` has a variant of the same name |
| `Attributed` | flag | out | - | stored discriminant; `TypeView` has a variant of the same name |
| `Splice` | flag | out | - | stored discriminant; `TypeView` has a variant of the same name |
| `UnresolvedUsing` | flag | out | - | stored discriminant; `TypeView` has a variant of the same name |
| `Paren` | flag | out | - | stored discriminant; `TypeView` has a variant of the same name |
| `DependentAddrSpace` | flag | out | - | stored discriminant; `TypeView` has a variant of the same name |

### `kiln_ast::types::context::TypeView`

`kind=view; pin=none; base=1024`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Invalid` | type | unsupported | - | the dummy node and the reserved ids; the type of nothing |
| `Builtin` | type | in | tag 1026 |  |
| `Builtin.0` | type | in | enum BuiltinType |  |
| `BitInt` | type | in | tag 1027 |  |
| `BitInt.signed` | flag | in | bool |  |
| `BitInt.width` | value | in | uv |  |
| `DependentBitInt` | type | in | tag 1028 |  |
| `DependentBitInt.signed` | flag | in | bool |  |
| `DependentBitInt.width` | value | in | profile |  |
| `Pointer` | type | in | tag 1029 |  |
| `Pointer.0` | type | in | type |  |
| `LValueRef` | type | in | tag 1030 |  |
| `LValueRef.0` | type | in | type |  |
| `RValueRef` | type | in | tag 1031 |  |
| `RValueRef.0` | type | in | type |  |
| `MemberPointer` | type | in | tag 1032 |  |
| `MemberPointer.pointee` | type | in | type |  |
| `MemberPointer.class` | type | in | type |  |
| `ConstantArray` | type | in | tag 1033 |  |
| `ConstantArray.0` | type | in | record ConstArray |  |
| `IncompleteArray` | type | in | tag 1034 |  |
| `IncompleteArray.0` | type | in | type |  |
| `VariableArray` | type | unsupported | - | the bound is evaluated where the type is declared, which may be a local typedef the walk skips; a GNU extension in C++ |
| `VariableArray.elem` | type | unsupported | - | as `VariableArray` |
| `VariableArray.size` | child | unsupported | - | as `VariableArray` |
| `DependentSizedArray` | type | in | tag 1036 |  |
| `DependentSizedArray.elem` | type | in | type |  |
| `DependentSizedArray.size` | value | in | enum ExprOperand |  |
| `Function` | type | in | tag 1037 |  |
| `Function.0` | type | in | record FuncProto |  |
| `Record` | type | in | tag 1038 |  |
| `Record.0` | entity | in | ref |  |
| `Enum` | type | in | tag 1039 |  |
| `Enum.0` | entity | in | ref |  |
| `UnresolvedUsing` | type | in | tag 1040 |  |
| `UnresolvedUsing.0` | entity | in | ref |  |
| `Paren` | written | out | - | sugar: declarator parentheses |
| `Paren.0` | written | out | - | sugar |
| `Complex` | type | in | tag 1042 |  |
| `Complex.0` | type | in | type |  |
| `Atomic` | type | in | tag 1043 |  |
| `Atomic.0` | type | in | type |  |
| `Vector` | type | in | tag 1044 |  |
| `Vector.elem` | type | in | type |  |
| `Vector.count` | value | in | uv |  |
| `Vector.kind` | flag | in | enum VectorKind |  |
| `DependentVector` | type | in | tag 1045 |  |
| `DependentVector.elem` | type | in | type |  |
| `DependentVector.size` | value | in | enum ExprOperand |  |
| `DependentVector.kind` | flag | in | enum VectorKind |  |
| `ExtQual` | type | in | tag 1046 |  |
| `ExtQual.base` | type | in | type |  |
| `ExtQual.word` | flag | in | record AddrSpace |  |
| `DependentAddrSpace` | type | in | tag 1047 |  |
| `DependentAddrSpace.pointee` | type | in | type |  |
| `DependentAddrSpace.space` | value | in | enum ExprOperand |  |
| `Typedef` | written | out | - | sugar: the alias a type was reached through (digest §1 rule 2) |
| `Typedef.decl` | written | out | - | sugar |
| `Typedef.underlying` | written | out | - | sugar |
| `Using` | written | out | - | sugar: the using-declaration a type was reached through |
| `Using.shadow` | written | out | - | sugar |
| `Using.underlying` | written | out | - | sugar |
| `Elaborated` | written | out | - | sugar: keyword and qualifier as written |
| `Elaborated.keyword` | written | out | - | sugar |
| `Elaborated.nns` | written | out | - | sugar |
| `Elaborated.named` | written | out | - | sugar |
| `QualifiedArray` | written | out | - | sugar: qualifiers written on an array type |
| `QualifiedArray.quals` | written | out | - | sugar |
| `QualifiedArray.base` | written | out | - | sugar |
| `TemplateTypeParm` | written | out | - | sugar: the parameter's declaration; canonical is `CanonTemplateTypeParm` |
| `TemplateTypeParm.0` | written | out | - | sugar |
| `CanonTemplateTypeParm` | type | in | tag 1053 |  |
| `CanonTemplateTypeParm.depth` | value | in | uv |  |
| `CanonTemplateTypeParm.index` | value | in | uv |  |
| `CanonTemplateTypeParm.pack` | flag | in | bool |  |
| `SubstTemplateTypeParm` | written | out | - | sugar: canonical is the replacement |
| `SubstTemplateTypeParm.replacement` | written | out | - | sugar |
| `SubstTemplateTypeParm.associated` | written | out | - | sugar |
| `SubstTemplateTypeParm.index` | written | out | - | sugar |
| `SubstTemplateTypeParm.pack_index` | written | out | - | sugar |
| `SubstTemplateTypeParmPack` | type | in | tag 1055 |  |
| `SubstTemplateTypeParmPack.associated` | entity | in | ref |  |
| `SubstTemplateTypeParmPack.index` | value | in | uv |  |
| `SubstTemplateTypeParmPack.pack` | value | in | enum ArgView* |  |
| `TemplateSpecialization` | type | in | tag 1056 |  |
| `TemplateSpecialization.name` | entity | in | enum TemplateName | canonical only when dependent |
| `TemplateSpecialization.args` | value | in | enum ArgView* |  |
| `TemplateSpecialization.underlying` | written | out | - | sugar only: absent in a canonical node |
| `TemplateSpecialization.alias` | written | out | - | sugar only: false in a canonical node (§4.2 rule 2) |
| `InjectedClassName` | written | out | - | sugar: canonical is the template's own specialisation |
| `InjectedClassName.pattern` | written | out | - | sugar |
| `InjectedClassName.injected` | written | out | - | sugar |
| `DependentName` | type | in | tag 1058 |  |
| `DependentName.keyword` | flag | in | enum TagKeyword | a tag keyword restricts what the name may find at instantiation |
| `DependentName.nns` | name | in | enum Nns |  |
| `DependentName.name` | name | in | sym |  |
| `DependentTemplateSpecialization` | type | in | tag 1059 |  |
| `DependentTemplateSpecialization.keyword` | flag | in | enum TagKeyword |  |
| `DependentTemplateSpecialization.nns` | name | in | enum Nns |  |
| `DependentTemplateSpecialization.name` | name | in | sym |  |
| `DependentTemplateSpecialization.args` | value | in | enum ArgView* |  |
| `PackExpansion` | type | in | tag 1060 |  |
| `PackExpansion.pattern` | type | in | type |  |
| `PackExpansion.num_expansions` | value | in | uv? |  |
| `PackIndexing` | type | in | tag 1061 |  |
| `PackIndexing.pattern` | type | in | type | canonical only when dependent |
| `PackIndexing.index` | value | in | enum ExprOperand |  |
| `PackIndexing.selected` | written | out | - | sugar only: absent in a canonical node |
| `Decltype` | written | out | - | sugar: canonical is the underlying type |
| `Decltype.expr` | written | out | - | sugar |
| `Decltype.underlying` | written | out | - | sugar |
| `DependentDecltype` | type | in | tag 1063 |  |
| `DependentDecltype.0` | value | in | profile |  |
| `TypeOf` | type | in | tag 1064 |  |
| `TypeOf.operand` | value | in | enum TypeOfOperand | canonical only when dependent |
| `TypeOf.underlying` | written | out | - | sugar only |
| `TypeOf.unqual` | flag | in | bool |  |
| `UnaryTransform` | type | in | tag 1065 |  |
| `UnaryTransform.kind` | flag | in | enum UnaryTransformKind | canonical only when dependent |
| `UnaryTransform.base` | type | in | type |  |
| `UnaryTransform.result` | written | out | - | sugar only |
| `Auto` | type | in | tag 1066 |  |
| `Auto.keyword` | flag | in | enum AutoKeyword | canonical only when undeduced |
| `Auto.deduced` | written | out | - | sugar only: a deduced placeholder's canonical type is the deduced type |
| `Auto.concept` | entity | in | ref |  |
| `Auto.args` | value | in | enum ArgView*? |  |
| `Auto.pack` | flag | in | bool |  |
| `DeducedTemplateSpecialization` | type | in | tag 1067 |  |
| `DeducedTemplateSpecialization.name` | entity | in | enum TemplateName | canonical only when undeduced |
| `DeducedTemplateSpecialization.deduced` | written | out | - | sugar only |
| `Attributed` | written | out | - | sugar: canonical is the equivalent type, which holds what the attribute did to the type (calling convention, `noreturn`); nullability, `noderef` and `lifetimebound` change no generated code |
| `Attributed.kind` | written | out | - | sugar |
| `Attributed.modified` | written | out | - | sugar |
| `Attributed.equivalent` | written | out | - | sugar |
| `Splice` | type | unsupported | - | reflection (§12) |
| `Splice.operand` | value | unsupported | - | reflection (§12) |
| `Splice.designated` | type | unsupported | - | reflection (§12) |

### `kiln_ast::types::kinds::BuiltinType`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Void` | type | in | tag 1 |  |
| `Bool` | type | in | tag 2 |  |
| `Char` | type | in | tag 3 |  |
| `SChar` | type | in | tag 4 |  |
| `UChar` | type | in | tag 5 |  |
| `WChar` | type | in | tag 6 |  |
| `Char8` | type | in | tag 7 |  |
| `Char16` | type | in | tag 8 |  |
| `Char32` | type | in | tag 9 |  |
| `Short` | type | in | tag 10 |  |
| `UShort` | type | in | tag 11 |  |
| `Int` | type | in | tag 12 |  |
| `UInt` | type | in | tag 13 |  |
| `Long` | type | in | tag 14 |  |
| `ULong` | type | in | tag 15 |  |
| `LongLong` | type | in | tag 16 |  |
| `ULongLong` | type | in | tag 17 |  |
| `Int128` | type | in | tag 18 |  |
| `UInt128` | type | in | tag 19 |  |
| `Float16` | type | in | tag 20 |  |
| `BFloat16` | type | in | tag 21 |  |
| `Float` | type | in | tag 22 |  |
| `Double` | type | in | tag 23 |  |
| `LongDouble` | type | in | tag 24 |  |
| `Float128` | type | in | tag 25 |  |
| `Half` | type | in | tag 26 |  |
| `NullPtr` | type | in | tag 27 |  |
| `MetaInfo` | type | in | tag 28 |  |
| `Dependent` | type | in | tag 29 |  |
| `Overload` | type | in | tag 30 |  |
| `BoundMember` | type | in | tag 31 |  |
| `BuiltinFn` | type | in | tag 32 |  |
| `Error` | type | unsupported | - | the error type: digest §10 |
| `SvInt8` | type | in | tag 34 |  |
| `SvInt16` | type | in | tag 35 |  |
| `SvInt32` | type | in | tag 36 |  |
| `SvInt64` | type | in | tag 37 |  |
| `SvUint8` | type | in | tag 38 |  |
| `SvUint16` | type | in | tag 39 |  |
| `SvUint32` | type | in | tag 40 |  |
| `SvUint64` | type | in | tag 41 |  |
| `SvFloat16` | type | in | tag 42 |  |
| `SvFloat32` | type | in | tag 43 |  |
| `SvFloat64` | type | in | tag 44 |  |
| `SvBfloat16` | type | in | tag 45 |  |
| `SvBool` | type | in | tag 46 |  |
| `SvCount` | type | in | tag 47 |  |
| `Float32` | type | in | tag 48 |  |
| `Float64` | type | in | tag 49 |  |
| `StdFloat128` | type | in | tag 50 |  |
| `Float32x` | type | in | tag 51 |  |
| `Float64x` | type | in | tag 52 |  |
| `SvInt8x2` | type | in | tag 53 |  |
| `SvInt16x2` | type | in | tag 54 |  |
| `SvInt32x2` | type | in | tag 55 |  |
| `SvInt64x2` | type | in | tag 56 |  |
| `SvUint8x2` | type | in | tag 57 |  |
| `SvUint16x2` | type | in | tag 58 |  |
| `SvUint32x2` | type | in | tag 59 |  |
| `SvUint64x2` | type | in | tag 60 |  |
| `SvFloat16x2` | type | in | tag 61 |  |
| `SvFloat32x2` | type | in | tag 62 |  |
| `SvFloat64x2` | type | in | tag 63 |  |
| `SvBfloat16x2` | type | in | tag 64 |  |
| `SvInt8x3` | type | in | tag 65 |  |
| `SvInt16x3` | type | in | tag 66 |  |
| `SvInt32x3` | type | in | tag 67 |  |
| `SvInt64x3` | type | in | tag 68 |  |
| `SvUint8x3` | type | in | tag 69 |  |
| `SvUint16x3` | type | in | tag 70 |  |
| `SvUint32x3` | type | in | tag 71 |  |
| `SvUint64x3` | type | in | tag 72 |  |
| `SvFloat16x3` | type | in | tag 73 |  |
| `SvFloat32x3` | type | in | tag 74 |  |
| `SvFloat64x3` | type | in | tag 75 |  |
| `SvBfloat16x3` | type | in | tag 76 |  |
| `SvInt8x4` | type | in | tag 77 |  |
| `SvInt16x4` | type | in | tag 78 |  |
| `SvInt32x4` | type | in | tag 79 |  |
| `SvInt64x4` | type | in | tag 80 |  |
| `SvUint8x4` | type | in | tag 81 |  |
| `SvUint16x4` | type | in | tag 82 |  |
| `SvUint32x4` | type | in | tag 83 |  |
| `SvUint64x4` | type | in | tag 84 |  |
| `SvFloat16x4` | type | in | tag 85 |  |
| `SvFloat32x4` | type | in | tag 86 |  |
| `SvFloat64x4` | type | in | tag 87 |  |
| `SvBfloat16x4` | type | in | tag 88 |  |
| `SvBoolx2` | type | in | tag 89 |  |
| `SvBoolx4` | type | in | tag 90 |  |
| `SvMfloat8` | type | in | tag 91 |  |
| `SvMfloat8x2` | type | in | tag 92 |  |
| `SvMfloat8x3` | type | in | tag 93 |  |
| `SvMfloat8x4` | type | in | tag 94 |  |
| `MFloat8` | type | in | tag 95 |  |

`Dependent`, `Overload`, `BoundMember` and `BuiltinFn` are placeholder
types of expressions in templates and of unresolved names; they are
written like any other builtin.

### `kiln_ast::types::node::ConstArray`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `elem` | type | in | type |  |
| `size` | value | in | uv |  |
| `size_expr` | written | out | - | the bound as written; absent in a canonical node, and `size` is the fact |

### `kiln_ast::types::kinds::AddrSpace`

`kind=struct; pin=none`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `0` | flag | in | uv | the address-space word as Kiln holds it; its values are not tabled ("Open questions") |

### `kiln_ast::types::kinds::TagKeyword`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `None` | flag | in | tag 1 |  |
| `Struct` | flag | in | tag 2 |  |
| `Class` | written | out | as Struct | `struct` against `class` (digest §1 rule 2) |
| `Union` | flag | in | tag 4 |  |
| `Enum` | flag | in | tag 5 |  |
| `Typename` | flag | in | tag 6 |  |
| `Interface` | flag | in | tag 7 |  |

### `kiln_ast::types::kinds::VectorKind`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Generic` | flag | in | tag 1 |  |
| `Neon` | flag | in | tag 2 |  |
| `NeonPoly` | flag | in | tag 3 |  |
| `Ext` | flag | in | tag 4 |  |
| `SveFixedData` | flag | in | tag 5 |  |
| `SveFixedPredicate` | flag | in | tag 6 |  |

### `kiln_ast::types::kinds::AutoKeyword`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Auto` | flag | in | tag 1 |  |
| `DecltypeAuto` | flag | in | tag 2 |  |
| `GnuAutoType` | flag | in | tag 3 |  |

### `kiln_ast::types::kinds::UnaryTransformKind`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `AddLvalueReference` | flag | in | tag 1 |  |
| `AddPointer` | flag | in | tag 2 |  |
| `AddRvalueReference` | flag | in | tag 3 |  |
| `Decay` | flag | in | tag 4 |  |
| `MakeSigned` | flag | in | tag 5 |  |
| `MakeUnsigned` | flag | in | tag 6 |  |
| `RemoveAllExtents` | flag | in | tag 7 |  |
| `RemoveConst` | flag | in | tag 8 |  |
| `RemoveCv` | flag | in | tag 9 |  |
| `RemoveCvref` | flag | in | tag 10 |  |
| `RemoveExtent` | flag | in | tag 11 |  |
| `RemovePointer` | flag | in | tag 12 |  |
| `RemoveReference` | flag | in | tag 13 |  |
| `RemoveRestrict` | flag | in | tag 14 |  |
| `RemoveVolatile` | flag | in | tag 15 |  |
| `UnderlyingType` | flag | in | tag 16 |  |

### `kiln_ast::types::kinds::TypeAttrKind`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Nonnull` | written | out | - | only in `TypeView::Attributed`, which is sugar |
| `Nullable` | written | out | - | only in `TypeView::Attributed`, which is sugar |
| `NullUnspecified` | written | out | - | only in `TypeView::Attributed`, which is sugar |
| `NullableResult` | written | out | - | only in `TypeView::Attributed`, which is sugar |
| `NoDeref` | written | out | - | only in `TypeView::Attributed`, which is sugar |
| `CallingConv` | written | out | - | only in `TypeView::Attributed`, which is sugar |
| `NoReturn` | written | out | - | only in `TypeView::Attributed`, which is sugar |
| `LifetimeBound` | written | out | - | only in `TypeView::Attributed`, which is sugar |
| `Other` | written | out | - | only in `TypeView::Attributed`, which is sugar |

### `kiln_ast::types::context::ExprOperand`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Expr` | written | out | - | sugar: the expression as written; a canonical type holds `Profile` |
| `Expr.0` | written | out | - | sugar |
| `Profile` | value | in | tag 2 |  |
| `Profile.0` | value | in | profile |  |

### `kiln_ast::types::context::TypeOfOperand`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Expr` | written | out | - | sugar |
| `Expr.0` | written | out | - | sugar |
| `Type` | written | out | - | sugar: `typeof(type)` is canonically the type |
| `Type.0` | written | out | - | sugar |
| `Profile` | value | in | tag 3 |  |
| `Profile.0` | value | in | profile |  |

### 4.4 Canonical expression operands

A `ProfileId` names a canonical expression: Kiln's private word format,
whose words hold ids local to the parse. The digest does not read the
words. `profile` is written as the expression `ProfileEntry.first_expr`
(`TypeContext::profile_expr`), walked by §7, inside the type or key stream
being written, with these rules:

- If `first_expr` is absent the unit is indeterminate.
- Entities declared inside the walked expression (the parameters of a
  requires-expression or of a lambda) are numbered from 0 within the
  stream being written. A reference to a local entity declared outside it
  uses the unit's ordinal (digest §4.3); a type digest that contains such a
  reference is memoised for the unit only.
- A function parameter named in a signature is `Ref::param`, a template
  parameter `Ref::template_param`; neither depends on the unit.

Two parses that agree on the program walk the same first spelling, because
both meet the spellings in the same order. A transformation that changes
which of two equivalent spellings comes first can change the digest; that
errs on the side of a reported difference ("Open questions").

### `kiln_ast::types::profile::ProfileEntry`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `start` | bookkeeping | out | - | range of the profile's words: Kiln's private format |
| `len` | bookkeeping | out | - |  |
| `first_expr` | child | in | child | §4.4; absent makes the unit indeterminate |
| `flags` | bookkeeping | out | - | `TypeFlags` of the profile |

### `kiln_ast::types::profile::ProfileTable`

`kind=root; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `words` | bookkeeping | out | - | the profile words: parse-local ids, not read (§4.4) |
| `entries` | bookkeeping | out | - | arena: reached through `ProfileId` |
| `intern.absent` | bookkeeping | out | - |  |
| `intern` | bookkeeping | out | - |  |

### 4.5 Function types

### `kiln_ast::types::func::FuncProto`

`kind=struct; pin=schema`

`params`, `info` and `exc` are read through `FuncProto::params()`,
`info()` and `exc()`.

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `ret` | type | in | type |  |
| `params` | type | in | type* | the adjusted parameter types, in order |
| `info` | flag | in | record FuncInfo |  |
| `exc` | value | in | enum ExcSpec | in normal form (below); §4.2 rules 3 and 4 |
| `n_params` | bookkeeping | out | - | the length of `params` |
| `abis` | flag | in | enum ParamAbi* | one per parameter; an absent list is written as the empty list |

### `kiln_ast::types::func::FuncInfo`

`kind=struct; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `0-variadic` | flag | in | bit |  |
| `0-ref-qual` | flag | in | enum RefQual | bits 1-2 |
| `0-no-proto` | flag | unsupported | - | bits 1-2 holding 3: a C function without a prototype; C is not digested |
| `0-this-const` | flag | in | bit |  |
| `0-this-volatile` | flag | in | bit |  |
| `0-this-restrict` | flag | in | bit |  |
| `0-cc` | flag | in | enum CallConv |  |
| `0-noreturn` | flag | in | bit |  |
| `0-trailing-return` | written | out | - | spelling of the return type (§4.2 rule 6) |
| `0-no-caller-saved-registers` | flag | in | bit |  |
| `0-regparm` | value | in | uv |  |
| `0-sme` | flag | in | uv | the nine AArch64 SME bits as Kiln packs them; not broken out ("Open questions") |
| `1` | flag | out | - | the parameter-ABI list: same list as `FuncProto` `abis` |

### `kiln_ast::types::func::ParamAbi`

`kind=enum; pin=none`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Ordinary` | flag | in | tag 1 |  |
| `SwiftContext` | flag | in | tag 2 |  |
| `SwiftAsyncContext` | flag | in | tag 3 |  |
| `SwiftErrorResult` | flag | in | tag 4 |  |
| `SwiftIndirectResult` | flag | in | tag 5 |  |

### `kiln_ast::types::kinds::RefQual`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `None` | flag | in | tag 1 |  |
| `LValue` | flag | in | tag 2 |  |
| `RValue` | flag | in | tag 3 |  |

### `kiln_ast::types::kinds::CallConv`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Default` | flag | in | tag 1 |  |
| `C` | flag | in | tag 2 |  |
| `IntelOclBicc` | flag | in | tag 3 |  |
| `Swift` | flag | in | tag 4 |  |
| `SwiftAsync` | flag | in | tag 5 |  |
| `VectorCall` | flag | in | tag 6 |  |
| `RegCall` | flag | in | tag 7 |  |
| `PreserveMost` | flag | in | tag 8 |  |
| `PreserveAll` | flag | in | tag 9 |  |
| `SysV` | flag | in | tag 10 |  |
| `Win64` | flag | in | tag 11 |  |
| `Aapcs` | flag | in | tag 12 |  |
| `AapcsVfp` | flag | in | tag 13 |  |
| `AArch64VectorCall` | flag | in | tag 14 |  |
| `AArch64SveCall` | flag | in | tag 15 |  |
| `PreserveNone` | flag | in | tag 16 |  |

### `kiln_ast::types::func::ExcSpec`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `None` | flag | in | tag 1 |  |
| `Nothrow` | flag | in | tag 2 |  |
| `DynamicNone` | written | out | as Nothrow | `throw()`: non-throwing, as `TypeContext::exc_identity` has it |
| `NothrowAttr` | written | out | as Nothrow | `__attribute__((nothrow))`: non-throwing, as `TypeContext::exc_identity` has it |
| `BasicNoexcept` | written | out | as Nothrow | `noexcept` without an operand |
| `NoexceptExpr` | written | out | as Nothrow | written by its value: rule below |
| `NoexceptExpr.expr` | written | out | - | the operand as written; `value` is the fact |
| `NoexceptExpr.value` | value | in | enum NoexceptValue | read to choose the normal form |
| `Dynamic` | flag | in | tag 7 |  |
| `Dynamic.0` | type | in | type* | in stored order |
| `DependentNoexcept` | flag | in | tag 8 |  |
| `DependentNoexcept.0` | value | in | profile |  |
| `Deferred` | flag | demand | - | §11.2 |
| `Deferred.0` | entity | demand | - | §11.2 |

An exception specification is written in **normal form**: `None`;
`Nothrow` (for `Nothrow`, `NothrowAttr`, `DynamicNone`, `BasicNoexcept`
and a `NoexceptExpr` whose value is `True`); `None` for a `NoexceptExpr`
whose value is `False`; `DependentNoexcept` with the profile for a
`NoexceptExpr` whose value is `Dependent`; `Dynamic` with its types (kept,
though Kiln's canonical form from C++17 on is `None`: before C++17 the
types are behaviour).

### `kiln_ast::types::func::NoexceptValue`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `False` | value | out | - | normalised to `ExcSpec::None` |
| `True` | value | out | - | normalised to `ExcSpec::Nothrow` |
| `Dependent` | value | out | - | normalised to `ExcSpec::DependentNoexcept` |
| `Dependent.0` | value | in | profile |  |

### `kiln_ast::types::func::TypeList`

`kind=struct; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `start` | bookkeeping | out | - | range in `Store` `type_lists` |
| `len` | bookkeeping | out | - |  |

### 4.6 Names, qualifiers and template arguments

Nested-name-specifiers and template names are interned and have a
canonical form (`TypeContext::canonical_nns`,
`canonical_template_name`). `enum Nns` and `enum TemplateName` always write
the canonical form. Template argument lists are written from
`TypeContext::canonical_args`.

### `kiln_ast::types::nns::Nns`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Global` | name | in | tag 1 |  |
| `Namespace` | name | in | tag 2 |  |
| `Namespace.prefix` | written | out | - | a namespace's key already holds its path; canonical form has no prefix |
| `Namespace.ns` | entity | in | ref |  |
| `NamespaceAlias` | written | out | as Namespace | the alias a namespace was reached through |
| `NamespaceAlias.prefix` | written | out | - | as `Namespace.prefix` |
| `NamespaceAlias.alias` | written | out | - | the alias as written |
| `NamespaceAlias.ns` | entity | in | ref |  |
| `Type` | name | in | tag 4 |  |
| `Type.prefix` | name | in | enum Nns? | whatever the canonical form keeps |
| `Type.ty` | type | in | type |  |
| `Type.template_kw` | written | out | - | the `template` keyword as written |
| `Identifier` | name | in | tag 5 |  |
| `Identifier.prefix` | name | in | enum Nns? |  |
| `Identifier.name` | name | in | sym |  |
| `Splice` | name | unsupported | - | reflection (§12) |
| `Splice.0` | child | unsupported | - | reflection (§12) |

### `kiln_ast::types::nns::NnsKind`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Global` | flag | out | - | stored discriminant; `Nns` has a variant of the same name |
| `Namespace` | flag | out | - | stored discriminant; `Nns` has a variant of the same name |
| `NamespaceAlias` | flag | out | - | stored discriminant; `Nns` has a variant of the same name |
| `Type` | flag | out | - | stored discriminant; `Nns` has a variant of the same name |
| `Identifier` | flag | out | - | stored discriminant; `Nns` has a variant of the same name |
| `Splice` | flag | out | - | stored discriminant; `Nns` has a variant of the same name |

### `kiln_ast::types::names::TemplateName`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Template` | entity | in | tag 1 |  |
| `Template.0` | entity | in | ref |  |
| `Qualified` | written | out | - | sugar: canonical is the underlying name |
| `Qualified.nns` | written | out | - | sugar |
| `Qualified.template_kw` | written | out | - | sugar |
| `Qualified.underlying` | written | out | - | sugar |
| `DependentIdentifier` | entity | in | tag 3 |  |
| `DependentIdentifier.nns` | name | in | enum Nns |  |
| `DependentIdentifier.name` | name | in | sym |  |
| `DependentOperator` | name | unsupported | - | the operator is a raw `kiln_lex` token kind, for which Kiln exposes no stable numbering |
| `DependentOperator.nns` | name | unsupported | - | as `DependentOperator` |
| `DependentOperator.op` | name | unsupported | - | as `DependentOperator` |
| `TemplateTemplateParm` | written | out | - | sugar: canonical is `CanonTemplateTemplateParm` |
| `TemplateTemplateParm.decl` | written | out | - | sugar |
| `TemplateTemplateParm.depth` | written | out | - | sugar |
| `TemplateTemplateParm.index` | written | out | - | sugar |
| `TemplateTemplateParm.pack` | written | out | - | sugar |
| `CanonTemplateTemplateParm` | entity | in | tag 6 |  |
| `CanonTemplateTemplateParm.depth` | value | in | uv |  |
| `CanonTemplateTemplateParm.index` | value | in | uv |  |
| `CanonTemplateTemplateParm.pack` | flag | in | bool |  |
| `SubstTemplateTemplateParm` | written | out | - | sugar: canonical is the replacement |
| `SubstTemplateTemplateParm.replacement` | written | out | - | sugar |
| `SubstTemplateTemplateParm.associated` | written | out | - | sugar |
| `SubstTemplateTemplateParm.index` | written | out | - | sugar |
| `SubstTemplateTemplateParmPack` | entity | in | tag 8 |  |
| `SubstTemplateTemplateParmPack.associated` | entity | in | ref |  |
| `SubstTemplateTemplateParmPack.index` | value | in | uv |  |
| `SubstTemplateTemplateParmPack.args` | value | in | enum ArgView* |  |
| `Using` | written | out | - | sugar: the using-declaration a template was reached through |
| `Using.shadow` | written | out | - | sugar |
| `Using.underlying` | written | out | - | sugar |
| `Overloaded` | entity | in | tag 10 |  |
| `Overloaded.0` | entity | in | ref*sorted | the templates found at the definition (digest §8), read through `TypeContext::decl_set` |
| `Splice` | entity | unsupported | - | reflection (§12) |
| `Splice.expr` | child | unsupported | - | reflection (§12) |
| `Splice.profile` | value | unsupported | - | reflection (§12) |
| `SpliceProfile` | entity | unsupported | - | reflection (§12) |
| `SpliceProfile.0` | value | unsupported | - | reflection (§12) |

### `kiln_ast::types::names::DeclSet`

`kind=struct; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `start` | bookkeeping | out | - | range in `TypeContext` `name_decls` |
| `len` | bookkeeping | out | - |  |

### `kiln_ast::types::args::ArgView`

`kind=view; pin=none`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Null` | value | in | tag 1 |  |
| `Type` | value | in | tag 2 |  |
| `Type.0` | type | in | type |  |
| `Value` | value | in | tag 3 |  |
| `Value.ty` | type | in | type |  |
| `Value.value` | value | in | value |  |
| `Template` | value | in | tag 4 |  |
| `Template.0` | entity | in | enum TemplateName |  |
| `TemplateExpansion` | value | in | tag 5 |  |
| `TemplateExpansion.name` | entity | in | enum TemplateName |  |
| `TemplateExpansion.num_expansions` | value | in | uv? |  |
| `Expr` | written | out | - | sugar: the argument as written; a canonical list holds `Profile` |
| `Expr.expr` | written | out | - | sugar |
| `Expr.profile` | written | out | - | sugar |
| `Profile` | value | in | tag 7 |  |
| `Profile.0` | value | in | profile |  |
| `Pack` | value | in | tag 8 |  |
| `Pack.0` | value | in | enum ArgView* |  |

### `kiln_ast::types::args::ArgKind`

`kind=enum; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Null` | flag | out | - | stored discriminant of `TemplateArg`; `ArgView` has a variant of the same name |
| `Type` | flag | out | - | stored discriminant of `TemplateArg`; `ArgView` has a variant of the same name |
| `Value` | flag | out | - | stored discriminant of `TemplateArg`; `ArgView` has a variant of the same name |
| `Template` | flag | out | - | stored discriminant of `TemplateArg`; `ArgView` has a variant of the same name |
| `TemplateExpansion` | flag | out | - | stored discriminant of `TemplateArg`; `ArgView` has a variant of the same name |
| `Expr` | flag | out | - | stored discriminant of `TemplateArg`; `ArgView` has a variant of the same name |
| `Profile` | flag | out | - | stored discriminant of `TemplateArg`; `ArgView` has a variant of the same name |
| `Pack` | flag | out | - | stored discriminant of `TemplateArg`; `ArgView` has a variant of the same name |

### `kiln_ast::types::args::TemplateArg`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `kind` | flag | out | - | stored form; the walk reads `ArgView` |
| `a` | bookkeeping | out | - | per-kind word; decoded by `ArgView` |
| `b` | bookkeeping | out | - | per-kind word; decoded by `ArgView` |

### `kiln_ast::types::args::ListEntry`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `start` | bookkeeping | out | - | range in `ArgTable` `args` |
| `len` | bookkeeping | out | - |  |
| `canon` | bookkeeping | out | - | the canonical list; used through `canonical_args` |
| `flags` | bookkeeping | out | - | `TypeFlags` of the list |

### `kiln_ast::types::args::ArgTable`

`kind=root; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `args` | bookkeeping | out | - | arena: reached through `ArgListId` |
| `lists` | bookkeeping | out | - |  |
| `intern.absent` | bookkeeping | out | - |  |
| `intern` | bookkeeping | out | - |  |

### `kiln_ast::types::intern::KeyedEntry`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `key` | bookkeeping | out | - | four packed words; decoded by `Nns` or `TemplateName` |
| `canon` | bookkeeping | out | - | the canonical entry; used through `canonical_nns` and `canonical_template_name` |
| `flags` | bookkeeping | out | - | `TypeFlags` of the entry |

### `kiln_ast::types::intern::KeyedTable`

`kind=root; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `entries` | bookkeeping | out | - | arena: reached through `NnsId` and `TemplateNameId` |
| `intern.absent` | bookkeeping | out | - |  |
| `intern` | bookkeeping | out | - |  |

### 4.7 The type arena and the target

Nothing below is written. The options and the target description are
properties of the command line, which is the same for the two parses a
verification compares (`verify.md`); "Open questions" asks whether they
should be in a unit all the same.

### `kiln_ast::types::node::Store`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `nodes` | bookkeeping | out | - | arena: reached through `TypeId` |
| `canon` | bookkeeping | out | - | canonical links; used through `canonical` |
| `protos` | bookkeeping | out | - | arena: reached through `TypeView::Function` |
| `type_lists` | bookkeeping | out | - |  |
| `pairs` | bookkeeping | out | - |  |
| `triples` | bookkeeping | out | - |  |
| `arrays` | bookkeeping | out | - |  |

### `kiln_ast::types::context::TypeContext`

`kind=root; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `store` | bookkeeping | out | - | arena |
| `intern.absent` | bookkeeping | out | - |  |
| `intern` | bookkeeping | out | - |  |
| `names` | bookkeeping | out | - | arena of template names |
| `name_decls` | bookkeeping | out | - | storage of `DeclSet` |
| `dependent_templates` | bookkeeping | out | - | derived from each template's context |
| `first_dtst_names` | written | out | - | first spelling of a deduced class template specialisation, for printing |
| `dtst_first_spelling` | written | out | - | printing option |
| `exc_protos` | bookkeeping | out | - | storage behind `Ast::exception_spec_of` (§4.2 rule 3) |
| `arg_origins` | written | out | - | first spelling (§4.2 rule 1) |
| `expr_origins` | written | out | - | first spelling (§4.2 rule 1) |
| `alias_converted` | written | out | - | converted arguments of alias specialisations, which are sugar (§4.2 rule 2) |
| `nns` | bookkeeping | out | - | arena of nested-name-specifiers |
| `args` | bookkeeping | out | - | arena of template arguments |
| `profiles` | bookkeeping | out | - | arena of profiles |
| `profile_scratch` | bookkeeping | out | - |  |
| `target` | flag | out | - | command line |
| `opts` | flag | out | - | command line |
| `exhausted` | bookkeeping | out | - | the arena overflowed; the parse has failed |
| `vla_star_bound` | bookkeeping | out | - | the shared bound expression of `[*]`, a C construct |
| `param_abis` | bookkeeping | out | - | storage of `FuncProto` `abis` |

### `kiln_ast::types::context::TypeOptions`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `noexcept_in_type` | flag | out | - | language mode, from the command line; §4.2 rule 3 makes the digest independent of it |

### `kiln_ast::types::layout::TargetTypes`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `size_align` | value | out | - | target, from the command line |
| `char_signed` | flag | out | - | target or `-fsigned-char`, from the command line |
| `wchar_signed` | flag | out | - | target, from the command line |
| `long_double` | flag | out | - | target, from the command line |
| `size_t` | type | out | - | target, from the command line |
| `ptrdiff_t` | type | out | - | target, from the command line |
| `intptr_t` | type | out | - | target, from the command line |
| `pointer_bytes` | value | out | - | target, from the command line |
| `max_atomic_inline_bytes` | value | out | - | target, from the command line |

### `kiln_ast::types::layout::FloatFormat`

`kind=enum; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `IeeeHalf` | flag | out | - | only in `TargetTypes`; a floating constant's format is `FloatKind` (§8) |
| `BFloat` | flag | out | - | only in `TargetTypes`; a floating constant's format is `FloatKind` (§8) |
| `IeeeSingle` | flag | out | - | only in `TargetTypes`; a floating constant's format is `FloatKind` (§8) |
| `IeeeDouble` | flag | out | - | only in `TargetTypes`; a floating constant's format is `FloatKind` (§8) |
| `X87Extended` | flag | out | - | only in `TargetTypes`; a floating constant's format is `FloatKind` (§8) |
| `IeeeQuad` | flag | out | - | only in `TargetTypes`; a floating constant's format is `FloatKind` (§8) |
| `PpcDoubleDouble` | flag | out | - | only in `TargetTypes`; a floating constant's format is `FloatKind` (§8) |

### `kiln_ast::types::layout::RecordLayout`

`kind=struct; pin=schema`

Layouts are computed on demand and are not digested (digest §2.3, §6):
they are functions of the class units.

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `size` | value | demand | - | §11 |
| `align` | value | demand | - | §11 |
| `preferred_align` | value | demand | - | §11 |
| `data_size` | value | demand | - | §11 |
| `nv_size` | value | demand | - | §11 |
| `nv_align` | value | demand | - | §11 |
| `unadjusted_align` | value | demand | - | §11 |
| `unadjusted_field_align` | value | demand | - | §11 |
| `fields` | value | demand | - | §11 |
| `bases` | value | demand | - | §11 |
| `vbases` | value | demand | - | §11 |
| `vptr` | value | demand | - | §11 |
| `primary_base` | value | demand | - | §11 |

### `kiln_ast::types::layout::LayoutCache`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `map` | bookkeeping | demand | - | §11: filled when a layout is asked for |

## 5. Declarations

### 5.1 A declaration's stream

A declaration, whether a unit (digest §2) or walked in place (§2.4), is
written as:

1. the rows of `DeclNode`, which bring in the payload record through the
   `data` row of the declaration's kind;
2. the rows of `ColdDeclData`;
3. the row `decl_attrs` of `AttrArena` (§10.1) and the rows of `AnnotArena`
   (§10.5);
4. the `in` rows of `TemplateTables` whose map is keyed by `DeclId`
   (§9.5).

For a unit the entity is read as §2.3 says. A row encoded `keyed` writes
nothing here.

### 5.2 Which declarations are units

"unit" means a declared unit, or a derived one when the entity is implicit,
instantiated or a builtin (digest §2.2). "local" means §2.4 applies inside
a function body, default argument, initialiser or lambda; a kind that is
both is a unit at namespace or class scope. "parent" means the declaration
is written inside another declaration's stream. This table is not parsed
against Kiln.

| DeclKind | Is | KeyKind | Name; discriminator |
|---|---|---|---|
| `TranslationUnit` | nothing | | it is `Parent::root` |
| `Namespace` | unit | `namespace` | identifier, or `anonymous` |
| `NamespaceAlias` | unit, local | `namespace_alias` | identifier |
| `LinkageSpec`, `Export` | nothing | | transparent: what they say is a fact of each declaration inside |
| `Empty`, `AccessSpec` | nothing | | |
| `FileScopeAsm` | unit | `file_scope_asm` | `content` |
| `UsingDirective` | unit, local | `using_directive` | `content` |
| `Using` | unit, local | `using_declaration` | the name; the key of the scope the qualifier names |
| `UsingEnum` | unit, local | `using_enum` | `content` |
| `UsingShadow`, `ConstructorUsingShadow` | unit, local | `using_shadow` | the name; the key of the target |
| `UnresolvedUsingValue`, `UnresolvedUsingTypename` | unit | `using_declaration` | the name; the qualifier by structure |
| `UnresolvedUsingIfExists` | unit | `unresolved_if_exists` | the name |
| `Typedef`, `TypeAlias` | unit, local | `alias` | identifier |
| `Enum` | unit, local | `enumeration` | identifier, or digest §3.3 |
| `EnumConstant` | unit | `enumerator` | identifier; the parent is the enumeration |
| `CXXRecord` | unit, local | `class` | identifier, digest §3.3, or `closure` |
| `ClassTemplateSpecialization`, `VarTemplateSpecialization` | unit | `specialisation` | the template's name; the canonical arguments; the parent is the template |
| `ClassTemplatePartialSpecialization`, `VarTemplatePartialSpecialization` | unit | `partial_specialisation` | the template's name; head and argument pattern; the parent is the template |
| `Field` | unit | `field` | identifier; unnamed: `unnamed_by` the name of its anonymous class, else `unnamed_ordinal` among the class's unnamed fields |
| `IndirectField` | unit | `field` | identifier |
| `Friend` | unit | `friend` | `content`; the parent is the class |
| `Function`, `CXXMethod` | unit | `function` | identifier, `operator` or `literal_operator`; the signature |
| `CXXConstructor`, `CXXDestructor`, `CXXConversion` | unit | `function` | `constructor`, `destructor`, `conversion`; the signature |
| `CXXDeductionGuide` | unit | `deduction_guide` | `deduction_guide`; the template and the signature |
| `Var` | unit, local | `variable`, or `decomposition` | identifier; a decomposition is named by its first binding |
| `ParmVar` | local | | an ordinal, or `Ref::param` inside a signature |
| `Binding` | unit, local | `binding` | identifier |
| `ClassTemplate`, `FunctionTemplate`, `VarTemplate`, `TypeAliasTemplate`, `Concept` | unit | `class_template`, `function_template`, `variable_template`, `alias_template`, `concept` | identifier (or the function's name); §3.3 |
| `BuiltinTemplate` | unit | `class_template` | identifier; the parent is `Parent::builtin` |
| `TemplateTypeParm`, `NonTypeTemplateParm`, `TemplateTemplateParm` | parent | | written in the template head; referred to as `Ref::template_param` |
| `RequiresExprBody` | nothing | | the requires-expression lists its parameters |
| `LifetimeExtendedTemporary` | parent | | written at its `MaterializeTemporaryExpr` |
| `TemplateParamObject` | unit | `template_param_object` | `content` |
| `StaticAssert` | unit, local | `static_assert` | `content` |
| `ConstevalBlock` | unit | `consteval_block` | `content` |
| `Label`, `ContractResultName` | local | | an ordinal |
| `UsingPack`, `FriendTemplate`, `ImplicitParam`, `ImplicitConceptSpecialization` | unsupported | | no construction site found in Kiln |

Further rules:

- A declaration whose `TemplLink` kind is `Pattern` is not a unit: it is
  the pattern of its template and is written in the template's unit (§9).
  One whose kind is `Specialization` is a `specialisation` unit whatever
  its `DeclKind`; one whose kind is `MemberOfSpecialization` has its
  ordinary key under the specialisation's key.
- A block-scope function or `extern` variable declaration belongs to the
  entity's unit at namespace scope (§2.4).
- An implicit special member, an implicit comparison or an implicit
  deduction guide is a unit only if it has a body or some unit's stream
  holds a `Ref` to it; the walk finds that out from the streams it writes,
  not from `DeclFlags` (`0-used`, `0-referenced`). The same holds for a
  lazily declared builtin and for a `TemplateParamObject`.
- A unit under `Parent::c_linkage` is a function or variable whose first
  declaration is lexically inside a `LinkageSpec` of language `C`
  (`ScopeData`, `aux-lang`), and that is not a class member.
- An injected-class-name is not a unit and a `Ref` never names one.

### 5.3 Headers

### `kiln_ast::decl::DeclKind`

`kind=enum; pin=schema; base=256`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `TranslationUnit` | bookkeeping | out | - | the root scope: `Parent::root` |
| `Namespace` | flag | in | tag 258 |  |
| `NamespaceAlias` | flag | in | tag 259 |  |
| `LinkageSpec` | flag | out | - | transparent: the language is written by each declaration inside (`ScopeData`, `aux-lang`) |
| `Export` | flag | out | - | transparent: module ownership is written by each declaration inside (`DeclMisc`, `0-module-ownership`) |
| `Empty` | written | out | - | declares nothing |
| `FileScopeAsm` | flag | in | tag 263 |  |
| `UsingDirective` | flag | in | tag 264 |  |
| `Using` | flag | in | tag 265 |  |
| `UsingEnum` | flag | in | tag 266 |  |
| `UsingShadow` | flag | in | tag 267 |  |
| `ConstructorUsingShadow` | flag | in | tag 268 |  |
| `UsingPack` | entity | unsupported | unbuilt | no construction site found in Kiln |
| `UnresolvedUsingValue` | flag | in | tag 270 |  |
| `UnresolvedUsingTypename` | flag | in | tag 271 |  |
| `UnresolvedUsingIfExists` | flag | in | tag 272 |  |
| `Typedef` | flag | in | tag 273 |  |
| `TypeAlias` | written | out | as Typedef | `using` against `typedef` |
| `Enum` | flag | in | tag 275 |  |
| `EnumConstant` | flag | in | tag 276 |  |
| `CXXRecord` | flag | in | tag 277 |  |
| `ClassTemplateSpecialization` | flag | in | tag 278 |  |
| `ClassTemplatePartialSpecialization` | flag | in | tag 279 |  |
| `Field` | flag | in | tag 280 |  |
| `IndirectField` | flag | in | tag 281 |  |
| `AccessSpec` | written | out | - | access is written by each member (`DeclMisc`, `0-access`) |
| `Friend` | flag | in | tag 283 |  |
| `FriendTemplate` | entity | unsupported | unbuilt | no construction site found in Kiln |
| `Function` | flag | in | tag 285 |  |
| `CXXMethod` | flag | in | tag 286 |  |
| `CXXConstructor` | flag | in | tag 287 |  |
| `CXXDestructor` | flag | in | tag 288 |  |
| `CXXConversion` | flag | in | tag 289 |  |
| `CXXDeductionGuide` | flag | in | tag 290 |  |
| `Var` | flag | in | tag 291 |  |
| `ParmVar` | flag | in | tag 292 |  |
| `ImplicitParam` | entity | unsupported | unbuilt | no construction site found in Kiln |
| `Binding` | flag | in | tag 294 |  |
| `VarTemplateSpecialization` | flag | in | tag 295 |  |
| `VarTemplatePartialSpecialization` | flag | in | tag 296 |  |
| `ClassTemplate` | flag | in | tag 297 |  |
| `FunctionTemplate` | flag | in | tag 298 |  |
| `VarTemplate` | flag | in | tag 299 |  |
| `TypeAliasTemplate` | flag | in | tag 300 |  |
| `Concept` | flag | in | tag 301 |  |
| `BuiltinTemplate` | flag | in | tag 302 |  |
| `TemplateTypeParm` | flag | in | tag 303 |  |
| `NonTypeTemplateParm` | flag | in | tag 304 |  |
| `TemplateTemplateParm` | flag | in | tag 305 |  |
| `ImplicitConceptSpecialization` | entity | unsupported | unbuilt | no construction site found in Kiln |
| `RequiresExprBody` | bookkeeping | out | - | the scope of a requires-expression's parameters, which the expression lists |
| `LifetimeExtendedTemporary` | flag | in | tag 308 |  |
| `TemplateParamObject` | flag | in | tag 309 |  |
| `StaticAssert` | flag | in | tag 310 |  |
| `Label` | flag | in | tag 311 |  |
| `ContractResultName` | flag | in | tag 312 |  |
| `ConstevalBlock` | flag | in | tag 313 |  |

### `kiln_ast::decl::DeclNode`

`kind=struct; pin=schema; selector=kiln_ast::decl::DeclKind`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `kind` | flag | in | enum DeclKind | the node's tag |
| `flags` | flag | in | record DeclFlags |  |
| `misc` | flag | in | record DeclMisc |  |
| `name` | name | in | keyed | a local entity's name is not digested (digest §4.3) |
| `ctx` | entity | in | keyed | the semantic context: the key's parent |
| `next` | bookkeeping | out | - | the next member of the context |
| `loc` | location | out | - | also chooses the unit's group (digest §11) |
| `ty-type` | type | in | type? | the declared type; a function's is written without its exception specification |
| `ty-exception-spec` | value | in | enum ExcSpec? | functions only, in normal form, from `TypeContext::proto` and `exc_spec`; absent for a function of the deferrable class (§11.2) |
| `NamespaceAlias.data` | entity | in | ref | the namespace named, through any alias |
| `FileScopeAsm.data` | value | in | child | the string literal |
| `UsingDirective.data` | entity | in | ref | the nominated namespace |
| `Using.data` | name | in | keyed | the qualifier: the scope it names is the discriminator |
| `UsingEnum.data` | entity | in | ref |  |
| `UsingShadow.data` | entity | in | keyed | the target: the discriminator |
| `ConstructorUsingShadow.data` | entity | in | keyed | the target constructor: the discriminator |
| `UnresolvedUsingValue.data` | name | in | keyed | the qualifier, in the discriminator as `enum Nns` |
| `UnresolvedUsingTypename.data` | name | in | keyed | as `UnresolvedUsingValue.data` |
| `Typedef.data` | type | in | record TypedefData |  |
| `TypeAlias.data` | type | in | record TypedefData |  |
| `Enum.data` | type | in | record EnumData |  |
| `EnumConstant.data` | value | in | record EnumeratorData |  |
| `CXXRecord.data` | type | in | record RecordData |  |
| `ClassTemplateSpecialization.data` | type | in | record RecordData |  |
| `ClassTemplatePartialSpecialization.data` | type | in | record RecordData |  |
| `Field.data` | entity | in | record FieldData |  |
| `IndirectField.data` | entity | in | ref* | the chain of fields from the class to the member |
| `Friend.data` | entity | in | enum FriendTarget | read with `ty` through `DeclArena::friend_target` |
| `Function.data` | entity | in | record FunctionData |  |
| `CXXMethod.data` | entity | in | record FunctionData |  |
| `CXXConstructor.data` | entity | in | record FunctionData |  |
| `CXXDestructor.data` | entity | in | record FunctionData |  |
| `CXXConversion.data` | entity | in | record FunctionData |  |
| `CXXDeductionGuide.data` | entity | in | record FunctionData |  |
| `Var.data` | entity | in | record VarData |  |
| `ParmVar.data` | entity | in | record ParamData |  |
| `VarTemplateSpecialization.data` | entity | in | record VarData |  |
| `VarTemplatePartialSpecialization.data` | entity | in | record VarData |  |
| `ClassTemplate.data` | entity | in | record TemplateInfo | index into `TemplateTables` `infos` |
| `FunctionTemplate.data` | entity | in | record TemplateInfo |  |
| `VarTemplate.data` | entity | in | record TemplateInfo |  |
| `TypeAliasTemplate.data` | entity | in | record TemplateInfo |  |
| `Concept.data` | entity | in | record TemplateInfo |  |
| `BuiltinTemplate.data` | flag | out | - | which builtin template: its name, in the key, says the same |
| `TemplateTypeParm.data` | entity | in | record TemplateParmInfo | index into `TemplateTables` `parms` |
| `NonTypeTemplateParm.data` | entity | in | record TemplateParmInfo |  |
| `TemplateTemplateParm.data` | entity | in | record TemplateParmInfo |  |
| `Namespace.data` | entity | in | record NamespaceData |  |
| `TranslationUnit.data` | bookkeeping | out | - | `ScopeData`; the root is not written |
| `LinkageSpec.data` | flag | out | - | `ScopeData`, read by the declarations inside |
| `Export.data` | bookkeeping | out | - | `ScopeData` |
| `RequiresExprBody.data` | bookkeeping | out | - | `ScopeData` |
| `StaticAssert.data` | value | in | record StaticAssertData |  |
| `ConstevalBlock.data` | value | in | record StaticAssertData |  |
| `LifetimeExtendedTemporary.data` | entity | in | record LetData |  |

`Binding`, `Label`, `ContractResultName`, `AccessSpec`, `Empty`,
`UnresolvedUsingIfExists` and `TemplateParamObject` store 0 in `data`. A
`TemplateParamObject`'s value is read with
`TemplateTables::param_object_value` (§9).

### `kiln_ast::decl::DeclFlags`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `0-invalid` | flag | unsupported | - | an invalid declaration: digest §10 |
| `0-implicit` | flag | in | bit |  |
| `0-used` | bookkeeping | demand | - | §11 |
| `0-referenced` | bookkeeping | demand | - | §11 |
| `0-has-attrs` | bookkeeping | out | - |  |
| `0-has-lexical-ctx` | bookkeeping | out | - |  |
| `0-dependent-ctx` | bookkeeping | out | - | follows from the context |
| `0-injected` | flag | in | bit | injected by `define_aggregate` |

### `kiln_ast::decl::DeclMisc`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `0-access` | flag | in | enum Access | bits 0-1 |
| `0-friend` | flag | in | enum FriendKind | bits 2-3; `Undeclared` only if every declaration of the entity is: the entity is then found by argument-dependent lookup alone |
| `0-local-extern` | flag | in | bit | set only if every declaration of the entity is at block scope |
| `0-module-ownership` | flag | in | enum ModuleOwnership | bits 5-7, from the first declaration |
| `0-name-independent` | flag | in | bit |  |
| `0-linkage-cache` | bookkeeping | demand | - | §11 |
| `0-visibility-cache` | bookkeeping | demand | - | §11 |
| `0-first` | bookkeeping | out | - | position in the redeclaration chain |
| `0-definition` | flag | in | bit | set if any declaration of the entity has it |

### `kiln_ast::decl::Access`

`kind=enum; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `None` | flag | in | tag 1 |  |
| `Public` | flag | in | tag 2 |  |
| `Protected` | flag | in | tag 3 |  |
| `Private` | flag | in | tag 4 |  |

### `kiln_ast::decl::FriendKind`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `None` | flag | in | tag 1 |  |
| `Declared` | written | out | as None | a friend declaration of an entity declared elsewhere: the befriending is in the class's `Friend` unit |
| `Undeclared` | flag | in | tag 3 |  |

### `kiln_ast::decl::ModuleOwnership`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Unowned` | flag | in | tag 1 |  |
| `Visible` | flag | in | tag 2 |  |
| `VisibleWhenImported` | flag | in | tag 3 |  |
| `ReachableWhenImported` | flag | in | tag 4 |  |
| `ModulePrivate` | flag | in | tag 5 |  |

### `kiln_ast::decl::PayloadClass`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `None` | bookkeeping | out | - | which payload record a kind has; the `data` rows of `DeclNode` say it kind by kind |
| `Function` | bookkeeping | out | - | which payload record a kind has; the `data` rows of `DeclNode` say it kind by kind |
| `Var` | bookkeeping | out | - | which payload record a kind has; the `data` rows of `DeclNode` say it kind by kind |
| `Record` | bookkeeping | out | - | which payload record a kind has; the `data` rows of `DeclNode` say it kind by kind |
| `Field` | bookkeeping | out | - | which payload record a kind has; the `data` rows of `DeclNode` say it kind by kind |
| `Enum` | bookkeeping | out | - | which payload record a kind has; the `data` rows of `DeclNode` say it kind by kind |
| `Enumerator` | bookkeeping | out | - | which payload record a kind has; the `data` rows of `DeclNode` say it kind by kind |
| `Param` | bookkeeping | out | - | which payload record a kind has; the `data` rows of `DeclNode` say it kind by kind |
| `Typedef` | bookkeeping | out | - | which payload record a kind has; the `data` rows of `DeclNode` say it kind by kind |
| `Namespace` | bookkeeping | out | - | which payload record a kind has; the `data` rows of `DeclNode` say it kind by kind |
| `Scope` | bookkeeping | out | - | which payload record a kind has; the `data` rows of `DeclNode` say it kind by kind |
| `StaticAssert` | bookkeeping | out | - | which payload record a kind has; the `data` rows of `DeclNode` say it kind by kind |
| `Let` | bookkeeping | out | - | which payload record a kind has; the `data` rows of `DeclNode` say it kind by kind |

### `kiln_ast::decl::Payload`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `None` | bookkeeping | out | - | a borrowed payload record; the `data` rows of `DeclNode` say which |
| `None.0` | bookkeeping | out | - |  |
| `Function` | bookkeeping | out | - | a borrowed payload record; the `data` rows of `DeclNode` say which |
| `Function.0` | bookkeeping | out | - |  |
| `Var` | bookkeeping | out | - | a borrowed payload record; the `data` rows of `DeclNode` say which |
| `Var.0` | bookkeeping | out | - |  |
| `Record` | bookkeeping | out | - | a borrowed payload record; the `data` rows of `DeclNode` say which |
| `Record.0` | bookkeeping | out | - |  |
| `Field` | bookkeeping | out | - | a borrowed payload record; the `data` rows of `DeclNode` say which |
| `Field.0` | bookkeeping | out | - |  |
| `Enum` | bookkeeping | out | - | a borrowed payload record; the `data` rows of `DeclNode` say which |
| `Enum.0` | bookkeeping | out | - |  |
| `Enumerator` | bookkeeping | out | - | a borrowed payload record; the `data` rows of `DeclNode` say which |
| `Enumerator.0` | bookkeeping | out | - |  |
| `Param` | bookkeeping | out | - | a borrowed payload record; the `data` rows of `DeclNode` say which |
| `Param.0` | bookkeeping | out | - |  |
| `Typedef` | bookkeeping | out | - | a borrowed payload record; the `data` rows of `DeclNode` say which |
| `Typedef.0` | bookkeeping | out | - |  |
| `Namespace` | bookkeeping | out | - | a borrowed payload record; the `data` rows of `DeclNode` say which |
| `Namespace.0` | bookkeeping | out | - |  |
| `Scope` | bookkeeping | out | - | a borrowed payload record; the `data` rows of `DeclNode` say which |
| `Scope.0` | bookkeeping | out | - |  |
| `StaticAssert` | bookkeeping | out | - | a borrowed payload record; the `data` rows of `DeclNode` say which |
| `StaticAssert.0` | bookkeeping | out | - |  |
| `Let` | bookkeeping | out | - | a borrowed payload record; the `data` rows of `DeclNode` say which |
| `Let.0` | bookkeeping | out | - |  |

### `kiln_ast::decl::Redecl`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `prev` | bookkeeping | out | - | the redeclaration chain; §2.3 says what is merged over it |
| `group` | bookkeeping | out | - |  |

### `kiln_ast::decl::Members`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `first` | bookkeeping | out | - | the member chain; the rows that hold a `Members` say what is listed |
| `last` | bookkeeping | out | - |  |

### 5.4 Functions and variables

### `kiln_ast::decl::FunctionData`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `redecl` | bookkeeping | out | - |  |
| `params` | entity | in | decl* | the parameters, in order |
| `body` | child | in | child? |  |
| `spec` | flag | in | record FnSpec |  |
| `templ` | entity | in | record TemplLink |  |

A function's stream also holds its language linkage (`ScopeData`,
`aux-lang`).

### `kiln_ast::decl::FnSpec`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `0-storage` | flag | in | enum FnStorage | bits 0-1 |
| `0-inline` | flag | in | bit |  |
| `0-implicit-inline` | flag | in | bit | kept apart from `0-inline`: only a written `inline` is a hint to the optimiser |
| `0-constexpr-kind` | flag | in | enum ConstexprKind | bits 4-5 |
| `0-virtual` | flag | in | bit |  |
| `0-pure` | flag | in | bit |  |
| `0-defaulted` | flag | in | enum Defaulted | bits 8-9 |
| `0-deleted` | flag | in | bit |  |
| `0-explicit` | flag | in | bit |  |
| `0-trivial` | flag | in | bit |  |
| `0-immediate-escalating` | flag | in | bit |  |
| `0-has-contracts` | flag | in | bit |  |
| `0-is-coroutine` | flag | in | bit |  |
| `0-escalated` | flag | in | bit |  |
| `0-variadic` | flag | in | bit |  |
| `0-override` | written | out | - | `override` (digest §1 rule 2) |
| `0-final` | flag | in | bit |  |
| `0-explicit-object` | flag | in | bit |  |
| `0-virtual-written` | written | out | - | whether `virtual` was written; `0-virtual` is the fact |
| `0-storage-inherited` | written | out | - | whether the storage class was written here or inherited from the first declaration; `0-storage` is the fact |
| `0-not-selected` | flag | in | bit |  |
| `0-return-deduced` | flag | in | bit |  |

### `kiln_ast::decl::FnStorage`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `None` | flag | in | tag 1 |  |
| `Static` | flag | in | tag 2 |  |
| `Extern` | written | out | as None | `extern` on a function declaration changes nothing |

### `kiln_ast::decl::ConstexprKind`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `None` | flag | in | tag 1 |  |
| `Constexpr` | flag | in | tag 2 |  |
| `Consteval` | flag | in | tag 3 |  |

### `kiln_ast::decl::Defaulted`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `No` | flag | in | tag 1 |  |
| `Explicit` | flag | in | tag 2 |  |
| `Implicit` | flag | in | tag 3 |  |

### `kiln_ast::decl::ParamData`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `default_arg` | child | in | child? | §11.4 when the function is instantiated |
| `original_ty` | written | out | - | the type as written, before array and function types are adjusted; `DeclNode` `ty-type` is the fact |
| `index` | bookkeeping | out | - | the position in `FunctionData` `params` |
| `flags-explicit-object` | flag | in | bit |  |
| `flags-pack` | flag | in | bit |  |
| `flags-default-arg-pending` | bookkeeping | demand | - | §11.4: the default argument is not parsed or instantiated yet |
| `flags-static-array` | flag | in | bit |  |
| `flags-scope-depth` | value | in | uv | high byte: the function-prototype scope depth, the `level` of `Ref::param` |

### `kiln_ast::decl::VarData`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `redecl` | bookkeeping | out | - |  |
| `init` | child | in | child? |  |
| `spec` | flag | in | record VarSpec |  |
| `eval` | value | demand | - | §11.3: the `resolved` unit `value` |
| `templ` | entity | in | record TemplLink |  |

A variable's stream also holds its language linkage (`ScopeData`,
`aux-lang`).

### `kiln_ast::decl::VarSpec`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `0-storage` | flag | in | enum VarStorage | bits 0-2 |
| `0-tls` | flag | in | enum TlsKind | bits 3-4 |
| `0-init-style` | flag | in | enum InitStyle | bits 5-6; it selects the form of initialisation |
| `0-constexpr` | flag | in | bit |  |
| `0-constinit` | flag | in | bit |  |
| `0-inline` | flag | in | bit |  |
| `0-nrvo` | flag | in | bit |  |
| `0-exception-var` | flag | in | bit |  |
| `0-cxx-for-range` | flag | in | bit |  |
| `0-inline-implied` | flag | in | bit |  |

### `kiln_ast::decl::VarStorage`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `None` | flag | in | tag 1 |  |
| `Static` | flag | in | tag 2 |  |
| `Extern` | flag | in | tag 3 |  |
| `Register` | flag | in | tag 4 |  |

### `kiln_ast::decl::TlsKind`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `None` | flag | in | tag 1 |  |
| `Static` | flag | in | tag 2 |  |
| `Dynamic` | flag | in | tag 3 |  |

### `kiln_ast::decl::InitStyle`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `C` | flag | in | tag 1 |  |
| `Call` | flag | in | tag 2 |  |
| `List` | flag | in | tag 3 |  |
| `ParenList` | flag | in | tag 4 |  |

### `kiln_ast::decl::VarEval`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `0` | value | demand | - | §11.3 |

### `kiln_ast::decl::VarEvalState`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `NotEvaluated` | value | demand | - | §11.3 |
| `Evaluating` | value | demand | - | §11.3 |
| `NotConstant` | value | demand | - | §11.3 |
| `Value` | value | demand | - | §11.3 |
| `Value.0` | value | demand | - | §11.3: written in the `resolved` unit `value` as `value` |

### `kiln_ast::decl::LetData`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `extending` | entity | in | ref |  |
| `mangling_index` | value | in | uv | part of the temporary's symbol name |
| `value` | value | in | value? | "Open questions": whether the Oven fills it only on demand |

### 5.5 Classes and enumerations

### `kiln_ast::decl::RecordData`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `redecl` | bookkeeping | out | - |  |
| `members-fields` | entity | in | key* | the non-static data members (`Field`, `IndirectField`), in declaration order |
| `members-virtual` | entity | in | key* | the virtual member functions, in declaration order |
| `members-other` | entity | in | key*sorted | every other member that is a declared unit |
| `bases` | type | in | record BaseSpec* | in declaration order |
| `defn` | flag | in | record DefinitionBits |  |
| `templ` | entity | in | record TemplLink |  |
| `lambda` | entity | in | record LambdaData? | a closure type (§10.4) |
| `tag` | flag | in | enum TagKind |  |

A member is in exactly one of the three lists. Members that are not units
(§5.2) and members whose unit is derived are in none: the lists must not
depend on which implicit members and instantiations the translation unit
asked for. A local class (§2.4) writes its members in place instead, in
the same three groups, each member as `decl`.

### `kiln_ast::decl::TagKind`

`kind=enum; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Struct` | flag | in | tag 1 |  |
| `Class` | written | out | as Struct | `struct` against `class` (digest §1 rule 2) |
| `Union` | flag | in | tag 3 |  |

### `kiln_ast::decl::DefinitionBits`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `0-being-defined` | bookkeeping | out | - | state of the parse; clear once the class is complete |
| `0-complete` | flag | in | bit |  |
| `0-anonymous` | flag | in | bit |  |
| `0-lambda` | flag | in | bit |  |
| `0-aggregate` | flag | in | bit |  |
| `0-polymorphic` | flag | in | bit |  |
| `0-abstract` | flag | in | bit |  |
| `0-empty` | flag | in | bit |  |
| `0-standard-layout` | flag | in | bit |  |
| `0-trivially-copyable` | flag | in | bit |  |
| `0-trivial-default-ctor` | flag | in | bit |  |
| `0-trivial-copy-ctor` | flag | in | bit |  |
| `0-trivial-move-ctor` | flag | in | bit |  |
| `0-trivial-copy-assign` | flag | in | bit |  |
| `0-trivial-move-assign` | flag | in | bit |  |
| `0-trivial-dtor` | flag | in | bit |  |
| `0-literal` | flag | in | bit |  |
| `0-structural` | flag | in | bit |  |
| `0-user-declared-ctor` | flag | in | bit |  |
| `0-has-constexpr-ctor` | flag | in | bit |  |
| `0-has-variant-members` | flag | in | bit |  |
| `0-has-mutable-fields` | flag | in | bit |  |
| `0-has-virtual-bases` | flag | in | bit |  |
| `0-trivially-relocatable` | flag | in | bit |  |
| `0-user-declared-copy-ctor` | flag | in | bit |  |
| `0-user-declared-move-ctor` | flag | in | bit |  |
| `0-user-declared-copy-assign` | flag | in | bit |  |
| `0-user-declared-move-assign` | flag | in | bit |  |
| `0-user-declared-dtor` | flag | in | bit |  |
| `0-user-provided-default-ctor` | flag | in | bit |  |
| `0-has-in-class-init` | flag | in | bit |  |
| `0-has-inherited-ctor` | flag | in | bit |  |
| `0-has-inherited-assign` | flag | in | bit |  |
| `0-implicit-copy-ctor-const-param` | flag | in | bit |  |
| `0-implicit-copy-assign-const-param` | flag | in | bit |  |
| `0-defaulted-default-ctor-constexpr` | flag | in | bit |  |
| `0-needs-ovl-copy-ctor` | flag | in | bit |  |
| `0-needs-ovl-move-ctor` | flag | in | bit |  |
| `0-needs-ovl-copy-assign` | flag | in | bit |  |
| `0-needs-ovl-move-assign` | flag | in | bit |  |
| `0-needs-ovl-dtor` | flag | in | bit |  |
| `0-defaulted-default-ctor-deleted` | flag | in | bit | "known deleted": see "Open questions" on when Kiln sets it |
| `0-defaulted-copy-ctor-deleted` | flag | in | bit | "known deleted": see "Open questions" on when Kiln sets it |
| `0-defaulted-move-ctor-deleted` | flag | in | bit | "known deleted": see "Open questions" on when Kiln sets it |
| `0-defaulted-dtor-deleted` | flag | in | bit | "known deleted": see "Open questions" on when Kiln sets it |
| `0-has-uninit-ref-or-const` | flag | in | bit |  |
| `0-has-private-fields` | flag | in | bit |  |
| `0-has-protected-fields` | flag | in | bit |  |
| `0-pod` | flag | in | bit |  |
| `0-can-const-default-init` | flag | in | bit |  |
| `0-final` | flag | in | bit |  |
| `0-lambda-captures` | flag | in | bit |  |
| `0-has-flexible-array-member` | flag | in | bit |  |
| `0-trivial-abi` | flag | in | bit |  |
| `0-copy-ctor-for-call` | flag | in | bit |  |
| `0-move-ctor-for-call` | flag | in | bit |  |
| `0-dtor-for-call` | flag | in | bit |  |
| `0-nontrivial-default-ctor` | flag | in | bit |  |
| `0-nontrivial-copy-ctor` | flag | in | bit |  |
| `0-nontrivial-move-ctor` | flag | in | bit |  |
| `0-nontrivial-copy-assign` | flag | in | bit |  |
| `0-nontrivial-move-assign` | flag | in | bit |  |
| `0-complete-definition-required` | bookkeeping | demand | - | §11: set when something first needs the class complete; read by limited debug information only |

### `kiln_ast::decl::BaseSpec`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `ty` | type | in | type |  |
| `loc` | location | out | - |  |
| `is_virtual` | flag | in | bool |  |
| `access` | flag | in | enum Access | the effective access: a stored `None` is written as `Private` for a class declared `class` and `Public` otherwise |
| `pack_expansion` | flag | in | bool |  |

### `kiln_ast::decl::FieldData`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `bit_width` | child | in | child? |  |
| `init` | child | in | child? | the default member initialiser; §11.4 when the class is instantiated |
| `flags-mutable` | flag | in | bit |  |
| `flags-no-unique-address` | flag | in | bit |  |
| `flags-init-style` | flag | in | enum InitStyle | bits 2-3 |
| `flags-has-default-init` | bookkeeping | out | - | set before the initialiser is parsed; `init` present is the fact |

### `kiln_ast::decl::EnumData`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `redecl` | bookkeeping | out | - |  |
| `members` | entity | in | key* | the enumerators, in declaration order |
| `int_type` | type | in | type? |  |
| `promotion_type` | type | in | type? |  |
| `flags-scoped` | flag | in | bit |  |
| `flags-scoped-struct` | written | out | - | `enum struct` against `enum class` |
| `flags-fixed` | flag | in | bit |  |
| `flags-being-defined` | bookkeeping | out | - | state of the parse |
| `flags-complete` | flag | in | bit |  |
| `templ` | entity | in | record TemplLink |  |

A local enumeration (§2.4) writes its enumerators in place as `decl*`.

### `kiln_ast::decl::EnumeratorData`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `value` | value | in | value? |  |
| `init` | child | in | child? | the initialiser: what it names is part of what resolves |

### `kiln_ast::decl::TypedefData`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `redecl` | bookkeeping | out | - |  |
| `underlying` | type | in | type | canonical, like every type: the spelling of the aliased type is not digested |
| `templ` | entity | in | record TemplLink |  |

### `kiln_ast::decl::CtorInitKind`

`kind=enum; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Base` | flag | in | tag 1 |  |
| `VirtualBase` | flag | in | tag 2 |  |
| `Field` | flag | in | tag 3 |  |
| `IndirectField` | flag | in | tag 4 |  |
| `Delegating` | flag | in | tag 5 |  |

### `kiln_ast::decl::CtorInit`

`kind=struct; pin=schema; selector=kiln_ast::decl::CtorInitKind`

The list a constructor holds (`ColdDeclData` `ctor_inits`) is in
initialisation order.

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `kind` | flag | in | enum CtorInitKind |  |
| `Base.target` | type | in | type | a raw `TypeId` |
| `VirtualBase.target` | type | in | type | a raw `TypeId` |
| `Field.target` | entity | in | ref | a raw `DeclId` |
| `IndirectField.target` | entity | in | ref | a raw `DeclId` |
| `written_index` | written | out | - | the position as written, or "implicit"; the list order and `init` are the facts |
| `init` | child | in | child |  |
| `loc` | location | out | - |  |

### 5.6 Scopes and the rest

### `kiln_ast::decl::NamespaceData`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `redecl` | bookkeeping | out | - |  |
| `members` | entity | out | - | the members of a namespace are not listed (digest §5): each is a unit whose key names the namespace |
| `flags-inline` | flag | in | bit |  |
| `flags-anonymous` | name | in | keyed | the name `anonymous` |

### `kiln_ast::decl::ScopeData`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `members` | bookkeeping | out | - | a transparent scope: its members are members of the enclosing context |
| `aux-lang` | flag | in | enum LinkageLang | low byte, `LinkageSpec` only; written in the stream of each function and variable whose first declaration is lexically inside (rule below) |
| `aux-braceless` | written | out | - | bit 8: `extern "C" int x;` without braces; its effect, that the declaration is not a definition, is `DeclMisc` `0-definition` |

Every function and variable stream holds one `enum LinkageLang?`: the
language of the innermost `LinkageSpec` that lexically encloses the
entity's first declaration, through enclosing classes and functions;
absent when there is none. Whether the entity then has C language linkage
(a class member has not, nor has an entity with internal linkage) follows
from the rest of its stream; the key's parent uses the rule of §5.2.

### `kiln_ast::decl::LinkageLang`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `C` | flag | in | tag 1 |  |
| `Cxx` | flag | in | tag 2 |  |

### `kiln_ast::decl::StaticAssertData`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `cond` | child | in | child | for a `ConstevalBlock`, the call of the block |
| `message` | child | in | child? |  |

### `kiln_ast::decl::FriendTarget`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Decl` | entity | in | tag 1 |  |
| `Decl.0` | entity | in | ref |  |
| `Type` | entity | in | tag 2 |  |
| `Type.ty` | type | in | type |  |
| `Type.owned_tag` | written | out | - | the tag a `friend struct X;` itself declares; `ty` names it and the tag is a unit of its own |

### 5.7 Cold maps and the arena

`ColdMap` is Kiln's sparse side table; each field of `ColdDeclData` is one
map keyed by `DeclId`, and its row says what an entry's value is.

### `kiln_ast::slot::ColdMap`

`kind=root; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `name` | bookkeeping | out | - | the map's name, for the snapshot |
| `map` | bookkeeping | out | - | the entries: each field of type `ColdMap` has a row for them |

### `kiln_ast::decl::ColdDeclData`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `lexical_ctx` | written | out | - | where the declaration was written when that is not its semantic context; read only to find an enclosing `LinkageSpec` (`ScopeData` `aux-lang`) |
| `begin_loc` | location | out | - |  |
| `delete_message` | child | in | child? |  |
| `explicit_expr` | child | in | child? |  |
| `ctor_inits` | child | in | record CtorInit* |  |
| `overrides` | entity | demand | - | §11.5: the `resolved` unit `overrides` |
| `friends` | entity | in | key*sorted | the class's `Friend` units |
| `key_function` | entity | demand | - | §11.5 |
| `dtor_delete` | entity | demand | - | §11.5 |
| `dtor_delete_this` | child | demand | - | §11.5 |
| `record_pack` | value | in | uv? |  |
| `contracts` | child | in | child* | the function's contract assertions, in order |
| `asm_label` | name | in | string? |  |
| `alignment` | child | in | child? |  |
| `deprecation` | value | in | string? |  |
| `instantiated_from` | entity | in | ref | by the rule of §9 for a pattern; no site in Kiln fills this map (`TemplateTables` `instantiated_from` is the one in use) |
| `injection_point` | location | out | - |  |
| `declarator_end` | location | out | - |  |
| `builtin_id` | flag | demand | - | §11.6: the builtin is a derived unit keyed by its name under `Parent::builtin`; the id is Kiln's numbering and is not written |
| `gnu_lib_twin` | flag | in | bool | whether the declaration is folded as a library builtin; which one follows from its name |
| `lib_builtin` | flag | demand | - | §11.6 |
| `typedef_for_anon` | name | in | keyed | the name of an unnamed class or enumeration (digest §3.3) |
| `decomp_bindings` | entity | in | decl* | at namespace scope the bindings are units and are written as `key*` |
| `binding_decomp` | bookkeeping | out | - | inverse of `decomp_bindings` |
| `binding_expr` | child | in | child? |  |
| `binding_var` | entity | in | decl? | the holding variable of a tuple-like binding |
| `ctor_shadow_nominated` | entity | in | ref |  |
| `inheriting_ctor` | entity | demand | - | §11.5 |
| `inherited_ctor` | entity | demand | - | §11.5 |
| `hidden_shadow` | entity | in | ref |  |
| `conversion_spec_target` | written | out | - | the type a conversion function template specialisation's name prints with; the declaration keeps the pattern's name |
| `compound_literal` | entity | unsupported | - | the implicit variable of a file-scope compound literal has no name to key it by; a GNU extension in C++ |

### `kiln_ast::decl::DeclArena`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `nodes` | bookkeeping | out | - | arena: reached through `DeclId` |
| `functions` | bookkeeping | out | - | arena |
| `min_args` | bookkeeping | out | - | per function, the least number of arguments: follows from the parameters' default arguments |
| `vars` | bookkeeping | out | - | arena |
| `records` | bookkeeping | out | - | arena |
| `fields` | bookkeeping | out | - | arena |
| `enums` | bookkeeping | out | - | arena |
| `enumerators` | bookkeeping | out | - | arena |
| `params` | bookkeeping | out | - | arena |
| `typedefs` | bookkeeping | out | - | arena |
| `namespaces` | bookkeeping | out | - | arena |
| `scopes` | bookkeeping | out | - | arena |
| `static_asserts` | bookkeeping | out | - | arena |
| `base_specs` | bookkeeping | out | - | arena |
| `lets` | bookkeeping | out | - | arena |
| `ctor_inits` | bookkeeping | out | - | arena |
| `completed_records` | bookkeeping | demand | - | §11: the order in which classes were completed, instantiations included |
| `top_level_ends` | bookkeeping | out | - | where each top-level declaration ended, for incremental consumers |
| `fn_def_insts` | bookkeeping | demand | - | §11: function definitions in the order they were instantiated |
| `cold` | bookkeeping | out | - | the cold maps: `ColdDeclData` |
| `ty_journal` | bookkeeping | out | - | the log of in-place type rewrites |

### `kiln_ast::journal::Journal`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `on` | bookkeeping | out | - | a mutation log used to undo tentative work |
| `overflowed` | bookkeeping | out | - |  |
| `items` | bookkeeping | out | - |  |

### 5.8 Redeclaration and lookup tables

Neither is written. The redeclaration table is how §2.3 finds an entity's
first declaration and definition (`Ast::first`, `Ast::definition`,
`Ast::redecls_of`). The lookup tables are Kiln's index of what each scope
declares; what a lookup found is in the node that used it.

### `kiln_ast::redecl::RedeclGroup`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `first` | bookkeeping | out | - |  |
| `latest` | bookkeeping | out | - |  |
| `definition` | bookkeeping | out | - |  |

### `kiln_ast::redecl::RedeclTable`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `groups` | bookkeeping | out | - |  |
| `journal` | bookkeeping | out | - |  |

### `kiln_ast::lookup::LookupEntry`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `0` | bookkeeping | out | - | one declaration, or an index into `LookupTables` `sets` |

### `kiln_ast::lookup::LookupFlags`

`kind=struct; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `0` | bookkeeping | out | - | which kinds of declaration a lookup result holds |

### `kiln_ast::lookup::LookupTables`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `map` | bookkeeping | out | - |  |
| `sets` | bookkeeping | out | - |  |
| `set_words` | bookkeeping | out | - |  |
| `set_cap` | bookkeeping | out | - |  |
| `inline_children` | bookkeeping | out | - | restates `NamespaceData` `flags-inline` |
| `using_dirs` | bookkeeping | out | - | restates the `UsingDirective` units |
| `generation` | bookkeeping | out | - |  |
| `journal` | bookkeeping | out | - |  |

## 6. Statements

A statement is written as the rows of `StmtNode`: its tag, then the rows
of its kind, which are in walk order. A kind with no rows (`NullStmt`,
`BreakStmt`, `ContinueStmt`) is its tag alone.

### `kiln_ast::stmt::StmtKind`

`kind=enum; pin=schema; base=512`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `CompoundStmt` | flag | in | tag 513 |  |
| `DeclStmt` | flag | in | tag 514 |  |
| `ExprStmt` | flag | in | tag 515 |  |
| `NullStmt` | flag | in | tag 516 |  |
| `IfStmt` | flag | in | tag 517 |  |
| `SwitchStmt` | flag | in | tag 518 |  |
| `CaseStmt` | flag | in | tag 519 |  |
| `DefaultStmt` | flag | in | tag 520 |  |
| `BreakStmt` | flag | in | tag 521 |  |
| `ContinueStmt` | flag | in | tag 522 |  |
| `WhileStmt` | flag | in | tag 523 |  |
| `DoStmt` | flag | in | tag 524 |  |
| `ForStmt` | flag | in | tag 525 |  |
| `ReturnStmt` | flag | in | tag 526 |  |
| `LabelStmt` | flag | in | tag 527 |  |
| `GotoStmt` | flag | in | tag 528 |  |
| `IndirectGotoStmt` | flag | in | tag 529 |  |
| `AttributedStmt` | flag | in | tag 530 |  |
| `CXXForRangeStmt` | flag | in | tag 531 |  |
| `CXXExpansionStmt` | flag | in | tag 532 |  |
| `CXXTryStmt` | flag | in | tag 533 |  |
| `CXXCatchStmt` | flag | in | tag 534 |  |
| `CoroutineBodyStmt` | flag | in | tag 535 |  |
| `CoreturnStmt` | flag | in | tag 536 |  |
| `GCCAsmStmt` | flag | in | tag 537 |  |
| `ContractAssertStmt` | flag | in | tag 538 |  |

### `kiln_ast::stmt::StmtNode`

`kind=struct; pin=schema; selector=kiln_ast::stmt::StmtKind`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `kind` | flag | in | enum StmtKind | the node's tag |
| `flags` | flag | unsupported | unbuilt | Kiln never sets it; a non-zero value would be a meaning this file does not know |
| `loc` | location | out | - |  |
| `CompoundStmt.aux` | flag | in | record FpOverride | index into `StmtArena` `fp_overrides`: the floating-point pragmas in effect in the block |
| `CompoundStmt.a` | child | in | child* |  |
| `CompoundStmt.b` | location | out | - | the closing brace |
| `DeclStmt.a` | entity | in | decl* |  |
| `ExprStmt.a` | child | in | child |  |
| `IfStmt.aux` | flag | in | enum IfKind |  |
| `IfStmt.a` | child | in | record IfData |  |
| `SwitchStmt.a` | child | in | record SwitchData |  |
| `CaseStmt.a` | child | in | record CaseData |  |
| `DefaultStmt.a` | child | in | child |  |
| `WhileStmt.a` | child | in | record LoopData |  |
| `DoStmt.a` | child | in | record LoopData |  |
| `DoStmt.b` | location | out | - | the closing parenthesis |
| `ForStmt.a` | child | in | record LoopData |  |
| `ReturnStmt.a` | child | in | child? |  |
| `ReturnStmt.b` | entity | in | ref | the variable the return may elide the copy of |
| `LabelStmt.a` | entity | in | decl | the label |
| `LabelStmt.b` | child | in | child |  |
| `GotoStmt.a` | entity | in | ref |  |
| `IndirectGotoStmt.a` | child | in | child |  |
| `IndirectGotoStmt.b` | location | out | - | the `*` |
| `AttributedStmt.a` | flag | in | attrs |  |
| `AttributedStmt.b` | child | in | child |  |
| `CXXForRangeStmt.a` | child | in | record RangeForData |  |
| `CXXExpansionStmt.aux-form` | flag | in | enum ExpansionForm | bits 0-1 |
| `CXXExpansionStmt.aux-expanded` | flag | in | bit | bit 2 |
| `CXXExpansionStmt.b` | value | in | uv | the depth of the statement's template level |
| `CXXExpansionStmt.a` | child | in | record ExpansionData |  |
| `CXXTryStmt.a` | child | in | child |  |
| `CXXTryStmt.b` | child | in | child* | the handlers, in order |
| `CXXCatchStmt.a` | entity | in | decl? | the exception variable; absent for `catch (...)` |
| `CXXCatchStmt.b` | child | in | child |  |
| `CoroutineBodyStmt.a` | child | in | record CoroutineBodyData |  |
| `CoreturnStmt.aux-implicit` | flag | in | bit | bit 0 |
| `CoreturnStmt.a` | child | in | child? |  |
| `CoreturnStmt.b` | child | in | child? | the promise call |
| `GCCAsmStmt.aux-volatile` | flag | in | bit |  |
| `GCCAsmStmt.aux-goto` | flag | in | bit |  |
| `GCCAsmStmt.aux-inline` | flag | in | bit |  |
| `GCCAsmStmt.aux-basic` | flag | in | bit |  |
| `GCCAsmStmt.a` | child | in | record AsmData |  |
| `ContractAssertStmt.aux-kind` | flag | in | enum ContractKind | bits 0-1 |
| `ContractAssertStmt.aux-semantic` | value | in | uv | bits 2-3: the evaluation-semantic slot |
| `ContractAssertStmt.b` | entity | in | decl? | the result name of a postcondition |
| `ContractAssertStmt.a` | child | in | child | the predicate |

A `ContractAssertStmt` also writes the rows of its `ContractSite`
(`Ast` `contract_sites`), which are all symbolic.

### `kiln_ast::stmt::IfKind`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Ordinary` | flag | in | tag 1 |  |
| `Constexpr` | flag | in | tag 2 |  |
| `Consteval` | flag | in | tag 3 |  |
| `NotConsteval` | flag | in | tag 4 |  |

### `kiln_ast::stmt::IfData`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `init` | child | in | child? |  |
| `cond_var` | entity | in | decl? |  |
| `cond` | child | in | child? |  |
| `then` | child | in | child? | absent when discarded in an instantiation |
| `els` | child | in | child? |  |

### `kiln_ast::stmt::SwitchData`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `init` | child | in | child? |  |
| `cond_var` | entity | in | decl? |  |
| `cond` | child | in | child |  |
| `body` | child | in | child |  |
| `cases` | bookkeeping | out | - | the case and default statements of the switch, which the walk meets in `body` |

### `kiln_ast::stmt::CaseData`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `lhs` | child | in | child |  |
| `rhs` | child | in | child? |  |
| `sub` | child | in | child |  |

### `kiln_ast::stmt::LoopData`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `init` | child | in | child? |  |
| `cond_var` | entity | in | decl? |  |
| `cond` | child | in | child? |  |
| `inc` | child | in | child? |  |
| `body` | child | in | child? |  |

### `kiln_ast::stmt::RangeForData`

`kind=struct; pin=schema`

The implicit variables and the `cond` and `inc` expressions hold the
`begin`, `end`, `!=` and `++` functions the loop selected (digest §6).

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `init` | child | in | child? |  |
| `range` | entity | in | decl |  |
| `begin` | entity | in | decl? |  |
| `end` | entity | in | decl? |  |
| `cond` | child | in | child? |  |
| `inc` | child | in | child? |  |
| `loop_var` | entity | in | decl |  |
| `body` | child | in | child |  |
| `colon` | location | out | - |  |

### `kiln_ast::stmt::ExpansionForm`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Dependent` | flag | in | tag 1 |  |
| `Enumerating` | flag | in | tag 2 |  |
| `Iterating` | flag | in | tag 3 |  |
| `Destructuring` | flag | in | tag 4 |  |

### `kiln_ast::stmt::ExpansionData`

`kind=struct; pin=schema`

Both the pattern parts and, once expanded, the expansions are written.

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `init` | child | in | child? |  |
| `decl` | entity | in | decl |  |
| `exprs` | child | in | child* |  |
| `range_init` | child | in | child? |  |
| `body` | child | in | child |  |
| `prelude` | child | in | child* |  |
| `expansions` | child | in | child* |  |
| `colon` | location | out | - |  |

### `kiln_ast::stmt::CoroutineBodyData`

`kind=struct; pin=schema`

Everything but `body` is built by Kiln and holds the promise functions the
coroutine selected (digest §6).

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `body` | child | in | child |  |
| `promise` | entity | in | decl |  |
| `promise_stmt` | child | out | - | the statement declaring `promise`, which that row writes |
| `init_suspend` | child | in | child |  |
| `final_suspend` | child | in | child |  |
| `on_exception` | child | in | child? |  |
| `on_fallthrough` | child | in | child? |  |
| `allocate` | child | in | child |  |
| `deallocate` | child | in | child |  |
| `return_value` | child | in | child |  |
| `result_decl` | entity | in | decl? |  |
| `return_stmt` | child | in | child? |  |
| `return_on_alloc_failure` | child | in | child? |  |
| `param_moves` | child | in | child* |  |

### `kiln_ast::stmt::AsmOperand`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `name` | name | in | sym? | the template refers to the operand by it |
| `constraint` | value | in | string |  |
| `constraint_loc` | location | out | - |  |
| `expr` | child | in | child |  |

### `kiln_ast::stmt::AsmData`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `string` | value | in | string |  |
| `string_loc` | location | out | - |  |
| `operands` | child | in | record AsmOperand* | outputs, then inputs |
| `num_outputs` | value | in | uv |  |
| `num_inputs` | bookkeeping | out | - | the rest of `operands` |
| `clobbers` | value | in | string* |  |
| `labels` | entity | in | ref* |  |
| `rparen` | location | out | - |  |
| `srclocs` | location | out | - | source positions of the template's lines, as clang's `!srcloc` encodes them |

### `kiln_ast::stmt::ContractKind`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Assert` | flag | in | tag 1 |  |
| `Pre` | flag | in | tag 2 |  |
| `Post` | flag | in | tag 3 |  |

### `kiln_ast::stmt::ContractSemantic`

`kind=enum; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Ignore` | flag | out | - | only in `Ast` `contract_semantic`: the command line's default, which is not written (§10.9) |
| `Observe` | flag | out | - | only in `Ast` `contract_semantic`: the command line's default, which is not written (§10.9) |
| `Enforce` | flag | out | - | only in `Ast` `contract_semantic`: the command line's default, which is not written (§10.9) |
| `QuickEnforce` | flag | out | - | only in `Ast` `contract_semantic`: the command line's default, which is not written (§10.9) |

### `kiln_ast::stmt::ContractSite`

`kind=struct; pin=schema`

What a `contract_violation` object reports about an assertion. Every field
comes from the assertion's position or spelling.

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `file` | location | symbolic | - | the presumed file name |
| `function` | name | symbolic | - | the enclosing function's `__PRETTY_FUNCTION__` |
| `comment` | written | symbolic | - | the predicate's source text |
| `line` | location | symbolic | - | the presumed line of the keyword |
| `column` | location | symbolic | - | the column of the keyword |

### `kiln_ast::stmt::StmtArena`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `nodes` | bookkeeping | out | - | arena: reached through `StmtId` |
| `ifs` | bookkeeping | out | - | arena |
| `switches` | bookkeeping | out | - | arena |
| `cases` | bookkeeping | out | - | arena |
| `loops` | bookkeeping | out | - | arena |
| `range_fors` | bookkeeping | out | - | arena |
| `expansions` | bookkeeping | out | - | arena |
| `asms` | bookkeeping | out | - | arena |
| `asm_operands` | bookkeeping | out | - | arena |
| `coros` | bookkeeping | out | - | arena |
| `fp_overrides` | bookkeeping | out | - | arena: reached through `CompoundStmt.aux` |

### 6.1 Floating-point state

### `kiln_ast::fp::FpOverride`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `mask` | flag | in | uv | which fields of `vals` a pragma set; the bits are Kiln's `fp_bit` constants ("Open questions") |
| `vals` | flag | in | record FpOptions |  |
| `cx_limited` | flag | in | bool? |  |
| `strict_fn` | flag | in | bool |  |

### `kiln_ast::fp::FpOptions`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `contract` | flag | in | enum FpContract |  |
| `reassoc` | flag | in | bool |  |
| `arcp` | flag | in | bool |  |
| `nnan` | flag | in | bool |  |
| `ninf` | flag | in | bool |  |
| `nsz` | flag | in | bool |  |
| `afn` | flag | in | bool |  |
| `math_errno` | flag | in | bool |  |
| `round` | flag | in | enum FpRound |  |
| `except` | flag | in | enum FpExcept? |  |
| `fenv_access` | flag | in | bool |  |
| `eval` | flag | in | enum FpEval |  |
| `rounding_math` | flag | in | bool |  |

### `kiln_ast::fp::FpContract`

`kind=enum; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Off` | flag | in | tag 1 |  |
| `On` | flag | in | tag 2 |  |
| `Fast` | flag | in | tag 3 |  |

### `kiln_ast::fp::FpRound`

`kind=enum; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Dynamic` | flag | in | tag 1 |  |
| `Nearest` | flag | in | tag 2 |  |
| `Upward` | flag | in | tag 3 |  |
| `Downward` | flag | in | tag 4 |  |
| `TowardZero` | flag | in | tag 5 |  |
| `NearestAway` | flag | in | tag 6 |  |

### `kiln_ast::fp::FpEval`

`kind=enum; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Source` | flag | in | tag 1 |  |
| `Double` | flag | in | tag 2 |  |
| `Extended` | flag | in | tag 3 |  |

### `kiln_ast::fp::FpExcept`

`kind=enum; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Ignore` | flag | in | tag 1 |  |
| `MayTrap` | flag | in | tag 2 |  |
| `Strict` | flag | in | tag 3 |  |

## 7. Expressions

An expression is written as the rows of `ExprNode` (§2.1): its tag, value
category, object kind, dependence and type, then the rows of its kind in
walk order, then the rows of the maps keyed by its `ExprId`
(`ColdExprData`, and the expression maps of `TemplateTables`, §9). A kind
with no rows of its own (`CXXNullPtrLiteralExpr`, `GNUNullExpr`,
`ImplicitValueInitExpr`, `CXXScalarValueInitExpr`, `NoInitExpr`,
`ArrayInitIndexExpr`) is the common rows alone.

Rules the rows refer to:

- **Conditional rows.** A row whose note begins "when" is written only
  under that condition, which is always a fact an earlier row of the same
  node wrote.
- **Parentheses.** A `ParenExpr` is not written: the walk writes its
  operand in its place (digest §6). The one exception is a `ParenExpr`
  that is the whole expression a `profile` of a `DependentDecltype` walks
  (§4.2 rule 7): that node is written with its rows.
- **Shared nodes.** The default argument behind a `CXXDefaultArgExpr` and
  the default member initialiser behind a `CXXDefaultInitExpr` are not
  children: they are written in the unit of the function or field
  (`ParamData` `default_arg`, `FieldData` `init`). The use writes its own
  node and, when Kiln rebuilt the initialiser for the use, the rebuilt
  expression (`ColdExprData` `default_arg_rewritten`).
- **Opaque values.** An `OpaqueValueExpr` stands for the value of its
  source, which is evaluated once elsewhere. The walk numbers sources from
  0 in the order it first reaches them through an `OpaqueValueExpr` of the
  unit. The node writes that number as `uv` and, the first time only, the
  source walked in place. A node without a source writes the next number
  and no source.
- **Overloaded member access.** A `MemberExpr` whose type is the
  overloaded-function placeholder names the first candidate of a set that
  Kiln's semantic analysis keeps outside `Ast`. The first candidate is
  written; the rest of the set is the result of looking that member's name
  up in a class whose unit is compared separately ("Open questions").
- **Literals from builtin macros.** A literal that came from `__LINE__`
  or one of its relatives is written by value until Kiln can say where a
  literal came from (digest "Open questions").

### `kiln_ast::expr::ExprKind`

`kind=enum; pin=schema; base=768`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `IntegerLiteral` | flag | in | tag 769 |  |
| `FloatingLiteral` | flag | in | tag 770 |  |
| `CharacterLiteral` | flag | in | tag 771 |  |
| `StringLiteral` | flag | in | tag 772 |  |
| `CXXBoolLiteralExpr` | flag | in | tag 773 |  |
| `CXXNullPtrLiteralExpr` | flag | in | tag 774 |  |
| `UserDefinedLiteral` | flag | unsupported | unbuilt | never built: a call of a literal operator is a `CallExpr` |
| `ImaginaryLiteral` | flag | in | tag 776 |  |
| `EmbedExpr` | flag | unsupported | unbuilt | never built: `#embed` expands to integer literals |
| `GNUNullExpr` | flag | in | tag 778 |  |
| `DeclRefExpr` | flag | in | tag 779 |  |
| `MemberExpr` | flag | in | tag 780 |  |
| `CXXThisExpr` | flag | in | tag 781 |  |
| `PredefinedExpr` | flag | in | tag 782 |  |
| `SourceLocExpr` | flag | in | tag 783 |  |
| `UnresolvedLookupExpr` | flag | in | tag 784 |  |
| `UnresolvedMemberExpr` | flag | unsupported | unbuilt | never built: an overloaded member access is a `MemberExpr` of overloaded type |
| `DependentScopeDeclRefExpr` | flag | in | tag 786 |  |
| `CXXDependentScopeMemberExpr` | flag | in | tag 787 |  |
| `SubstNonTypeTemplateParmExpr` | flag | in | tag 788 |  |
| `SubstNonTypeTemplateParmPackExpr` | flag | in | tag 789 |  |
| `FunctionParmPackExpr` | flag | in | tag 790 |  |
| `UnaryOperator` | flag | in | tag 791 |  |
| `BinaryOperator` | flag | in | tag 792 |  |
| `CompoundAssignOperator` | flag | in | tag 793 |  |
| `ConditionalOperator` | flag | in | tag 794 |  |
| `BinaryConditionalOperator` | flag | in | tag 795 |  |
| `ArraySubscriptExpr` | flag | in | tag 796 |  |
| `ParenExpr` | written | in | tag 797 | written only by the rule for parentheses above |
| `CallExpr` | flag | in | tag 798 |  |
| `CXXMemberCallExpr` | flag | in | tag 799 |  |
| `CXXOperatorCallExpr` | flag | in | tag 800 |  |
| `CXXRewrittenBinaryOperator` | flag | in | tag 801 |  |
| `ParenListExpr` | flag | in | tag 802 |  |
| `ImplicitCastExpr` | flag | in | tag 803 |  |
| `CStyleCastExpr` | flag | in | tag 804 |  |
| `CXXFunctionalCastExpr` | flag | in | tag 805 |  |
| `CXXStaticCastExpr` | flag | in | tag 806 |  |
| `CXXConstCastExpr` | flag | in | tag 807 |  |
| `CXXReinterpretCastExpr` | flag | in | tag 808 |  |
| `CXXDynamicCastExpr` | flag | in | tag 809 |  |
| `BuiltinBitCastExpr` | flag | in | tag 810 |  |
| `InitListExpr` | flag | in | tag 811 |  |
| `ImplicitValueInitExpr` | flag | in | tag 812 |  |
| `DesignatedInitExpr` | flag | in | tag 813 |  |
| `CXXConstructExpr` | flag | in | tag 814 |  |
| `CXXTemporaryObjectExpr` | flag | in | tag 815 |  |
| `CXXInheritedCtorInitExpr` | flag | in | tag 816 |  |
| `CXXDefaultArgExpr` | flag | in | tag 817 |  |
| `CXXDefaultInitExpr` | flag | in | tag 818 |  |
| `CXXScalarValueInitExpr` | flag | in | tag 819 |  |
| `CXXParenListInitExpr` | flag | in | tag 820 |  |
| `ArrayInitLoopExpr` | flag | in | tag 821 |  |
| `ArrayInitIndexExpr` | flag | in | tag 822 |  |
| `CXXStdInitializerListExpr` | flag | in | tag 823 |  |
| `CompoundLiteralExpr` | flag | in | tag 824 |  |
| `NoInitExpr` | flag | in | tag 825 |  |
| `DesignatedInitUpdateExpr` | flag | in | tag 826 |  |
| `MaterializeTemporaryExpr` | flag | in | tag 827 |  |
| `CXXBindTemporaryExpr` | flag | in | tag 828 |  |
| `ExprWithCleanups` | flag | in | tag 829 |  |
| `OpaqueValueExpr` | flag | in | tag 830 |  |
| `CXXNewExpr` | flag | in | tag 831 |  |
| `CXXDeleteExpr` | flag | in | tag 832 |  |
| `CXXPseudoDestructorExpr` | flag | in | tag 833 |  |
| `CXXThrowExpr` | flag | in | tag 834 |  |
| `CXXNoexceptExpr` | flag | in | tag 835 |  |
| `CXXTypeidExpr` | flag | in | tag 836 |  |
| `UnaryExprOrTypeTraitExpr` | flag | in | tag 837 |  |
| `TypeTraitExpr` | flag | in | tag 838 |  |
| `ArrayTypeTraitExpr` | flag | in | tag 839 |  |
| `ExpressionTraitExpr` | flag | in | tag 840 |  |
| `OffsetOfExpr` | flag | in | tag 841 |  |
| `SizeOfPackExpr` | flag | in | tag 842 |  |
| `RequiresExpr` | flag | in | tag 843 |  |
| `ConceptSpecializationExpr` | flag | in | tag 844 |  |
| `LambdaExpr` | flag | in | tag 845 |  |
| `PackExpansionExpr` | flag | in | tag 846 |  |
| `CXXFoldExpr` | flag | in | tag 847 |  |
| `PackIndexingExpr` | flag | in | tag 848 |  |
| `CXXUnresolvedConstructExpr` | flag | in | tag 849 |  |
| `CoawaitExpr` | flag | in | tag 850 |  |
| `DependentCoawaitExpr` | flag | in | tag 851 |  |
| `CoyieldExpr` | flag | in | tag 852 |  |
| `ConstantExpr` | flag | in | tag 853 |  |
| `StmtExpr` | flag | in | tag 854 |  |
| `AddrLabelExpr` | flag | in | tag 855 |  |
| `ChooseExpr` | flag | in | tag 856 |  |
| `VAArgExpr` | flag | in | tag 857 |  |
| `AtomicExpr` | flag | in | tag 858 |  |
| `ShuffleVectorExpr` | flag | in | tag 859 |  |
| `ConvertVectorExpr` | flag | in | tag 860 |  |
| `ExtVectorElementExpr` | flag | in | tag 861 |  |
| `CXXReflectExpr` | flag | unsupported | - | reflection (§12) |
| `CXXSpliceExpr` | flag | unsupported | - | reflection (§12) |
| `RecoveryExpr` | flag | unsupported | - | an error-recovery expression: digest §10 |
| `CXXExpansionSelectExpr` | flag | in | tag 865 |  |
| `PseudoObjectExpr` | flag | in | tag 866 |  |
| `GenericSelectionExpr` | flag | in | tag 867 |  |
| `IntegerPackExpr` | flag | in | tag 868 |  |

### `kiln_ast::expr::ExprNode`

`kind=struct; pin=schema; selector=kiln_ast::expr::ExprKind`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `kind` | flag | in | enum ExprKind | the node's tag |
| `bits-value-category` | flag | in | enum ValueCategory | bits 0-1 |
| `bits-object-kind` | flag | in | enum ObjectKind | bits 2-3 |
| `deps` | flag | in | record ExprDeps |  |
| `ty` | type | in | type |  |
| `loc` | location | out | - |  |
| `IntegerLiteral.bits-wide` | flag | in | bit | bit 4: wider than 64 bits |
| `IntegerLiteral.a-low` | value | in | uv | when not wide: bits 0-31 of the value |
| `IntegerLiteral.b-high` | value | in | uv | when not wide: bits 32-63 |
| `IntegerLiteral.a-wide` | value | in | value | when wide: the constant `a` names |
| `IntegerLiteral.aux` | written | out | - | `Radix`: the base the literal was written in |
| `FloatingLiteral.a` | value | in | value |  |
| `CharacterLiteral.aux` | flag | in | enum CharKind |  |
| `CharacterLiteral.a` | value | in | uv | the code unit's value |
| `StringLiteral.aux` | flag | in | enum StringKind |  |
| `StringLiteral.a` | value | in | string |  |
| `CXXBoolLiteralExpr.a` | value | in | bool |  |
| `ImaginaryLiteral.a` | child | in | child |  |
| `DeclRefExpr.bits-names-closure-copy` | flag | in | bit | bit 4 |
| `DeclRefExpr.bits-refers-to-enclosing` | flag | in | bit | bit 5 |
| `DeclRefExpr.bits-had-multiple-candidates` | bookkeeping | out | - | bit 6: the entity chosen is the fact (digest §6) |
| `DeclRefExpr.bits-immediate-escalating` | flag | in | bit | bit 7 |
| `DeclRefExpr.aux` | flag | in | enum NonOdrUse |  |
| `DeclRefExpr.a` | entity | in | ref | the entity named, not the declaration it was found through |
| `DeclRefExpr.b` | written | in | record DeclRefExtra | index into `ExprArena` `declref_extras`; 0 is written as a record with nothing present |
| `MemberExpr.bits-arrow` | flag | in | bit | bit 4 |
| `MemberExpr.bits-spliced` | flag | unsupported | - | bit 5: written as a member splice; reflection (§12) |
| `MemberExpr.bits-had-multiple-candidates` | bookkeeping | out | - | bit 6: the member chosen is the fact (digest §6) |
| `MemberExpr.aux` | flag | in | enum NonOdrUse |  |
| `MemberExpr.a` | child | in | child | the object expression |
| `MemberExpr.b` | entity | in | ref | the member; for an overloaded access the first candidate (rule above) |
| `CXXThisExpr.aux-implicit` | flag | in | bit | bit 0: Kiln's evaluator and its template substitution read it |
| `PredefinedExpr.aux` | flag | in | enum PredefinedKind |  |
| `PredefinedExpr.a` | value | symbolic | - | the string literal holding the enclosing function's name; absent in a dependent context |
| `SourceLocExpr.aux` | flag | in | enum SourceLocKind |  |
| `SourceLocExpr.a` | entity | symbolic | - | the declaration the builtin is evaluated in, which follows from where the expression is |
| `UnresolvedLookupExpr.aux` | flag | in | bool | the name is subject to argument-dependent lookup at instantiation (digest §8) |
| `UnresolvedLookupExpr.a` | name | in | name | a raw `DeclName` |
| `UnresolvedLookupExpr.b` | entity | in | ref*sorted | the declarations found at the definition |
| `DependentScopeDeclRefExpr.a` | name | in | enum Nns | a raw `NnsId` |
| `DependentScopeDeclRefExpr.b` | name | in | name | a raw `DeclName` |
| `CXXDependentScopeMemberExpr.aux-arrow` | flag | in | bit | bit 0 |
| `CXXDependentScopeMemberExpr.aux-template-keyword` | written | out | - | bit 1: `template` was written; the explicit arguments (`TemplateTables` `expr_targs`) are the fact |
| `CXXDependentScopeMemberExpr.a` | child | in | child? | the object expression; absent for an implicit access |
| `CXXDependentScopeMemberExpr.b` | name | in | name | a raw `DeclName` |
| `SubstNonTypeTemplateParmExpr.bits-reference-parameter` | flag | in | bit | bit 4 |
| `SubstNonTypeTemplateParmExpr.a` | entity | in | ref | the parameter replaced |
| `SubstNonTypeTemplateParmExpr.b` | child | in | child | the replacement |
| `SubstNonTypeTemplateParmPackExpr.a` | entity | in | ref |  |
| `SubstNonTypeTemplateParmPackExpr.b` | value | in | enum ArgView* | a raw `ArgListId` |
| `FunctionParmPackExpr.a` | entity | in | ref | the pattern's parameter pack |
| `FunctionParmPackExpr.b` | entity | in | ref* | the expanded parameters, in order |
| `UnaryOperator.bits-cannot-overflow` | flag | in | bit | bit 4 |
| `UnaryOperator.aux` | flag | in | enum UnaryOp |  |
| `UnaryOperator.a` | child | in | child |  |
| `BinaryOperator.aux` | flag | in | enum BinaryOp |  |
| `BinaryOperator.a` | child | in | child |  |
| `BinaryOperator.b` | child | in | child |  |
| `CompoundAssignOperator.aux` | flag | in | enum BinaryOp |  |
| `CompoundAssignOperator.a` | child | in | child |  |
| `CompoundAssignOperator.b` | child | in | record CompoundAssignExtra | index into `compound_assigns` |
| `ConditionalOperator.a` | child | in | child |  |
| `ConditionalOperator.b-then` | child | in | child | index into `cond_arms`: the first arm |
| `ConditionalOperator.b-else` | child | in | child | the second arm |
| `BinaryConditionalOperator.a` | child | in | child* | common expression, condition, true arm, false arm |
| `ArraySubscriptExpr.a` | child | in | child |  |
| `ArraySubscriptExpr.b` | child | in | child |  |
| `ParenExpr.a` | child | in | child |  |
| `ParenExpr.b` | location | out | - | the closing parenthesis |
| `CallExpr.bits-adl` | bookkeeping | out | - | bit 5: found by argument-dependent lookup; the callee is the fact (digest §6) |
| `CallExpr.bits-assume-ignored` | flag | in | bit | bit 6 |
| `CallExpr.a` | child | in | child | the callee |
| `CallExpr.b` | child | in | child* |  |
| `CXXMemberCallExpr.a` | child | in | child | the callee: a `MemberExpr` or a pointer-to-member operator |
| `CXXMemberCallExpr.b` | child | in | child* |  |
| `CXXOperatorCallExpr.bits-adl` | bookkeeping | out | - | bit 5: as `CallExpr.bits-adl` |
| `CXXOperatorCallExpr.aux` | flag | in | enum OverloadedOperator | the operator as written, which fixes the order of evaluation |
| `CXXOperatorCallExpr.a` | child | in | child |  |
| `CXXOperatorCallExpr.b` | child | in | child* |  |
| `CXXRewrittenBinaryOperator.aux` | flag | in | enum BinaryOp | the operator as written: instantiation rebuilds from it |
| `CXXRewrittenBinaryOperator.a` | child | in | child | the semantic form |
| `CXXRewrittenBinaryOperator.b` | child | in | child* | the two operands as written: instantiation rebuilds from them |
| `ParenListExpr.a` | child | in | child* |  |
| `ImplicitCastExpr.bits-part-of-explicit-cast` | flag | in | bit | bit 4 |
| `ImplicitCastExpr.aux` | flag | in | enum CastKind |  |
| `ImplicitCastExpr.a` | child | in | child |  |
| `ImplicitCastExpr.b` | entity | in | type* | the base path: the base class type of each `BaseSpec` step |
| `CStyleCastExpr.aux` | flag | in | enum CastKind |  |
| `CStyleCastExpr.a` | child | in | child |  |
| `CStyleCastExpr.b` | entity | in | type* | as `ImplicitCastExpr.b` |
| `CXXFunctionalCastExpr.aux` | flag | in | enum CastKind |  |
| `CXXFunctionalCastExpr.a` | child | in | child |  |
| `CXXFunctionalCastExpr.b` | entity | in | type* | as `ImplicitCastExpr.b` |
| `CXXStaticCastExpr.aux` | flag | in | enum CastKind |  |
| `CXXStaticCastExpr.a` | child | in | child |  |
| `CXXStaticCastExpr.b` | entity | in | type* | as `ImplicitCastExpr.b` |
| `CXXConstCastExpr.aux` | flag | in | enum CastKind |  |
| `CXXConstCastExpr.a` | child | in | child |  |
| `CXXConstCastExpr.b` | entity | in | type* | as `ImplicitCastExpr.b` |
| `CXXReinterpretCastExpr.aux` | flag | in | enum CastKind |  |
| `CXXReinterpretCastExpr.a` | child | in | child |  |
| `CXXReinterpretCastExpr.b` | entity | in | type* | as `ImplicitCastExpr.b` |
| `CXXDynamicCastExpr.aux` | flag | in | enum CastKind |  |
| `CXXDynamicCastExpr.a` | child | in | child |  |
| `CXXDynamicCastExpr.b` | entity | in | type* | as `ImplicitCastExpr.b` |
| `BuiltinBitCastExpr.aux` | flag | in | enum CastKind |  |
| `BuiltinBitCastExpr.a` | child | in | child |  |
| `BuiltinBitCastExpr.b` | entity | in | type* | as `ImplicitCastExpr.b` |
| `InitListExpr.a` | child | in | child* |  |
| `InitListExpr.b` | child | in | record InitListExtra | index into `init_lists` |
| `DesignatedInitExpr.a` | child | in | child | the initialiser; the designators are `ColdExprData` `designators` |
| `CXXConstructExpr.bits-had-multiple-candidates` | bookkeeping | out | - | bit 6: the constructor chosen is the fact (digest §6) |
| `CXXConstructExpr.aux` | flag | in | record ConstructInfo |  |
| `CXXConstructExpr.a` | entity | in | ref | the constructor |
| `CXXConstructExpr.b` | child | in | child* |  |
| `CXXTemporaryObjectExpr.bits-had-multiple-candidates` | bookkeeping | out | - | bit 6: as `CXXConstructExpr` |
| `CXXTemporaryObjectExpr.aux` | flag | in | record ConstructInfo |  |
| `CXXTemporaryObjectExpr.a` | entity | in | ref |  |
| `CXXTemporaryObjectExpr.b` | child | in | child* |  |
| `CXXInheritedCtorInitExpr.aux-kind` | flag | in | enum ConstructionKind | bits 0-1 |
| `CXXInheritedCtorInitExpr.aux-virtual-base` | flag | in | bit | bit 2 |
| `CXXInheritedCtorInitExpr.a` | entity | in | ref | the inherited constructor |
| `CXXDefaultArgExpr.a` | entity | in | uv | the parameter, by its position (`ParamData` `index`); its function is the callee or constructor of the enclosing call |
| `CXXDefaultArgExpr.b` | bookkeeping | out | - | the context of the use, which follows from where the expression is |
| `CXXDefaultInitExpr.aux` | bookkeeping | out | - | always 1 |
| `CXXDefaultInitExpr.a` | entity | in | ref | the field |
| `CXXDefaultInitExpr.b` | bookkeeping | out | - | the context of the use |
| `CXXParenListInitExpr.a` | child | in | child* |  |
| `CXXParenListInitExpr.b` | child | in | child? | the array filler |
| `ArrayInitLoopExpr.a` | child | in | child | the common expression, an `OpaqueValueExpr` |
| `ArrayInitLoopExpr.b` | child | in | child | the element initialiser |
| `CXXStdInitializerListExpr.a` | child | in | child |  |
| `CompoundLiteralExpr.aux` | flag | unsupported | - | non-zero: a file-scope compound literal, whose implicit variable has no key (`ColdDeclData` `compound_literal`) |
| `CompoundLiteralExpr.a` | child | in | child |  |
| `CompoundLiteralExpr.b` | entity | unsupported | - | the implicit variable, when `aux` is non-zero |
| `DesignatedInitUpdateExpr.a` | child | in | child |  |
| `DesignatedInitUpdateExpr.b` | child | in | child |  |
| `MaterializeTemporaryExpr.aux-storage` | flag | in | enum StorageDuration | bits 0-1 |
| `MaterializeTemporaryExpr.aux-lvalue-reference` | flag | in | bit | bit 2 |
| `MaterializeTemporaryExpr.a` | child | in | child |  |
| `MaterializeTemporaryExpr.b` | entity | in | decl? | the `LifetimeExtendedTemporary`, written here as the rows of its declaration, without an ordinal |
| `CXXBindTemporaryExpr.a` | child | in | child |  |
| `CXXBindTemporaryExpr.b` | entity | in | ref | the destructor |
| `ExprWithCleanups.aux-side-effects` | flag | in | bit | bit 0 |
| `ExprWithCleanups.a` | child | in | child |  |
| `OpaqueValueExpr.a` | child | in | child? | the source, by the rule for opaque values above |
| `CXXNewExpr.bits-global` | flag | in | bit | bit 4 |
| `CXXNewExpr.bits-array-form` | flag | in | bit | bit 5 |
| `CXXNewExpr.bits-pass-align` | flag | in | bit | bit 6 |
| `CXXNewExpr.aux` | flag | in | enum NewInitStyle |  |
| `CXXNewExpr.a` | child | in | record NewExtra | index into `news` |
| `CXXDeleteExpr.bits-global` | flag | in | bit | bit 4 |
| `CXXDeleteExpr.bits-array-form` | flag | in | bit | bit 5 |
| `CXXDeleteExpr.bits-array-as-written` | written | out | - | bit 6: `delete[]` was written; `bits-array-form` is the fact |
| `CXXDeleteExpr.aux-sized` | flag | in | bit | bit 0: the deallocation function takes the size |
| `CXXDeleteExpr.aux-aligned` | flag | in | bit | bit 1: it takes the alignment |
| `CXXDeleteExpr.a` | child | in | child |  |
| `CXXDeleteExpr.b` | entity | in | ref | the deallocation function |
| `CXXPseudoDestructorExpr.aux-arrow` | flag | in | bit | bit 0 |
| `CXXPseudoDestructorExpr.aux-qualified` | flag | in | bit | bit 1: the destructor name was qualified; instantiation reads it |
| `CXXPseudoDestructorExpr.a` | child | in | child |  |
| `CXXPseudoDestructorExpr.b` | type | in | type | the destroyed type |
| `CXXThrowExpr.a` | child | in | child? |  |
| `CXXNoexceptExpr.a` | child | in | child |  |
| `CXXNoexceptExpr.b` | value | in | bool | the stored result |
| `CXXTypeidExpr.bits-arg-is-type` | bookkeeping | out | - | bit 4: which variant of `TypeidOperand` `a` holds |
| `CXXTypeidExpr.aux-potentially-evaluated` | flag | in | bit | bit 0 |
| `CXXTypeidExpr.a` | child | in | enum TypeidOperand |  |
| `UnaryExprOrTypeTraitExpr.bits-arg-is-type` | flag | in | bit | bit 4 |
| `UnaryExprOrTypeTraitExpr.aux` | flag | in | enum TraitKind |  |
| `UnaryExprOrTypeTraitExpr.a-type` | type | in | type | when the operand is a type |
| `UnaryExprOrTypeTraitExpr.a-expr` | child | in | child | when it is not |
| `TypeTraitExpr.aux` | flag | in | enum TypeTrait |  |
| `TypeTraitExpr.a` | type | in | type* |  |
| `TypeTraitExpr.b` | value | in | uv | the stored result: 0 or 1, or a count |
| `ArrayTypeTraitExpr.aux` | flag | in | enum TypeTrait |  |
| `ArrayTypeTraitExpr.a` | type | in | type |  |
| `ExpressionTraitExpr.aux` | flag | in | enum TypeTrait |  |
| `ExpressionTraitExpr.a` | child | in | child |  |
| `ExpressionTraitExpr.b` | value | in | uv | the stored result |
| `OffsetOfExpr.aux` | flag | in | bool | non-zero: the record is not standard-layout |
| `OffsetOfExpr.a` | child | in | child* | the index expressions |
| `OffsetOfExpr.b` | value | in | uv | the stored result |
| `SizeOfPackExpr.a` | entity | in | ref | the pack |
| `RequiresExpr.aux` | value | in | uv | the stored result: 0 not computed, 1 false, 2 true ("Open questions") |
| `RequiresExpr.a` | bookkeeping | out | - | the `RequiresExprBody`, the scope of the parameters `RequiresData` lists |
| `RequiresExpr.b` | child | in | record RequiresData | index into `TemplateTables` `requires` (§9) |
| `ConceptSpecializationExpr.aux` | value | in | uv | as `RequiresExpr.aux` |
| `ConceptSpecializationExpr.a` | entity | in | ref | the concept |
| `ConceptSpecializationExpr.b` | value | in | enum ArgView* | a raw `ArgListId` |
| `LambdaExpr.a` | child | in | decl | the closure type of the `LambdaData` that `a` indexes, written whole (§2.4); its `RecordData` `lambda` row writes the rest of the record (§10.4) |
| `PackExpansionExpr.a` | child | in | child |  |
| `CXXFoldExpr.aux` | flag | in | enum BinaryOp |  |
| `CXXFoldExpr.a` | child | in | child? |  |
| `CXXFoldExpr.b` | child | in | child? |  |
| `PackIndexingExpr.a` | child | in | child |  |
| `PackIndexingExpr.b` | child | in | child |  |
| `CXXUnresolvedConstructExpr.aux` | flag | in | bool | the braced form |
| `CXXUnresolvedConstructExpr.a` | child | in | child* |  |
| `CoawaitExpr.aux-implicit` | flag | in | bit | bit 0 |
| `CoawaitExpr.aux-suspend` | flag | in | enum SuspendKind | bits 1-2 |
| `CoawaitExpr.a` | child | in | child* | the first five elements of the list (`CoawaitParts`); the sixth, the placeholder for the awaiter, is reached from the calls |
| `DependentCoawaitExpr.a` | child | in | child |  |
| `CoyieldExpr.aux-implicit` | flag | in | bit | bit 0 |
| `CoyieldExpr.aux-suspend` | flag | in | enum SuspendKind | bits 1-2 |
| `CoyieldExpr.a` | child | in | child* | as `CoawaitExpr.a`; in a template pattern the operand alone |
| `ConstantExpr.aux-elided` | flag | in | bit | bit 0 |
| `ConstantExpr.aux-value-hidden` | flag | in | bit | bit 1 |
| `ConstantExpr.a` | child | in | child |  |
| `ConstantExpr.b` | value | in | value? | the stored value |
| `StmtExpr.a` | child | in | child | the compound statement |
| `AddrLabelExpr.a` | entity | in | ref | the label |
| `ChooseExpr.aux` | value | in | uv | the arm chosen: 1 or 2; 0 while dependent |
| `ChooseExpr.a` | child | in | child |  |
| `ChooseExpr.b-first` | child | in | child | index into `cond_arms` |
| `ChooseExpr.b-second` | child | in | child |  |
| `VAArgExpr.a` | child | in | child |  |
| `VAArgExpr.b` | type | in | type |  |
| `AtomicExpr.aux` | flag | in | uv | which atomic builtin, in Kiln's numbering ("Open questions") |
| `AtomicExpr.a` | child | in | child* |  |
| `ShuffleVectorExpr.aux-mask-known` | flag | in | bit | bit 0 |
| `ShuffleVectorExpr.a` | child | in | child* |  |
| `ShuffleVectorExpr.b` | value | in | uv* | when the mask is known: the folded indices |
| `ConvertVectorExpr.a` | child | in | child |  |
| `ExtVectorElementExpr.bits-arrow` | flag | in | bit | bit 4 |
| `ExtVectorElementExpr.a` | child | in | child |  |
| `ExtVectorElementExpr.b` | name | in | sym | the accessor, a raw `Symbol` |
| `CXXReflectExpr.aux` | value | unsupported | - | reflection (§12) |
| `CXXReflectExpr.a` | value | unsupported | - | reflection (§12) |
| `CXXReflectExpr.b` | value | unsupported | - | reflection (§12) |
| `CXXSpliceExpr.aux` | written | unsupported | - | reflection (§12) |
| `CXXSpliceExpr.a` | child | unsupported | - | reflection (§12) |
| `CXXSpliceExpr.b` | child | unsupported | - | reflection (§12) |
| `RecoveryExpr.a` | child | unsupported | - | digest §10 |
| `CXXExpansionSelectExpr.aux` | flag | in | enum ExpansionForm |  |
| `CXXExpansionSelectExpr.a` | value | in | uv | the depth of the statement's template level |
| `PseudoObjectExpr.a` | child | in | child* | the syntactic form, then the semantic expressions in evaluation order |
| `PseudoObjectExpr.b` | value | in | uv | which semantic expression is the result |
| `GenericSelectionExpr.bits-arg-is-type` | bookkeeping | out | - | bit 4: which variant of `GenericControl` the list starts with |
| `GenericSelectionExpr.a-control` | child | in | enum GenericControl | the first two words of the list |
| `GenericSelectionExpr.a-associations` | child | in | record GenericAssoc* | the rest of the list, three words each |
| `GenericSelectionExpr.b` | value | in | uv? | the association selected, by position; absent while dependent |
| `IntegerPackExpr.a` | child | in | child |  |

### 7.1 Words of the header

### `kiln_ast::expr::ValueCategory`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `PRValue` | flag | in | tag 1 |  |
| `LValue` | flag | in | tag 2 |  |
| `XValue` | flag | in | tag 3 |  |

### `kiln_ast::expr::ObjectKind`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Ordinary` | flag | in | tag 1 |  |
| `BitField` | flag | in | tag 2 |  |
| `VectorComponent` | flag | in | tag 3 |  |

### `kiln_ast::expr::ExprFlags`

`kind=struct; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `0` | bookkeeping | out | - | bits 4-7 of `ExprNode` `bits`; their meaning is per kind and is in the `bits-` rows of `ExprNode` |

### `kiln_ast::expr::ExprDeps`

`kind=struct; pin=schema`

Dependence is written: it decides what instantiation does with the node.

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `0-type` | flag | in | bit |  |
| `0-value` | flag | in | bit |  |
| `0-instantiation` | flag | in | bit |  |
| `0-unexpanded-pack` | flag | in | bit |  |
| `0-error` | flag | unsupported | - | the expression holds an error: digest §10 |

### 7.2 Enumerations stored in `aux`

### `kiln_ast::expr::UnaryOp`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `PostInc` | flag | in | tag 1 |  |
| `PostDec` | flag | in | tag 2 |  |
| `PreInc` | flag | in | tag 3 |  |
| `PreDec` | flag | in | tag 4 |  |
| `AddrOf` | flag | in | tag 5 |  |
| `Deref` | flag | in | tag 6 |  |
| `Plus` | flag | in | tag 7 |  |
| `Minus` | flag | in | tag 8 |  |
| `Not` | flag | in | tag 9 |  |
| `LNot` | flag | in | tag 10 |  |
| `Real` | flag | in | tag 11 |  |
| `Imag` | flag | in | tag 12 |  |
| `Extension` | flag | in | tag 13 |  |
| `Coawait` | flag | in | tag 14 |  |

### `kiln_ast::expr::BinaryOp`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `PtrMemD` | flag | in | tag 1 |  |
| `PtrMemI` | flag | in | tag 2 |  |
| `Mul` | flag | in | tag 3 |  |
| `Div` | flag | in | tag 4 |  |
| `Rem` | flag | in | tag 5 |  |
| `Add` | flag | in | tag 6 |  |
| `Sub` | flag | in | tag 7 |  |
| `Shl` | flag | in | tag 8 |  |
| `Shr` | flag | in | tag 9 |  |
| `Cmp` | flag | in | tag 10 |  |
| `LT` | flag | in | tag 11 |  |
| `GT` | flag | in | tag 12 |  |
| `LE` | flag | in | tag 13 |  |
| `GE` | flag | in | tag 14 |  |
| `EQ` | flag | in | tag 15 |  |
| `NE` | flag | in | tag 16 |  |
| `And` | flag | in | tag 17 |  |
| `Xor` | flag | in | tag 18 |  |
| `Or` | flag | in | tag 19 |  |
| `LAnd` | flag | in | tag 20 |  |
| `LOr` | flag | in | tag 21 |  |
| `Assign` | flag | in | tag 22 |  |
| `MulAssign` | flag | in | tag 23 |  |
| `DivAssign` | flag | in | tag 24 |  |
| `RemAssign` | flag | in | tag 25 |  |
| `AddAssign` | flag | in | tag 26 |  |
| `SubAssign` | flag | in | tag 27 |  |
| `ShlAssign` | flag | in | tag 28 |  |
| `ShrAssign` | flag | in | tag 29 |  |
| `AndAssign` | flag | in | tag 30 |  |
| `XorAssign` | flag | in | tag 31 |  |
| `OrAssign` | flag | in | tag 32 |  |
| `Comma` | flag | in | tag 33 |  |

### `kiln_ast::expr::CastKind`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Dependent` | flag | in | tag 1 |  |
| `NoOp` | flag | in | tag 2 |  |
| `LValueToRValue` | flag | in | tag 3 |  |
| `LValueToRValueBitCast` | flag | in | tag 4 |  |
| `LValueBitCast` | flag | in | tag 5 |  |
| `BitCast` | flag | in | tag 6 |  |
| `ArrayToPointerDecay` | flag | in | tag 7 |  |
| `FunctionToPointerDecay` | flag | in | tag 8 |  |
| `BuiltinFnToFnPtr` | flag | in | tag 9 |  |
| `NullToPointer` | flag | in | tag 10 |  |
| `NullToMemberPointer` | flag | in | tag 11 |  |
| `IntegralToPointer` | flag | in | tag 12 |  |
| `PointerToIntegral` | flag | in | tag 13 |  |
| `PointerToBoolean` | flag | in | tag 14 |  |
| `IntegralCast` | flag | in | tag 15 |  |
| `IntegralToBoolean` | flag | in | tag 16 |  |
| `BooleanToSignedIntegral` | flag | in | tag 17 |  |
| `IntegralToFloating` | flag | in | tag 18 |  |
| `FloatingToIntegral` | flag | in | tag 19 |  |
| `FloatingToBoolean` | flag | in | tag 20 |  |
| `FloatingCast` | flag | in | tag 21 |  |
| `DerivedToBase` | flag | in | tag 22 |  |
| `UncheckedDerivedToBase` | flag | in | tag 23 |  |
| `BaseToDerived` | flag | in | tag 24 |  |
| `Dynamic` | flag | in | tag 25 |  |
| `DerivedToBaseMemberPointer` | flag | in | tag 26 |  |
| `BaseToDerivedMemberPointer` | flag | in | tag 27 |  |
| `MemberPointerToBoolean` | flag | in | tag 28 |  |
| `ReinterpretMemberPointer` | flag | in | tag 29 |  |
| `UserDefinedConversion` | flag | in | tag 30 |  |
| `ConstructorConversion` | flag | in | tag 31 |  |
| `ToVoid` | flag | in | tag 32 |  |
| `ToUnion` | flag | in | tag 33 |  |
| `VectorSplat` | flag | in | tag 34 |  |
| `NonAtomicToAtomic` | flag | in | tag 35 |  |
| `AtomicToNonAtomic` | flag | in | tag 36 |  |
| `IntegralRealToComplex` | flag | in | tag 37 |  |
| `FloatingRealToComplex` | flag | in | tag 38 |  |
| `IntegralComplexCast` | flag | in | tag 39 |  |
| `FloatingComplexCast` | flag | in | tag 40 |  |
| `IntegralComplexToReal` | flag | in | tag 41 |  |
| `FloatingComplexToReal` | flag | in | tag 42 |  |
| `IntegralComplexToBoolean` | flag | in | tag 43 |  |
| `FloatingComplexToBoolean` | flag | in | tag 44 |  |
| `IntegralComplexToFloatingComplex` | flag | in | tag 45 |  |
| `FloatingComplexToIntegralComplex` | flag | in | tag 46 |  |
| `AddressSpaceConversion` | flag | in | tag 47 |  |

### `kiln_ast::expr::CharKind`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Ascii` | flag | in | tag 1 |  |
| `Wide` | flag | in | tag 2 |  |
| `Utf8` | flag | in | tag 3 |  |
| `Utf16` | flag | in | tag 4 |  |
| `Utf32` | flag | in | tag 5 |  |

### `kiln_ast::expr::StringKind`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Ordinary` | flag | in | tag 1 |  |
| `Wide` | flag | in | tag 2 |  |
| `Utf8` | flag | in | tag 3 |  |
| `Utf16` | flag | in | tag 4 |  |
| `Utf32` | flag | in | tag 5 |  |
| `Unevaluated` | flag | in | tag 6 |  |

### `kiln_ast::expr::TraitKind`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `SizeOf` | flag | in | tag 1 |  |
| `AlignOf` | flag | in | tag 2 |  |
| `PreferredAlignOf` | flag | in | tag 3 |  |
| `DataSizeOf` | flag | in | tag 4 |  |
| `VecStep` | flag | in | tag 5 |  |

### `kiln_ast::expr::NonOdrUse`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `None` | flag | in | tag 1 |  |
| `Unevaluated` | flag | in | tag 2 |  |
| `Constant` | flag | in | tag 3 |  |
| `Discarded` | flag | in | tag 4 |  |

### `kiln_ast::expr::Radix`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Decimal` | written | out | - | the base an integer literal was written in |
| `Hex` | written | out | - | the base an integer literal was written in |
| `Octal` | written | out | - | the base an integer literal was written in |
| `Binary` | written | out | - | the base an integer literal was written in |

### `kiln_ast::expr::ConstructionKind`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Complete` | flag | in | tag 1 |  |
| `NonVirtualBase` | flag | in | tag 2 |  |
| `VirtualBase` | flag | in | tag 3 |  |
| `Delegating` | flag | in | tag 4 |  |

### `kiln_ast::expr::StorageDuration`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `FullExpression` | flag | in | tag 1 |  |
| `Automatic` | flag | in | tag 2 |  |
| `Static` | flag | in | tag 3 |  |
| `Thread` | flag | in | tag 4 |  |

### `kiln_ast::expr::TypeTrait`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `HasNothrowAssign` | flag | in | tag 1 |  |
| `HasNothrowConstructor` | flag | in | tag 2 |  |
| `HasNothrowCopy` | flag | in | tag 3 |  |
| `HasNothrowMoveAssign` | flag | in | tag 4 |  |
| `HasTrivialAssign` | flag | in | tag 5 |  |
| `HasTrivialDefaultConstructor` | flag | in | tag 6 |  |
| `HasTrivialCopy` | flag | in | tag 7 |  |
| `HasTrivialDestructor` | flag | in | tag 8 |  |
| `HasTrivialMoveAssign` | flag | in | tag 9 |  |
| `HasTrivialMoveConstructor` | flag | in | tag 10 |  |
| `HasUniqueObjectRepresentations` | flag | in | tag 11 |  |
| `HasVirtualDestructor` | flag | in | tag 12 |  |
| `IsAbstract` | flag | in | tag 13 |  |
| `IsAggregate` | flag | in | tag 14 |  |
| `IsArithmetic` | flag | in | tag 15 |  |
| `IsArray` | flag | in | tag 16 |  |
| `IsBoundedArray` | flag | in | tag 17 |  |
| `IsClass` | flag | in | tag 18 |  |
| `IsCompleteType` | flag | in | tag 19 |  |
| `IsCompound` | flag | in | tag 20 |  |
| `IsConst` | flag | in | tag 21 |  |
| `IsDestructible` | flag | in | tag 22 |  |
| `IsEmpty` | flag | in | tag 23 |  |
| `IsEnum` | flag | in | tag 24 |  |
| `IsFinal` | flag | in | tag 25 |  |
| `IsFloatingPoint` | flag | in | tag 26 |  |
| `IsFunction` | flag | in | tag 27 |  |
| `IsFundamental` | flag | in | tag 28 |  |
| `IsIntegral` | flag | in | tag 29 |  |
| `IsLiteral` | flag | in | tag 30 |  |
| `IsLiteralType` | flag | in | tag 31 |  |
| `IsLvalueReference` | flag | in | tag 32 |  |
| `IsMemberFunctionPointer` | flag | in | tag 33 |  |
| `IsMemberObjectPointer` | flag | in | tag 34 |  |
| `IsMemberPointer` | flag | in | tag 35 |  |
| `IsNothrowDestructible` | flag | in | tag 36 |  |
| `IsNullptr` | flag | in | tag 37 |  |
| `IsObject` | flag | in | tag 38 |  |
| `IsPod` | flag | in | tag 39 |  |
| `IsPointer` | flag | in | tag 40 |  |
| `IsPolymorphic` | flag | in | tag 41 |  |
| `IsReference` | flag | in | tag 42 |  |
| `IsReferenceable` | flag | in | tag 43 |  |
| `IsRvalueReference` | flag | in | tag 44 |  |
| `IsScalar` | flag | in | tag 45 |  |
| `IsScopedEnum` | flag | in | tag 46 |  |
| `IsSigned` | flag | in | tag 47 |  |
| `IsStandardLayout` | flag | in | tag 48 |  |
| `IsTrivial` | flag | in | tag 49 |  |
| `IsTriviallyCopyable` | flag | in | tag 50 |  |
| `IsTriviallyDestructible` | flag | in | tag 51 |  |
| `IsTriviallyEqualityComparable` | flag | in | tag 52 |  |
| `IsTriviallyRelocatable` | flag | in | tag 53 |  |
| `IsUnboundedArray` | flag | in | tag 54 |  |
| `IsUnion` | flag | in | tag 55 |  |
| `IsUnsigned` | flag | in | tag 56 |  |
| `IsVoid` | flag | in | tag 57 |  |
| `IsVolatile` | flag | in | tag 58 |  |
| `CanPassInRegs` | flag | in | tag 59 |  |
| `IsBitwiseCloneable` | flag | in | tag 60 |  |
| `IsImplicitLifetime` | flag | in | tag 61 |  |
| `IsCppTriviallyRelocatable` | flag | in | tag 62 |  |
| `IsBaseOf` | flag | in | tag 63 |  |
| `IsVirtualBaseOf` | flag | in | tag 64 |  |
| `IsSame` | flag | in | tag 65 |  |
| `IsSameAs` | flag | in | tag 66 |  |
| `IsAssignable` | flag | in | tag 67 |  |
| `IsConvertible` | flag | in | tag 68 |  |
| `IsConvertibleTo` | flag | in | tag 69 |  |
| `IsLayoutCompatible` | flag | in | tag 70 |  |
| `IsNothrowAssignable` | flag | in | tag 71 |  |
| `IsNothrowConvertible` | flag | in | tag 72 |  |
| `IsPointerInterconvertibleBaseOf` | flag | in | tag 73 |  |
| `IsTriviallyAssignable` | flag | in | tag 74 |  |
| `ReferenceBindsToTemporary` | flag | in | tag 75 |  |
| `ReferenceConstructsFromTemporary` | flag | in | tag 76 |  |
| `ReferenceConvertsFromTemporary` | flag | in | tag 77 |  |
| `IsConstructible` | flag | in | tag 78 |  |
| `IsNothrowConstructible` | flag | in | tag 79 |  |
| `IsTriviallyConstructible` | flag | in | tag 80 |  |
| `ArrayRank` | flag | in | tag 81 |  |
| `ArrayExtent` | flag | in | tag 82 |  |
| `IsLvalueExpr` | flag | in | tag 83 |  |
| `IsRvalueExpr` | flag | in | tag 84 |  |
| `TypesCompatible` | flag | in | tag 85 |  |
| `IsStructural` | flag | in | tag 86 |  |
| `IsInvocable` | flag | in | tag 87 |  |
| `IsNothrowInvocable` | flag | in | tag 88 |  |
| `StructuredBindingSize` | flag | in | tag 89 |  |
| `LtSynthesizesFromSpaceship` | flag | in | tag 90 |  |
| `LeSynthesizesFromSpaceship` | flag | in | tag 91 |  |
| `GtSynthesizesFromSpaceship` | flag | in | tag 92 |  |
| `GeSynthesizesFromSpaceship` | flag | in | tag 93 |  |

### `kiln_ast::expr::NewInitStyle`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `None` | flag | in | tag 1 |  |
| `Parens` | flag | in | tag 2 |  |
| `Braces` | flag | in | tag 3 |  |

### `kiln_ast::expr::PredefinedKind`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Func` | flag | in | tag 1 |  |
| `Function` | flag | in | tag 2 |  |
| `PrettyFunction` | flag | in | tag 3 |  |

### `kiln_ast::expr::SourceLocKind`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `File` | flag | in | tag 1 |  |
| `FileName` | flag | in | tag 2 |  |
| `Function` | flag | in | tag 3 |  |
| `Line` | flag | in | tag 4 |  |
| `Column` | flag | in | tag 5 |  |
| `SourceLocStruct` | flag | in | tag 6 |  |

### `kiln_ast::expr::SuspendKind`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Void` | flag | in | tag 1 |  |
| `Bool` | flag | in | tag 2 |  |
| `Handle` | flag | in | tag 3 |  |

### `kiln_ast::expr::ConstructInfo`

`kind=struct; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `kind` | flag | in | enum ConstructionKind | bits 0-1 of `aux` |
| `list` | flag | in | bit | list-initialisation: its arguments are evaluated in order |
| `std_init_list` | flag | in | bit |  |
| `zeroing` | flag | in | bit |  |
| `immediate_escalating` | flag | in | bit |  |

### `kiln_ast::expr::TypeidOperand`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Type` | flag | in | tag 1 |  |
| `Type.0` | type | in | type |  |
| `Expr` | flag | in | tag 2 |  |
| `Expr.0` | child | in | child |  |

### `kiln_ast::expr::GenericControl`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Expr` | flag | in | tag 1 |  |
| `Expr.0` | child | in | child |  |
| `Type` | flag | in | tag 2 |  |
| `Type.0` | type | in | type |  |
| `Type.1` | location | out | - |  |

### `kiln_ast::expr::GenericAssoc`

`kind=struct; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `ty` | type | in | type? | absent for `default` |
| `ty_loc` | location | out | - |  |
| `expr` | child | in | child |  |

### `kiln_ast::expr::CoawaitParts`

`kind=struct; pin=extra`

The decoded list of a resolved `CoawaitExpr` or `CoyieldExpr`. The walk
reads the list itself (`ExprNode`, `CoawaitExpr.a`).

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `operand` | child | out | - | first element of the list `CoawaitExpr.a` writes |
| `common` | child | out | - | second element: the awaiter |
| `ready` | child | out | - | third element |
| `suspend` | child | out | - | fourth element |
| `resume` | child | out | - | fifth element |
| `opaque` | child | out | - | sixth element: the `OpaqueValueExpr` standing for `common`, which the three calls name and the rule for opaque values writes there |

### `kiln_ast::expr::ExtVectorAccess`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Lanes` | bookkeeping | out | - | computed from the accessor's spelling by `ext_vector_access`; never stored |
| `Lanes.lanes` | bookkeeping | out | - |  |
| `Lanes.repeated` | bookkeeping | out | - |  |
| `Illegal` | bookkeeping | out | - | computed from the accessor's spelling by `ext_vector_access`; never stored |
| `Illegal.0` | bookkeeping | out | - |  |
| `Illegal.1` | bookkeeping | out | - |  |
| `Exceeds` | bookkeeping | out | - | computed from the accessor's spelling by `ext_vector_access`; never stored |

### 7.3 Side records

### `kiln_ast::expr::DeclRefExtra`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `qualifier` | written | out | - | how the name was qualified: the entity found is the fact. Whether a member access is qualified is `ColdExprData` `member_extra-qualified` |
| `template_args` | written | in | enum ArgView*? | written only when the entity named is a template, where the arguments select what the name means; for a specialisation the entity is the fact |
| `found_decl` | entity | out | - | the using-declaration the name was found through: a reference names the entity (digest §4.1) |

### `kiln_ast::expr::CompoundAssignExtra`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `rhs` | child | in | child |  |
| `lhs_ty` | type | in | type | the type the left operand is converted to |
| `result_ty` | type | in | type | the type of the computation |

### `kiln_ast::expr::Designator`

`kind=enum; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Field` | flag | in | tag 1 |  |
| `Field.name` | name | in | sym | the member is looked up by name at instantiation |
| `Field.loc` | location | out | - |  |
| `Index` | flag | in | tag 2 |  |
| `Index.index` | child | in | child |  |
| `Index.loc` | location | out | - |  |
| `Range` | flag | in | tag 3 |  |
| `Range.first` | child | in | child |  |
| `Range.last` | child | in | child |  |
| `Range.loc` | location | out | - |  |
| `Range.ellipsis` | location | out | - |  |

### `kiln_ast::expr::OffsetofComponent`

`kind=enum; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Field` | flag | in | tag 1 |  |
| `Field.name` | name | in | sym |  |
| `Field.start` | location | out | - |  |
| `Field.end` | location | out | - |  |
| `Index` | flag | in | tag 2 |  |
| `Index.index` | child | in | child |  |
| `Index.start` | location | out | - |  |
| `Index.end` | location | out | - |  |

### `kiln_ast::expr::OffsetofStep`

`kind=enum; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Offset` | flag | in | tag 1 |  |
| `Offset.0` | value | in | uv | bytes |
| `Index` | flag | in | tag 2 |  |
| `Index.index` | child | in | child | also an element of `OffsetOfExpr.a`; written again here so that the step says which |
| `Index.elem_size` | value | in | uv |  |

### `kiln_ast::expr::InitListExtra`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `array_filler` | child | in | child? |  |
| `union_field` | entity | in | ref |  |
| `rbrace` | location | out | - |  |

### `kiln_ast::expr::NewExtra`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `allocated_ty` | type | in | type |  |
| `array_size` | child | in | child? |  |
| `init` | child | in | child? |  |
| `placement` | child | in | child*? |  |
| `operator_new` | entity | in | ref |  |
| `operator_delete` | entity | in | ref |  |
| `type_loc` | location | out | - |  |

### `kiln_ast::expr::SourceLocValues`

`kind=struct; pin=schema`

The values of the source-location builtins at one use. All of them follow
from where the use is and what the enclosing function is called (digest
§9); the builtin named is `ExprNode` `SourceLocExpr.aux`.

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `file` | value | symbolic | - | the presumed file name |
| `file_name` | value | symbolic | - | its last component |
| `function` | value | symbolic | - | the enclosing function's name |
| `line` | value | symbolic | - | the presumed line |
| `column` | value | symbolic | - | the column |
| `location` | value | symbolic | - | the `std::source_location::__impl` object built from the others |
| `at` | location | out | - |  |
| `ctx` | entity | symbolic | - | the declaration the values are for, which follows from where the use is |

### `kiln_ast::expr::ArrayTraitExtra`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `dim` | child | in | child? |  |
| `value` | value | in | uv | the stored result |

### 7.4 Cold maps and the arena

Each row of `ColdExprData` is written for every expression, whatever its
kind; an expression with no entry writes the encoding's empty form (§2.1).

### `kiln_ast::expr::ColdExprData`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `end_loc` | location | out | - |  |
| `member_extra-qualified` | flag | in | bool | a `MemberExpr` whose `DeclRefExtra` has a qualifier: a qualified member name suppresses virtual dispatch (digest §6) |
| `member_extra-record` | written | in | record DeclRefExtra | no entry is written as a record with nothing present |
| `designators` | name | in | enum Designator* | outermost first |
| `syntactic_list` | child | in | child? | the list as written, in a template pattern: instantiation rebuilds from it |
| `array_trait` | value | in | record ArrayTraitExtra? |  |
| `trait_rhs_loc` | location | out | - |  |
| `source_loc` | value | symbolic | - | the rows of `SourceLocValues` |
| `default_imm` | value | demand | - | §11.4 |
| `union_field` | entity | in | ref |  |
| `cast_written-unresolved-construct` | type | in | type? | on a `CXXUnresolvedConstructExpr`: the type as written, which instantiation and mangling use |
| `cast_written-cast` | written | out | - | on a cast: qualifiers and a reference the node's type and value category already say |
| `offsetof_pattern-type` | type | in | type? | a value-dependent `OffsetOfExpr`: the operand type |
| `offsetof_pattern-components` | name | in | enum OffsetofComponent* | its designator as written |
| `offsetof_layout` | value | in | enum OffsetofStep* |  |
| `splice_args` | value | unsupported | - | reflection (§12) |
| `default_arg_rewritten` | child | in | child? | the initialiser rebuilt for this use |
| `fp_round` | flag | in | enum FpRound? |  |

### `kiln_ast::expr::ExprArena`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `nodes` | bookkeeping | out | - | arena: reached through `ExprId` |
| `cond_arms` | bookkeeping | out | - | arena |
| `declref_extras` | bookkeeping | out | - | arena |
| `compound_assigns` | bookkeeping | out | - | arena |
| `init_lists` | bookkeeping | out | - | arena |
| `news` | bookkeeping | out | - | arena |
| `lambdas` | bookkeeping | out | - | arena: `LambdaArena` (§10.4) |
| `cold` | bookkeeping | out | - | the cold maps: `ColdExprData` |
| `round_scope` | bookkeeping | out | - | the builder's state while parsing; not stored (`snap(skip)`) |

## 8. Constant values

`value` writes a constant in place, by structure (digest §7): the tag of
its `ValueKind`, then the `in` rows of the variant of `Scalar` with the
same name, or the rows of `ByteImage` for `Aggregate`. The walk reads a
constant only through `ConstStore::get`, which returns the decoded
`ConstValue`; the stored words (`Node`, `Pools`) have rows that are `out`
and name the view, as for types (§4.1). Nothing written depends on the
`ConstValueId` or on the order in which Kiln interned the values.

A constant can hold a pointer to a constant (`PtrBase::Const`, `Reloc`);
that one is written in place by the same rule. Kiln's store is append-only
and a value can only name values interned before it, so this ends.

### `kiln_ast::consts::value::ValueKind`

`kind=enum; pin=extra; base=1280`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Int` | value | in | tag 1281 |  |
| `Bool` | value | in | tag 1282 |  |
| `Float` | value | in | tag 1283 |  |
| `Null` | value | in | tag 1284 |  |
| `Pointer` | value | in | tag 1285 |  |
| `MemberPtr` | value | in | tag 1286 |  |
| `Reflection` | value | unsupported | - | reflection (§12) |
| `Aggregate` | value | in | tag 1288 |  |

### `kiln_ast::consts::value::ConstValue`

`kind=view; pin=none`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Scalar` | bookkeeping | out | - | not a tag: `ValueKind` gives the tag |
| `Scalar.0` | value | in | record Scalar | the rows of the scalar's variant |
| `Aggregate` | bookkeeping | out | - | not a tag: `ValueKind` gives the tag |
| `Aggregate.0` | value | in | record ByteImage |  |

### `kiln_ast::consts::value::Scalar`

`kind=view; pin=none`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Int` | bookkeeping | out | - | not a tag: the `ValueKind` of the same name gives the tag |
| `Int.ty` | type | in | type |  |
| `Int.signed` | flag | in | bool |  |
| `Int.bits` | value | in | bytes | the normalised words, each little-endian: 8 bytes for every 64 bits of the type's width |
| `Bool` | bookkeeping | out | - | not a tag: the `ValueKind` of the same name gives the tag |
| `Bool.ty` | type | in | type |  |
| `Bool.value` | value | in | bool |  |
| `Float` | bookkeeping | out | - | not a tag: the `ValueKind` of the same name gives the tag |
| `Float.ty` | type | in | type |  |
| `Float.kind` | flag | in | enum FloatKind |  |
| `Float.bits` | value | in | bytes | the exact encoding, 16 bytes little-endian |
| `Null` | bookkeeping | out | - | not a tag: the `ValueKind` of the same name gives the tag |
| `Null.ty` | type | in | type |  |
| `Null.kind` | flag | in | enum NullKind |  |
| `Pointer` | bookkeeping | out | - | not a tag: the `ValueKind` of the same name gives the tag |
| `Pointer.ty` | type | in | type |  |
| `Pointer.base` | entity | in | enum PtrBase |  |
| `Pointer.offset` | value | in | uv |  |
| `Pointer.designated` | type | in | type? |  |
| `Pointer.past` | flag | in | bool |  |
| `MemberPtr` | bookkeeping | out | - | not a tag: the `ValueKind` of the same name gives the tag |
| `MemberPtr.ty` | type | in | type |  |
| `MemberPtr.member` | entity | in | ref |  |
| `MemberPtr.kind` | flag | in | enum MemberKind |  |
| `MemberPtr.derived` | flag | in | bool |  |
| `MemberPtr.path` | entity | in | ref* | the classes traversed, outermost first |
| `Reflection` | bookkeeping | out | - | not a tag: the `ValueKind` of the same name gives the tag |
| `Reflection.ty` | type | unsupported | - | reflection (§12) |
| `Reflection.id` | value | unsupported | - | reflection (§12) |

### `kiln_ast::consts::value::FloatKind`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Half` | flag | in | tag 1 |  |
| `BFloat16` | flag | in | tag 2 |  |
| `Single` | flag | in | tag 3 |  |
| `Double` | flag | in | tag 4 |  |
| `X87Extended` | flag | in | tag 5 |  |
| `Quad` | flag | in | tag 6 |  |

### `kiln_ast::consts::value::NullKind`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Pointer` | flag | in | tag 1 |  |
| `MemberData` | flag | in | tag 2 |  |
| `MemberFunction` | flag | in | tag 3 |  |

### `kiln_ast::consts::value::MemberKind`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Data` | flag | in | tag 1 |  |
| `Function` | flag | in | tag 2 |  |

### `kiln_ast::consts::value::PtrBase`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Decl` | entity | in | tag 1 |  |
| `Decl.0` | entity | in | ref |  |
| `Function` | entity | in | tag 2 |  |
| `Function.0` | entity | in | ref |  |
| `Temporary` | entity | in | tag 3 |  |
| `Temporary.0` | entity | in | ref | the `Ref` of the temporary's extending declaration, then `uv` its `LetData` `mangling_index`; the temporary's value is written where it is materialised (rule below) |
| `StringLit` | entity | in | tag 4 |  |
| `StringLit.lit` | value | in | string |  |
| `StringLit.origin` | entity | out | - | which literal expression made the object: whether two equal literals are one object is unspecified and is not digested (digest §7) |
| `Const` | entity | in | tag 5 |  |
| `Const.0` | value | in | value |  |
| `TypeInfo` | entity | in | tag 6 |  |
| `TypeInfo.0` | type | in | type |  |
| `Label` | entity | in | tag 7 |  |
| `Label.label` | entity | in | ref |  |
| `Label.origin` | entity | out | - | which `AddrLabelExpr` produced the address; the label is the fact |
| `Absolute` | entity | in | tag 8 |  |
| `LabelDiff` | entity | in | tag 9 |  |
| `LabelDiff.plus` | entity | in | ref | the label the `AddrLabelExpr` names |
| `LabelDiff.minus` | entity | in | ref | the label the `AddrLabelExpr` names |
| `LabelDiff.size` | value | in | uv |  |

A pointer to a lifetime-extended temporary does not write the temporary's
value, although digest §7 says "by the digest of its value": a temporary
can hold a pointer to itself, and its value is already written, with its
`LetData`, at the `MaterializeTemporaryExpr` in the unit of the declaration
that extends it. The pair written here names the temporary within that
declaration. ("Open questions" proposes the matching change to
`digest.md`.)

### `kiln_ast::consts::value::ByteImage`

`kind=view; pin=none`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `ty` | type | in | type |  |
| `bytes` | value | in | bytes | the object representation in target layout; relocated and padding bytes are zero in it |
| `relocs` | value | in | record Reloc* | sorted by offset, as stored |
| `padding` | value | in | record ByteRange* | bytes that hold no value; an uninitialised subobject is among them, so the list is not a function of the type alone |
| `unions` | entity | in | record UnionMember* |  |

### `kiln_ast::consts::value::ByteRange`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `start` | value | in | uv |  |
| `len` | value | in | uv |  |

### `kiln_ast::consts::value::Reloc`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `at` | value | in | uv |  |
| `value` | value | in | value | the pointer the bytes hold |

### `kiln_ast::consts::value::UnionMember`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `at` | value | in | uv |  |
| `field` | entity | in | ref | the active member |

### `kiln_ast::consts::value::ShadowKind`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Value` | bookkeeping | out | - | computed from `relocs` and `padding` by `ConstStore::shadow`; never stored |
| `Padding` | bookkeeping | out | - | computed from `relocs` and `padding` by `ConstStore::shadow`; never stored |
| `Pointer` | bookkeeping | out | - | computed from `relocs` and `padding` by `ConstStore::shadow`; never stored |
| `Pointer.0` | bookkeeping | out | - |  |

### `kiln_ast::consts::value::ShadowRun`

`kind=struct; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `range` | bookkeeping | out | - | as `ShadowKind` |
| `kind` | bookkeeping | out | - | as `ShadowKind` |

### 8.1 The store

### `kiln_ast::consts::Node`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `ty` | bookkeeping | out | - | read through `ConstValue` |
| `tag` | bookkeeping | out | - | kind, flags and a 24-bit operand, packed; read through `ConstValue` |
| `a` | bookkeeping | out | - | per kind; read through `ConstValue` |
| `b` | bookkeeping | out | - | per kind; read through `ConstValue` |

### `kiln_ast::consts::AggRec`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `bytes` | bookkeeping | out | - | where the aggregate's slices start in the pools |
| `relocs` | bookkeeping | out | - |  |
| `padding` | bookkeeping | out | - |  |
| `unions` | bookkeeping | out | - |  |
| `hash` | bookkeeping | out | - | the intern hash |

### `kiln_ast::consts::Pools`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `pointer_bytes` | bookkeeping | out | - | the target's pointer size, also `TargetTypes` (§4.7) |
| `nodes` | bookkeeping | out | - | arena: reached through `ConstValueId` |
| `words` | bookkeeping | out | - | arena |
| `paths` | bookkeeping | out | - | arena |
| `aggs` | bookkeeping | out | - | arena |
| `bytes` | bookkeeping | out | - | arena |
| `relocs` | bookkeeping | out | - | arena |
| `padding` | bookkeeping | out | - | arena |
| `unions` | bookkeeping | out | - | arena |

### `kiln_ast::consts::ConstStore`

`kind=root; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `pools` | bookkeeping | out | - |  |
| `table` | bookkeeping | out | - | the intern table; not stored (`snap(skip)`) |

## 9. Templates and specialisations

A template is one unit. Its stream is its declaration's rows (§5.1); the
`data` row of its kind brings in `TemplateInfo`, which writes the template
head and then the pattern in place. The pattern (`TemplLink` kind
`Pattern`) is not a unit. If it is a class its members are units as the
members of any class are, and their parent is the template's key.

A template parameter is written by `decl` inside its head: its rows, with
no ordinal. A reference to it is `Ref::template_param` with the depth and
index its `TemplateParmInfo` gives; its name is not written (`DeclNode`
`name` is `keyed`, and a parameter has no key).

The **template head** of a discriminator (§3.3) is the rows of
`TemplateParamList` with, for each parameter, the rows of `DeclNode` and
`TemplateParmInfo` and nothing else: default template arguments are not
part of a template's identity.

Which specialisations are units follows from `SpecKind`, read from the
`SpecInfo` or `MemberSpecInfo` the declaration's `TemplLink` indexes:

| SpecKind | The specialisation or member is |
|---|---|
| `Undeclared` | not a unit: it was named and nothing else is known of it. A reference writes its key as for any entity |
| `ImplicitInstantiation` | a derived unit (digest §2.2) |
| `ExplicitSpecialization` | a declared unit |
| `ExplicitInstantiationDeclaration`, `ExplicitInstantiationDefinition` | a declared unit: Kiln has no declaration for an explicit instantiation, only this kind on the specialisation it names |

A member whose own kind is `ImplicitInstantiation` is derived even when
its class was explicitly instantiated; that the class's explicit
instantiation covers it is a fact of the class's unit (`SpecInfo` `kind`).

A `ref` to a pattern that has no key (a local declaration of the
template's unit) is written as `Ref::none`: such a pattern cannot be named
from another unit, and the instantiated declaration's own rows say what it
is.

### 9.1 Links

### `kiln_ast::decl::TemplLinkKind`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Pattern` | flag | in | tag 1 |  |
| `Specialization` | flag | in | tag 2 |  |
| `MemberOfSpecialization` | flag | in | tag 3 |  |
| `DependentFriend` | flag | unsupported | unbuilt | no construction site found in Kiln |
| `PartialSpecialization` | flag | in | tag 5 |  |

### `kiln_ast::decl::TemplLink`

`kind=struct; pin=schema; selector=kiln_ast::decl::TemplLinkKind`

The word is 0 for a declaration that is neither templated nor a
specialisation. Otherwise bits 29-31 are the kind and bits 0-28 an index.

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `0-kind` | flag | in | enum TemplLinkKind? | absent when the word is 0 |
| `Pattern.0-index` | bookkeeping | out | - | index into `TemplateTables` `infos`: the template whose pattern this is, in whose unit the declaration is being written |
| `Specialization.0-index` | entity | in | record SpecInfo | index into `TemplateTables` `specs` |
| `MemberOfSpecialization.0-index` | entity | in | record MemberSpecInfo | index into `TemplateTables` `member_specs` |
| `DependentFriend.0-index` | entity | unsupported | unbuilt | no construction site found in Kiln |
| `PartialSpecialization.0-index` | entity | in | record PartialSpecInfo | index into `TemplateTables` `partials` |

### 9.2 Templates and their parameters

### `kiln_ast::templates::TemplateInfo`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `decl` | bookkeeping | out | - | the template itself |
| `params` | entity | in | record TemplateParamList | the head; also in the key (§3.3) |
| `pattern` | entity | in | decl? | the templated declaration, written whole |
| `constraint` | child | in | child? | a concept's constraint-expression |

### `kiln_ast::templates::TemplateParamList`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `template_loc` | location | out | - |  |
| `l_angle` | location | out | - |  |
| `r_angle` | location | out | - |  |
| `depth` | value | in | uv |  |
| `params` | entity | in | decl* | the parameters, in order |
| `requires` | child | in | child? | the requires-clause after the head |

### `kiln_ast::templates::TemplateParmInfo`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `depth` | value | in | uv |  |
| `index` | value | in | uv |  |
| `pack` | flag | in | bit |  |
| `typename_kw` | written | out | - | `typename` or `class`: spelling |
| `constraint` | child | in | child? | a type parameter's type-constraint |
| `params` | entity | in | record TemplateParamList? | a template template parameter's own head |
| `ttp_kind` | flag | in | enum TtpKind |  |

### `kiln_ast::templates::TtpKind`

`kind=enum; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Type` | flag | in | tag 1 |  |
| `Variable` | flag | in | tag 2 |  |
| `Concept` | flag | in | tag 3 |  |

### 9.3 Requires-expressions

A `RequiresExpr` (§7) writes the rows of its `RequiresData`: the
parameters by `decl`, each with the ordinal §2.4 gives a parameter, then
the requirements.

### `kiln_ast::templates::RequiresData`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `body` | bookkeeping | out | - | the `RequiresExprBody` declaration: a scope and nothing else |
| `params` | entity | in | decl* |  |
| `first` | child | in | enum Requirement* | the `len` entries of `TemplateTables` `requirements` from this index |
| `len` | bookkeeping | out | - | the count `first` writes |

### `kiln_ast::templates::Requirement`

`kind=enum; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Simple` | child | in | tag 1 |  |
| `Simple.0` | child | in | child |  |
| `Type` | child | in | tag 2 |  |
| `Type.0` | type | in | type |  |
| `Type.1` | location | out | - |  |
| `Compound` | child | in | tag 3 |  |
| `Compound.expr` | child | in | child |  |
| `Compound.noexcept-present` | flag | in | bool | `noexcept` was written |
| `Compound.noexcept-position` | location | out | - |  |
| `Compound.constraint` | child | in | child? | the return-type-requirement's constraint |
| `Nested` | child | in | tag 4 |  |
| `Nested.0` | child | in | child |  |

### 9.4 Specialisations

### `kiln_ast::templates::SpecKind`

`kind=enum; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Undeclared` | flag | out | - | a specialisation of this kind has no unit and so no stream (table above) |
| `ImplicitInstantiation` | flag | in | tag 2 |  |
| `ExplicitSpecialization` | flag | in | tag 3 |  |
| `ExplicitInstantiationDeclaration` | flag | in | tag 4 |  |
| `ExplicitInstantiationDefinition` | flag | in | tag 5 |  |

### `kiln_ast::templates::SpecInfo`

`kind=struct; pin=schema`

`from` is the template itself until Kiln instantiates the definition, and
then the pattern chosen: the primary template's pattern or a partial
specialisation. It is written as the `Ref` of the template or of the
partial specialisation, so a specialisation not yet instantiated and one
instantiated from the primary template write the same.

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `template` | entity | in | keyed | the primary template: the key's parent |
| `args` | value | in | keyed | the discriminator, in canonical form (`TypeContext::canonical_args`); the sugar is not written |
| `kind` | flag | in | enum SpecKind |  |
| `from` | entity | in | ref | rule above (digest §8) |
| `from_args` | value | in | enum ArgView*? | the arguments deduced for the partial specialisation `from` |
| `point_of_instantiation` | location | out | - |  |
| `print_args` | written | out | - | the arguments names print with: `args` without trailing defaults; read by printing only |
| `defaulted` | flag | in | uv | bit i: argument i came from, or equals, its parameter's default; debug information reads it |

### `kiln_ast::templates::MemberSpecInfo`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `instantiated_from` | entity | in | ref | the member of the pattern |
| `kind` | flag | in | enum SpecKind |  |
| `point_of_instantiation` | location | out | - |  |

### `kiln_ast::templates::PartialSpecInfo`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `partial` | bookkeeping | out | - | the partial specialisation itself |
| `template` | entity | in | keyed | the primary template: the key's parent |
| `params` | entity | in | record TemplateParamList | the head; also in the key (§3.3) |
| `args` | written | out | - | the argument pattern as written; `canonical` carries it |
| `canonical` | value | in | keyed | the argument pattern: in the discriminator after the head |
| `has_pack` | bookkeeping | out | - | computed from `params` |
| `shapes` | bookkeeping | out | - | index into `TemplateTables` `shapes`: a prefilter computed from `canonical` |
| `shapes_len` | bookkeeping | out | - |  |

### `kiln_ast::templates::ArgShape`

`kind=enum; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Any` | bookkeeping | out | - | a prefilter for matching partial specialisations, computed from `PartialSpecInfo` `canonical` when the partial is declared |
| `Type` | bookkeeping | out | - | a prefilter for matching partial specialisations, computed from `PartialSpecInfo` `canonical` when the partial is declared |
| `Type.kind` | bookkeeping | out | - |  |
| `Type.quals` | bookkeeping | out | - |  |
| `Exact` | bookkeeping | out | - | a prefilter for matching partial specialisations, computed from `PartialSpecInfo` `canonical` when the partial is declared |
| `Exact.0` | bookkeeping | out | - |  |

### 9.5 The template tables

The `in` rows whose map is keyed by `DeclId` close every declaration's
stream (§5.1), and those keyed by `ExprId` close every expression's (§2.1
step 3), each in the empty form when the map has no entry.

A parameter's default template argument is the entry of `defaults` for the
parameter itself, or for the parameter `inherited_defaults` names; over
the template's redeclarations it is taken from the declaration that gives
it (§2.3).

The rows from `explicit_member_insts` to `late_explicit_set` say where
among the translation unit's definitions code generation emits an
instantiated definition. None is written: the position changes the order
of the object file and of nothing the language defines, since the dynamic
initialisation of an instantiated variable is unordered
([basic.start.dynamic]/1), and the order unit (digest §2.1) therefore
lists no instantiation. The rows that record what was instantiated, and in
what order, are `demand`.

### `kiln_ast::templates::TemplateTables`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `param_lists` | bookkeeping | out | - | arena: reached through `TemplateParamListId` |
| `infos` | bookkeeping | out | - | arena |
| `parms` | bookkeeping | out | - | arena |
| `requirements` | bookkeeping | out | - | arena |
| `requires` | bookkeeping | out | - | arena |
| `defaults` | value | in | enum ArgView? | by parameter; rule above |
| `inherited_defaults` | bookkeeping | out | - | which earlier parameter owns the default `defaults` writes |
| `trailing_requires` | child | in | child? | by function; also in the key as a profile (§3.3) |
| `expr_targs` | value | in | enum ArgView*? | the explicit template arguments of a dependent or unresolved name |
| `expr_nns` | name | in | enum Nns? | the qualifier of a dependent or unresolved name |
| `expr_begin` | location | out | - |  |
| `size_of_pack_origin` | entity | in | ref | the pack a substituted `sizeof...` named: the literal still mangles by it |
| `size_of_pack_partial` | value | in | enum ArgView*? | the partly substituted arguments of a `sizeof...`: mangled |
| `pack_index_origin` | flag | in | bool | the expression is the selected element of a substituted `pack...[i]`; the pattern's expression it maps to belongs to the template's unit |
| `member_op_loc` | location | out | - |  |
| `pseudo_dtor_name` | name | in | sym? | the identifier after `~` that named no type at the definition |
| `pseudo_dtor_first-type` | type | in | type? | the first type-name of `p->S::F::~Q()` |
| `pseudo_dtor_first-position` | location | out | - |  |
| `template_kw` | written | out | - | the `template` keyword after a qualifier: read by the printer only |
| `ttp_owner` | entity | in | ref | by parameter: the template whose head declares a concept template parameter, its mangling parent |
| `member_like_friend` | entity | in | ref | by friend function template: the class specialisation that is its mangling scope |
| `pattern_next` | bookkeeping | out | - | state of the builder |
| `dependent_depth` | bookkeeping | out | - | state of the builder |
| `specs` | bookkeeping | out | - | arena: reached through `TemplLink` |
| `member_specs` | bookkeeping | out | - | arena: reached through `TemplLink` |
| `explicit_member_insts` | bookkeeping | out | - | where an explicitly instantiated member is emitted; that it is explicitly instantiated is its `MemberSpecInfo` `kind` or its class's `SpecInfo` `kind` |
| `end_of_tu_defs` | bookkeeping | demand | - | §11: the definitions instantiated at the end of the translation unit, in instantiation order |
| `ref_var_insts` | bookkeeping | demand | - | §11: the variables instantiated at a reference, with the declaration each is emitted beside |
| `moved_class_insts` | bookkeeping | out | - | the classes whose members are emitted at an explicit instantiation definition that follows a declaration; both are `SpecInfo` `kind`, of which the last is written |
| `written_args` | written | out | - | how many arguments were written before defaults filled the rest: read by GCC-style display names only |
| `args_as_written` | written | out | - | the arguments of an explicit specialisation or instantiation as written: read by the type printer only; `SpecInfo` `args` is the fact |
| `late_explicit_insts` | bookkeeping | out | - | functions explicitly instantiated before their template was defined: emitted at the end of the translation unit |
| `late_explicit_set` | bookkeeping | out | - | the same functions as a set |
| `preferred_named` | written | out | - | which specialisations print by their template's `preferred_name`: the attribute is written with the template, and names are not digested |
| `specializations` | bookkeeping | demand | - | §11: the lookup table from template and canonical arguments to specialisation; each specialisation is a unit or nothing by the rule of this section |
| `spec_order` | bookkeeping | demand | - | §11: each template's specialisations in creation order |
| `instantiated_from` | entity | in | ref | by declaration: the pattern of an instantiated declaration that has no `TemplLink` |
| `member_map` | bookkeeping | demand | - | §11: lookup table from specialisation and pattern member to instantiated member; `MemberSpecInfo` `instantiated_from` says the same from the member |
| `partials` | bookkeeping | out | - | arena: reached through `TemplLink` |
| `shapes` | bookkeeping | out | - | arena |
| `partials_of` | bookkeeping | out | - | each template's partial specialisations: each is a unit whose key names the template |
| `partial_map` | bookkeeping | out | - | lookup table for redeclarations of a partial specialisation |
| `param_objects` | bookkeeping | out | - | the inverse of `param_object_values` |
| `param_object_values` | value | in | value? | by `TemplateParamObject`: the object's value, which is its unit's whole stream |

## Tests

To be completed with the last group of tables.

## Open questions

To be completed with the last group of tables.
