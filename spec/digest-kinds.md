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

1. its tag, if it is a variant of a node enumeration;
2. the `in` rows of its record's table that have no `kind` prefix, in
   table order;
3. the `in` rows prefixed with its kind, in table order. This is the
   node's walk order (digest §4.3): the tables are ordered for it, not by
   the position of the words in Kiln's record;
4. for a node with cold-map entries, the `in` rows of the cold map's table
   that apply to it, in table order, each as an optional value.

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
declaration `Ast::exception_spec_of` answers for), `inline`, `constexpr`
and `constinit` (set if any declaration sets them), language linkage and
module ownership (from the first declaration). A row merged in another way
says so in its note.

### 2.4 Declarations walked in place

`decl` applies digest §4.3 to a declaration met inside a unit:

- a variable, parameter, binding or label gets the next ordinal, then (at
  its declaration, not at a reference) its rows are written;
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
| 18 | `using_enum` | |
| 19 | `namespace_alias` | |
| 20 | `binding` | a structured binding at namespace scope |
| 21 | `decomposition` | the variable of a structured binding declaration at namespace scope |
| 22 | `using_directive` | keyed by content |
| 23 | `static_assert` | keyed by content |
| 24 | `explicit_instantiation` | keyed by content |
| 25 | `friend` | keyed by content |
| 26 | `file_scope_asm` | keyed by content |
| 27 | `consteval_block` | keyed by content |
| 28 | `resolved` | a resolved property (digest §2.2); its parent is the unit it belongs to |
| 29 | `order` | the order unit (digest §2.1) |
| 30 | `default_argument` | an instantiated default argument or default member initialiser; its parent is the function or field |
| 31 | `template_param_object` | a class-type constant used as a template argument; keyed by content |

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
| `function`, `deduction_guide` | parameter types in order (`type*`), variadic, cv- and ref-qualifiers, explicit-object flag, trailing requires-clause (`profile?`); absent for a function with C language linkage |
| `function_template` | the template head (§9, `TemplateParamList`), then the function's discriminator, then its return type |
| `class_template`, `variable_template`, `alias_template`, `concept` | the template head |
| `specialisation` | the canonical template arguments (`enum ArgView*`) |
| `partial_specialisation` | the template head, then the argument pattern (`enum ArgView*`) |
| `using_declaration` | the key of the scope its qualifier names; for a dependent one, the qualifier by structure (`enum Nns`) |
| `using_enum` | the key of the enumeration |
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
   *function declaration* is a field of the function's unit (§5,
   `FunctionData`), read through `Ast::exception_spec_of`, in every
   language mode, so it is compared whether or not the type holds it.
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
| `DynamicNone` | written | out | as Nothrow | `throw()`: non-throwing, as Kiln's canonical form has it |
| `NothrowAttr` | flag | in | tag 4 |  |
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
`Nothrow` (for `Nothrow`, `DynamicNone`, `BasicNoexcept` and a
`NoexceptExpr` whose value is `True`); `None` for a `NoexceptExpr` whose
value is `False`; `DependentNoexcept` with the profile for a `NoexceptExpr`
whose value is `Dependent`; `NothrowAttr` (kept apart: the attribute is not
part of the function type as `noexcept` is); `Dynamic` with its types.

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

## Tests

To be completed with the last group of tables.

## Open questions

To be completed with the last group of tables.
