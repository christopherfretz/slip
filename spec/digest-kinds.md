# digest-kinds — the digest's tables

Status: draft (M1). Implemented by `slip_digest`. This file is the
per-kind and per-field companion of [digest.md](digest.md): for every
enumeration Kiln's AST uses and every stored field of every AST record,
payload, side record and cold map it says whether the fact enters the
digest and how. `digest.md` defines everything the tables plug into (units,
keys, streams, references); section numbers written "digest §N" are
sections of that file. The Kiln revision described is the one the `kiln`
submodule pins (`60fb5ab9d`, Kiln's `main`, since WP-163,
`spec/landed/digest-kinds/WP-163.md`; before it `4a83f6e3c`, WP-066; first written against `4bfd999da`); the facts were first collected in
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
| | `view` | a decoded form Kiln's API returns, whose rows are its variants; it may have no descriptor. A decoded form that is a record is `struct` with `pin=none` |
| | `consts` | a newtype whose values are associated constants; the rows are the constants |
| `pin` | `schema` | the type is reached from `kiln_ast::ast::Ast`'s descriptor |
| | `extra` | the type derives `Snap` but is decoded from a packed word, so `Ast`'s descriptor does not reach it; the test registers it by name, and `slip_digest` pins its `SCHEMA_HASH` like any other |
| | `none` | the type has no descriptor; nothing pins it and it is listed in "Open questions" |
| `selector` | a tabled enumeration's path | the record has per-kind words; rows may be prefixed by a variant of this enumeration |
| `base` | a number | added to the row number to give the variant's tag (§1.4) |

The test checks every value of `pin`.

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
`unsupported` row may say `unbuilt` (no construction site was found in Kiln,
or for an attribute kind none that attaches it, §10.1; the row needs no
test) and an `out` variant may say `as V` (§1.4). For an
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
  `TypeView` 1024, `ValueKind` 1280; the walk's own rows 1536, §1.7), so
  a statement can never be read as an expression.
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
the names and fails for a missing directory or a directory no row names
(digest §13 obligation 5, §14).

A test's files (`t.cpp`, or `a.cpp` and `b.cpp`), its header lines
(`// std:`, `// errors:`, `// target:`, `// args:`) and the command line
each side is parsed under are those of `spec/harness.md` §6.1. What its
`expect.txt` says, and which lines each of the four directories reads,
is digest §14.1: an `in` row's test names the unit that differs
(`changed`, `added`, `removed`) and may name the derived units that move
with it (`derived-added`, `derived-removed`, `derived-changed`, all of
them under `exact derived`); a `demand` row's test may name the
instantiations the use forces (`derived-added`); an `out`, `symbolic` or
`unsupported` row's test names no derived unit.

### 1.6 What the test cannot see

The descriptors show fields and variants. They do not show the meaning of
a per-kind word, a bit of a flag word, or a value of a `consts` type. Every
row with a `kind` prefix or a `fact` suffix is therefore outside
obligations 3 and 4 of digest §13, and the words concerned are listed in
"Open questions" as obligation 6 requires.

### 1.7 Rows of the walk

A few things the walk writes are no Kiln type's row: they are the walk's
own. Their tags are the next range of §1.4 after the node enumerations',
1536: a row's tag is 1536 plus its position in this table, from 1, so no
statement, expression or other node can be read as one. The test checks
the numbers and that no table's range reaches them, and `src/tables.rs`
gives each as a constant of `walk_row`.

| Tag | Row | Written |
|---|---|---|
| 1537 | `backref` | in place of a statement or expression node that the stream has written already in the same state: `tag 1537`, then `uv` the node's count (digest §4.3 "Back-references"); the node's rows and children are not written again |
| 1538 | `enclosing` | at the end of the stream of a unit at namespace scope (§5.1 item 6): what the entity takes from its enclosing namespaces other than their names (rule below) |

The row `enclosing` is written for the unit (not a declaration walked
in place, §2.4) of every entity that is not a namespace, not
content-keyed (digest §3.3) and not of a key kind a scope may repeat
(a using-declaration and its shadows, a namespace alias, `using enum`, a
friend declaration: `units::repeatable`), and whose representative's (the
declaration the walk writes the unit from, `Walk::representative`)
semantic context, through linkage
specifications and `export` declarations, is a namespace or the
translation unit, and for a block-scope `extern` declaration's entity,
which is keyed in its namespace (§2.4): `tag 1538`, then

1. `enum Linkage`: the linkage of the entity's name, Kiln's
   `Ast::linkage` of the representative ([basic.link]/3-4);
2. `bool`: an unnamed namespace encloses the entity, at any depth
   (`Ast::in_anonymous_namespace` of the representative);
3. two `opt<uv>`, the visibility pair that every declaration of the
   entity shares (item 4): the `VISIBILITY.a` of the nearest namespace
   opening around the declaration that has a `visibility` attribute, and
   the `a` of the nearest that has a `type_visibility` or a `visibility`
   attribute (the first if it has both); 0 default, 1 hidden,
   2 protected, another value indeterminate. Absent when no namespace
   is around the declaration, or none around it has the attribute.
   "Around" is the walk clang makes for the declaration's semantic
   context: it starts at an opening of the declaration's namespace,
   which is, for a declaration instantiated from a pattern (a friend
   defined or declared in a class template, a friend function template:
   `Ast::instantiated_from`, through every level, for a template its
   pattern's), that of the pattern, never one found by the position of
   the instantiation (clang++-19 gives such a friend the opening its
   class template is written in, whatever openings of the namespace come
   between; `enclosing/friend-instantiated-after-plain-reopening`), and
   for any other declaration
   - the opening the declaration is written in (its lexical context,
     through linkage specifications and `export`), when that is an
     opening of its semantic namespace;
   - otherwise (a declaration written qualified outside its namespace:
     `int n::f() {}`, `struct n::S {…};`, a friend `int n::f(S) {}`, an
     explicit specialisation `template<> int d::g<int>() {}`, or
     `int a::b::f() {}` written in an opening of `a`), the latest opening
     of the namespace written before the declaration (by `DeclId`, which
     Kiln gives in the order declarations are written); where there is
     none the unit is indeterminate (`enclosing/opening`);
   - for a block-scope `extern` declaration, the function it is written
     in;

   and goes outward, a namespace opening's next being the opening it is
   written in (its lexical context), any other context's next its
   semantic context. The attribute is the opening's own (the
   declaration of the namespace that the walk meets), not one of
   another opening of the same namespace. Kiln's code generation starts
   at the semantic context instead (`kiln_codegen/src/visibility.rs`
   `ns_lv`, `parent_ctx`), and reads the first declaration where clang
   reads the most recent (`spec/landed/digest/WP-113.md`, "For Kiln");
4. the pair is computed for each of the declarations of item 5, and
   the pairs must be equal: a symbol's visibility is that of the
   declaration most recent where it is defined or used (clang++-19: an
   undefined reference takes the declaration most recent at the use), so
   when they differ it is a fact of each use's place, which no unit
   holds, and the unit is indeterminate (`visibility_by_use`, digest
   §10). The one pair they share is written once: written per
   declaration it would add only their number, which no symbol depends
   on;
5. the declarations whose pairs are compared and written are those of
   the chain (`Walk::redecls`) of the entity that governs the symbol's
   visibility, as clang++-19 gives it (`spec/landed/digest/WP-113.md`,
   "Specialisations"), each filter below applied only when it keeps at
   least one declaration (when it would keep none, all the declarations
   it was given count: a chain made only of declarations a filter drops,
   such as an explicit specialisation known only through the declaration
   Kiln made for it, or an entity declared only as an instantiated
   friend):
   - a specialisation of a class template or of a variable template, of
     any kind (explicit, partial, implicit or explicit instantiation),
     a member of one, and an instantiation of a function template take
     the template's: the opening a specialisation is written in does
     not change its symbols;
   - of a variable template (its own, or the one a specialisation
     takes) only the first declaration counts;
   - an explicit specialisation of a function template takes its own
     declarations, without the declaration Kiln makes for it from the
     template (at the location of a declaration of the template or of
     its pattern);
   - a class or an enumeration that this translation unit defines takes
     its definition alone, wherever it is used, before the definition
     too;
   - for a function or a variable this translation unit does not define,
     an absent visibility is read as 0 (default) before the pairs are
     compared and written: an undefined reference takes an opening's
     attribute or default, never `-fvisibility`;
   - a declaration instantiated from a pattern (above) that is not the
     entity's definition (a friend declared in a class template) counts
     only when the entity has no other declaration (clang++-19: `int
     h(int);` in a plain opening and `friend int h(int);` in an
     instantiated class template of a hidden one give `UND DEFAULT`;
     `enclosing/friend-declared-beside-namespace-declaration`).
     A declaration the compiler makes (`IMPLICIT`) counts as any other:
     Kiln makes none at namespace scope that is chained with a written
     one in an opening of another pair (the global allocation functions
     are at file scope; `enclosing/implicit-align-val-t-beside-written`),
     and one would make the unit indeterminate, never a wrong equal.

A namespace's unit has no such row: a namespace has no symbol, its own
linkage is read by reflection alone (`has_internal_linkage(^^n)`), whose
result is a value of the unit that asks, and its unit is one for every
namespace a rule merges onto it (`spec/verify.md` §2.1). A
content-keyed unit (a `static_assert`, a using-directive, file-scope
`asm`, a `consteval` block) declares no entity with linkage or a
symbol.

Every name a key holds is rewritten by a rule; what this row holds is
not, so a relocation that changes it changes the unit
(`spec/landed/digest/WP-113.md`). An entity inside a class or a
function takes all of it from that class or function, whose unit holds
the row.

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

A `ref` to a namespace is `Ref::unit` of its key, but at the sites of
digest §3.4 "A reference to a namespace" (WP-141): the scope of a
using-declaration in its key, the namespace qualifier of an unresolved
name (`UnresolvedLookupExpr`, in a stream and in a profile) and of a
qualified concept-id (`ConceptSpecializationExpr`) can be
`Ref::views` (6: the count, then the view keys ascending); the target of
a using-directive at namespace scope and of a namespace alias can be
`Ref::part` (7: the key, then the view as a `bool`, `true` owned). Both
need a namespace rule and a namespace of the image with two view keys;
neither is ever written for a pre-image.

### 2.3 Redeclarations

A unit is an entity, not a declaration (digest §2.1). Its stream is
written from the entity's **defining declaration** if it has one, else
from its first declaration, with these facts merged over every
declaration of the chain: attributes (the union, as §10.1 orders it),
default arguments and default template arguments (each from the
declaration that gives it), the exception specification (from the
declaration the walk reads, §11.2), `inline`, `constexpr`
and `constinit` (set if any declaration sets them) and language linkage
(from the first declaration). A row merged in another way
says so in its note.

### 2.4 Declarations walked in place

`decl` applies digest §4.3 to a declaration met inside a unit:

- a variable, parameter, binding or label has the ordinal it got when the
  walk first met it, by this declaration or by an earlier reference (a
  `goto` before its label). At its declaration, and not at a reference, its
  ordinal and then its rows are written;
- a local class, enumeration or closure type is written whole under the
  key `local(n)` of its owner (digest §4.3): `uv` `n`, the type's own
  stream as a unit of its kind would have it, then one stream for each
  member its lists name (`RecordData` `members-fields`,
  `members-virtual`, `members-other`, in that order and in the order
  of each list; an enumeration's enumerators are in its own stream).
  Implicit members and the specialisations of member templates are not
  written here: one that is used is a derived unit. A closure met a
  second time writes `n` alone;
- a local typedef or alias is written as its rows (its aliased type with
  the list that type's spelling holds, §4.4 "Spellings", rule 4): it has no
  unit of its own, and what lookup found in its aliased type is the
  function's meaning even when nothing uses the alias;
- a using-declaration, a using-directive and an empty declaration write
  nothing: their effect is in the references resolved through them;
- a block-scope declaration of a function or of a variable with linkage
  (`DeclMisc` `misc-local-extern`) is written as a `ref`: the entity is a
  unit of the namespace it belongs to. A function's `ref` is followed by a
  count and, for each parameter, what that declaration's parameter holds
  that the entity's unit leaves out: its default argument (`child?`), then
  its attributes, as one `attrs` list (§10.1: the count and the nodes of
  its own `decl_attrs`, `inherited` copies left out; no `opt`, no
  parts), and its annotations (§10.5), its own alone
  (`spec/landed/digest-kinds/WP-026.md`, `WP-100.md`);
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

A class's constructors and destructor are named `constructor` and
`destructor`, never by the class name: renaming the class changes their
parent key and nothing else.

### 3.3 Discriminators

| KeyKind | `disc` is the digest of |
|---|---|
| `deduction_guide` | the `Ref` of the template deduced, then as `function` |
| `function` | parameter types in order (`type*`), variadic, cv- and ref-qualifiers, explicit-object flag, trailing requires-clause (`profile?`); then, only for a friend whose parent is the class that befriends it (a member-like friend, or a friend every declaration of which is in a templated class), `uv` 1, so that it never has the key of a member of that class with the same name and signature; absent for a function with C language linkage |
| `field` | for a name-independent member (P2169: Kiln's `DeclMisc` `name_independent`, a member named `_` in C++26), `uv` its ordinal: the number of members before it, in member order (`Ast::members`), that have its `DeclKind`, its name and the name-independent bit. The members are those of the definition of the record it is a member of (for a member of a class template that is the pattern, for an instantiation's member the instantiation's own members), else, when it is not among them, those of the definition of that record's entity. An `IndirectField` counts only among `IndirectField`s and a `Field` only among `Field`s. If it is in neither list (no case is known; it would be an AST whose member is not in its parent's members) the ordinal is `uv` 2^64−1 and the unit is indeterminate, cause `Unaccounted("key/name-independent")`. Absent for any other member |
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
A bare `TypeId` is digested as the unqualified `QualType` of it. §4.2
rule 9 is the one exception: a type through whose sugar layout reads a
typedef carrying an alignment.

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
   and printing. They are `out`: the profile is the fact (§4.4), and the
   bytes of a profile are a function of the profile, not of the spelling
   Kiln met first. A fact a spelling holds and the profile does not (the
   declarations lookup found for an unresolved name or a dependent
   operator, the functions a resolved `new` or `delete` chose) is not in
   the type: the unit that writes the type writes it, read from its own
   spelling, in the list at the end of its stream (§4.4 "Spellings").
2. **Alias template specialisations** are sugar; `alias_converted` holds
   their converted arguments and is `out`. The type the alias names is what
   is digested.
3. **Exception specifications before C++17.** With
   `TypeOptions::noexcept_in_type` false a canonical function type has
   `ExcSpec::None` and `exc_protos` keeps the written specification of each
   non-canonical function node. A type digest is taken from the canonical
   type in every language mode. The exception specification of a
   *function declaration* is a field of the function's unit (§5.3,
   `DeclNode`, `ty-exception-spec`), read from the declaration's own
   written function node (`Walk::own_proto`): before C++17
   `TypeContext::proto` of the declared type answers with that node's
   written specification; from C++17 on it answers with the canonical
   prototype, whose dependent `noexcept` is the profile of the first
   spelling (`ExcSpec::DependentNoexcept`), so `own_proto` takes the first
   function node met along the declared type's sugar
   (`TypeContext::proto_of`, `desugar_once`), which holds the operand as
   written (`ExcSpec::NoexceptExpr`). The specification is compared in
   every language mode, whether or not the type holds it.
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
   `DependentDecltype` the operand is walked in profile mode (§4.4),
   where every `ParenExpr` is written (§7 "Parentheses", `ExprNode`,
   `ParenExpr.a`).
8. **Name-derived arrays** (digest §9). The `ty` of a `PredefinedExpr`
   that holds its value (`a` present) is `const char[N]`, `N` the length
   of a name plus one. As the operand of an implicit `ArrayToPointerDecay`
   it is written as the type `const char[]`: the stream `Quals`, tag
   `IncompleteArray`, the element type, which is the digest that type
   has, so no program can tell the two apart and none needs to (an array
   of unknown bound is never a `PredefinedExpr`'s type). A pointer value
   into such a string designates the array the same way (`Scalar`
   `Pointer.designated`, `PtrBase` below). Anywhere else, and where the
   walk writes name-derived values by value (digest §9: a key, a type, a
   `static_assert` condition and a block-scope variable's value read
   there, the operand of a `ConstantExpr` with no stored value), the type
   is written by value.
9. **Typedefs carrying an alignment** (`spec/landed/digest/WP-107.md`).
   Kiln's layout and `alignof` read the alignment of a typedef that
   carries one (`Layouter::type_info`, `kiln_abi/src/layout/mod.rs:365`,
   its `Typedef` arm), which the canonical type has lost. A typedef
   *carries an alignment* when any of its declarations has an `ALIGNED`
   attribute or an entry in `DeclArena::cold.alignment`
   (`max_align_attr`, `kiln_abi/src/layout/props.rs:78`).

   The typedefs of `q` that layout reads are found by `type_info`'s walk
   from `q`:
   - from a `Typedef`, its `underlying`;
   - from a `QualifiedArray` or an `ExtQual`, the base;
   - from an array of any of the four kinds, or a `Complex`, the element;
   - from an `Atomic`, the value type;
   - from an `LValueRef` or `RValueRef`, the referee. `alignof(T&)` is
     `alignof(T)` ([expr.alignof]/3); Kiln reads the canonical referee
     there, so this writes one fact more;
   - from any other view, one step of `Ast::layout_desugar_once`.

   The walk stops at a canonical type, whose parts are canonical, or when
   a step returns its argument. Every typedef met that carries an
   alignment is listed, in the walk's order. When the list is empty,
   `q`'s digest is §4.1's. Otherwise the stream is:
   1. the tag of `Typedef`;
   2. the list, as `ref*`;
   3. the digest of `TypeContext::canonical(q)` (`digest`).

   After 4096 steps the walk stops, and the type is indeterminate:
   `Unsupported("TypeView/Typedef-aligned-bound")`.

   A listed typedef whose declarations do not all carry the alignment
   of its first declaration makes the type indeterminate:
   `Unsupported("typedef/alignment-by-declaration")`. The typedef's own
   unit is indeterminate for the same cause. Each declaration's requests
   are read: each `ALIGNED` attribute's value, the target's biggest
   alignment for one with no argument, the canonical type of
   `alignas(type-id)`, and the `cold.alignment` value. The declarations
   agree when every declaration's requests are among the first
   declaration's (`Ast::first`), and no request's value fails to be a
   constant the AST holds (read as Kiln's layout reads it). A single
   declaration agrees. Since Kiln WP-2830 (`8e23911be`) a declaration's
   alignment is its own requests and those of the declarations before
   it (`Ast::self_and_prev`, `kiln_ast/src/ast.rs:1132`), for `alignof`
   (`Ast::layout_attributed_typedef`, `kiln_ast/src/layout_sugar.rs:72`)
   and for layout (`max_align_attr`, `kiln_abi/src/layout/props.rs:89`),
   as clang's `mergeAlignedAttrs`: no longer by when a class is first
   laid out. When the declarations agree, every declaration has the
   first one's alignment, wherever a use names it. When a later
   declaration adds a request, a use before it and a use after it have
   different alignments (clang too), which depends on the declaration a
   use names, a fact no unit holds; the digest does not try to say
   which. A later request that is not among the first's but no
   stricter (`aligned(16)` then `aligned(8)`) is still taken to
   disagree: comparing it would need the layout of a type request.

   What is written depends on the sugar a typedef `I` carrying an
   alignment is reached through.

   The list holds `I` when `I` is reached through:
   - a typedef or alias-declaration naming `I`;
   - `decltype` of an entity of type `I`;
   - an elaborated name, or `using`;
   - a deduced type (`auto x = i;`, whose type keeps `I` in Kiln and in
     clang);
   - an alias template's argument (`A<I>`). Kiln keeps it, as clang 22
     does; clang 19 makes it canonical;
   - a cv-qualified type or an array of `I`, or an array typedef of `I`.

   Nothing is written in these cases:
   - A class, function or variable template's substituted argument
     (`T` inside `X<I>` or `f<I>`): `layout_desugar_once` makes it
     canonical, as clang instantiates from canonical arguments. A class
     template's arguments are stored canonical; a function template
     specialization keeps its first use's sugar in Kiln, which layout
     does not read.
   - A pointer, a member pointer, a function type and a vector are not
     walked: their layout reads no typedef's alignment. Where a use reads
     the pointee's or the result's alignment (`*p`, `decltype(f())`,
     `p->*mp`), the expression's type, with the sugar Kiln gives it, is
     written in the using unit, so the pointer and the function need not
     be. (Kiln gives `.*` and `->*` the canonical pointee:
     `spec/landed/digest/WP-107.md`, "For Kiln".)

   Only the outermost listed typedef whose alignment is not 0 decides.
   Listing the others is a false difference, never a wrong equal.

   A typedef is written as its entity, so a rule maps it like any other
   reference. The alignment it carries is a fact of its own unit (the
   typedef's attributes). A class whose field is of type `I` holds `I`'s
   `Ref`, and its layout follows from `I`'s unit, as from a field's class
   type.

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
| `Decayed` | flag | out | - | stored discriminant; `TypeView` has a variant of the same name |

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
| `Typedef` | type | in | tag 1048 | §4.2 rule 9 only: a type through whose sugar layout reads a typedef carrying an alignment; elsewhere sugar, the alias a type was reached through (digest §1 rule 2), not written |
| `Typedef.decl` | entity | in | ref* | §4.2 rule 9: every typedef carrying an alignment that the walk meets, in its order |
| `Typedef.underlying` | type | in | type | §4.2 rule 9: the canonical type of the whole type, not this field |
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
| `Auto.deduced` | flag | in | bool | true: the placeholder of a templated entity not yet deduced, whose `deduced` is the builtin dependent type; any other deduced type is sugar (the canonical type is the deduced type) and is not written |
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
| `Decayed` | written | out | - | sugar (Kiln WP-2720, `kiln_ast/src/types/context.rs:219`): a parameter type adjusted from an array type whose bound is an instantiation-dependent expression; canonical is the adjusted pointer type. The declared array type keeps the bound as written, which §4.4 "Spellings" rule 2 reads |
| `Decayed.original` | written | out | - | sugar: the declared array type |
| `Decayed.adjusted` | written | out | - | sugar: the adjusted pointer type |

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
words. `profile` is the expression `ProfileEntry.first_expr`
(`TypeContext::profile_expr`) walked in **profile mode**, inside the type
or key stream being written. Profile mode is also how every expression
written into a key is walked (a function's trailing requires-clause,
§3.3, and a template head's requires-clause, §3.4).

**Profile mode writes a fact iff Kiln's profile distinguishes it**
(`Ast::profile_node`, `kiln_ast` `types/profile_expr.rs`, [temp.over.link]/5):
two expressions Kiln profiles alike give the same bytes whichever was met
first, and two it tells apart give different bytes. The two tables below
are the whole encoding; the line numbers are those of `profile_expr.rs` at
the pinned Kiln, and every row is pinned by a sample of
`slip_digest` `expr_profile_tests::the_bytes_are_pinned` (golden
`tests/golden/profile_bytes.txt`, `SLIP_BLESS=1` rewrites it; the test
fails on a row without a sample and on a sample no row names).

**Notation.** `uv`, `bool`, `opt`, `count`, `tag`, `flags` are §2's;
`flags(a, b)` has `a` in bit 0. `type` is the §4.1 type digest (and its
mark, below), `type?` is `opt` and `type`, `ref` a §4.3 `Ref`, `sym` an
identifier, `name` a §3.2 `Name` (of a raw `DeclName`, as a declaration's
name is written). `tag K` is the row tag of `K` in §7's `ExprKind` table;
`aux T` is the row tag of the node's `aux` decoded as `T` (§7's tables),
or `uv 0` and `Unaccounted("profile/<kind>.aux")` when it does not
decode. `child` is the profile-mode bytes of an operand, `child?` is `opt`
and `child` when present, `list` is `count` and each `child`. A missing
operand Kiln's node must have is `uv 0` and `Unaccounted`.

**Order.** A node writes the fields of its row in the order given. A
`child` is written whole before the next field, with one exception, which
mirrors Kiln's stack: the per-argument entries of `args each` follow the
rest of their node's fields that are not children (in
`CXXDependentScopeMemberExpr`, after `spelled-nns`), and precede its
children.

#### Forms

| Form | Kiln | Bytes | Sample |
|---|---|---|---|
| `decl template-parm` | `:327-344` | `uv 1`, `uv` depth, `uv` index, `bool` pack: a template type or template template parameter | `decl-template-parm` |
| `decl nttp` | `:327-344` | `uv 2`, `uv` depth, `uv` index, `bool` pack, `nttp-type` | `decl-nttp` |
| `decl parm` | `:345-362` | `uv 3`, `uv` index, `flags(pack, in-requires)`, `uv` function-scope depth, `type?` of the parameter. In-requires: the parameter's context is a `RequiresExprBody` | `decl-parm`, `decl-parm-requires` |
| `decl result-name` | `:363` | `uv 5`: a contract's result name | none: in a postcondition, which no profile holds |
| `decl other` | `:364` | `uv 4`, `ref` of the first declaration (also a template parameter Kiln holds no position for) | `decl-other` |
| `nttp-type placeholder` | `:375-382` | the canonical type is an undeduced placeholder (`auto`, `decltype(auto)`, its constraint ignored): `uv 1`, `tag` `AutoKeyword`, `bool` pack, `uv` qualifier bits | `nttp-type-placeholder` |
| `nttp-type type` | `:384` | otherwise: `uv 0`, `type?` | `decl-nttp` |
| `args none` | `:415` | no explicit arguments: `uv 0` | `decl-other` |
| `args list` | `:418-420` | the canonical list holds no expression without a profile: `uv 1`, §4.6 `enum ArgView*` of the canonical list | `args-list` |
| `args each` | `:421-431` | otherwise: `uv 2`, `count`, then per argument `uv 0` and the expression's `child`, or `uv 1` and §4.6 `enum ArgView` | `args-each` |
| `ref-args` | `:393-406` | `uv 0` when the written list is present, not instantiation-dependent, and the referenced declaration is neither a function template nor a variable template; otherwise `args` of the written list | `ref-args-implied`, `args-list` |
| `nns?` | `:322` | `opt`, §4.6 qualifier | `DependentScopeDeclRefExpr` |
| `spelled-nns` | `:190-224` | `count` of levels, then each level outermost first: global `uv 1`; namespace `uv 2`, `decl-name-or-ref` of the namespace; namespace alias `uv 3`, `decl-name-or-ref` of the alias; a type written without `template` whose name is one identifier (a template type parameter, a record, an enum, a typedef or using-declaration, through elaboration without keyword or qualifier; not a specialisation) `uv 4`, `sym`; an identifier `uv 5`, `sym`; any other type, or a splice, `uv 6` and §4.6 qualifier of that level | `spelled-nns`, `spelled-nns-namespace` |
| `decl-name-or-ref` | `:190-224` | a declaration named by an identifier: `uv 0`, `sym`; otherwise `uv 1`, `ref` | `spelled-nns-namespace` |
| `ctad` | `:303-312` | `opt`: present when the type is a deduced class template specialisation not yet deduced (or deduced as dependent), then §4.6 template name | `ctad` |

#### Kinds

`in full` is §7's walk of the node (`expr::child`), with the profile's
frame installed; `transparent` writes nothing of the node and is its
operand's bytes. These kinds are walked in full: Kiln profiles the first
group as unique (a word of the expression's own id, `:838-860`), and the
digest has no profile form yet for the second, whose Kiln form needs
reflection facts (`:792-837`).

| Kind | Kiln | Bytes | Sample |
|---|---|---|---|
| `ImplicitCastExpr` | `:450` | transparent: `child` of `a` | `ImplicitCastExpr` |
| `ConstantExpr` | `:451` | transparent: `child` of `a` | none: Kiln wraps a constant of a declaration (a bound, an enumerator, a bit-field width), a case label, a `__builtin_choose_expr` condition (walked in full) and an immediate call outside a template (`kiln_sema` `consteval.rs:948-954`), none of them in a profile |
| `MaterializeTemporaryExpr` | `:452` | transparent: `child` of `a` | `MaterializeTemporaryExpr` |
| `CXXBindTemporaryExpr` | `:453` | transparent: `child` of `a` | `CXXBindTemporaryExpr` |
| `ExprWithCleanups` | `:454` | transparent: `child` of `a` | none: built only for a full-expression, which a profile operand is not |
| `SubstNonTypeTemplateParmExpr` | `:455` | transparent: `child` of the replacement `b` | `SubstNonTypeTemplateParmExpr` |
| `OpaqueValueExpr` | `:456-461` | `child` of its source; with no source `tag K` | none: Kiln builds it only in non-dependent semantic forms a profile does not reach |
| `IntegerLiteral` | `:462-468` | `tag K`, `type`, then a value that fits 64 bits `uv 0`, `uv` value, else `uv 1` and §8 value | `IntegerLiteral` |
| `FloatingLiteral` | `:469-473` | `tag K`, `type`, §8 value | `FloatingLiteral` |
| `CharacterLiteral` | `:474` | `tag K`, `aux CharKind`, `type`, `uv` code point | `CharacterLiteral` |
| `StringLiteral` | `:476` | `tag K`, `type`, §8 string | `StringLiteral` |
| `CXXBoolLiteralExpr` | `:477` | `tag K`, `bool` value | `CXXBoolLiteralExpr` |
| `AddrLabelExpr` | `:479` | `tag K`, `ref` of the label | none: a label is in a function body |
| `CXXNullPtrLiteralExpr` | `:480-485` | `tag K`, `type` | `CXXNullPtrLiteralExpr` |
| `GNUNullExpr` | `:480-485` | `tag K`, `type` | `GNUNullExpr` |
| `ImplicitValueInitExpr` | `:480-485` | `tag K`, `type` | none: built for an aggregate's omitted members in a non-dependent initialiser after the profile is formed |
| `CXXScalarValueInitExpr` | `:480-485` | `tag K`, `type` | `CXXScalarValueInitExpr` |
| `DeclRefExpr` | `:488-497` | `tag K`, `decl`, `ref-args` | `decl-other` |
| `MemberExpr` | `:498-508` | `tag K`, `bool` arrow, `decl` of the member, `ref-args`, `child?` of the base | `MemberExpr` |
| `CXXThisExpr` | `:509` | `tag K`, `bool` implicit | `CXXThisExpr` |
| `PredefinedExpr` | `:510` | `tag K`, `aux PredefinedKind` | none: `__func__` and its kin are in a function body |
| `SourceLocExpr` | `:510` | `tag K`, `aux SourceLocKind` | `SourceLocExpr` |
| `UnresolvedLookupExpr` | `:511-523` | `tag K`, `bool` ADL (`aux` bit 0), then: when the first declaration found is a template template parameter (a variable template template parameter's `V<args>`), `uv 1` and its `decl`; otherwise `uv 0` and `name` (the declarations found are left out: "The mark"); then `nns?`, `args` | `UnresolvedLookupExpr`, `UnresolvedLookupExpr-ttp` |
| `DependentScopeDeclRefExpr` | `:524-527` | `tag K`, `nns?` of `a`, `name` of `b`, `args` | `DependentScopeDeclRefExpr` |
| `CXXDependentScopeMemberExpr` | `:528-552` | `tag K`, `flags(arrow, spelled)`, `name` of `b`, `nns?` (absent when spelled), `args`, `spelled-nns` when spelled, `child?` of the base. Arrow: a base is present and `aux` bit 0. Spelled: a base is present, is type-dependent, is not an implicit `this`, and a qualifier is present | `CXXDependentScopeMemberExpr`, `spelled-nns` |
| `SizeOfPackExpr` | `:553-561` | `tag K`, `opt` and `decl` of the pack | `SizeOfPackExpr` |
| `UnaryOperator` | `:562-566` | `tag K`, `aux UnaryOp`, `child?` | `UnaryOperator` |
| `ParenExpr` | `:562-566` | `tag K`, `child?` | `ParenExpr` |
| `PackExpansionExpr` | `:562-566` | `tag K`, `child?` | `PackExpansionExpr` |
| `CXXNoexceptExpr` | `:562-566` | `tag K`, `child?` | `CXXNoexceptExpr` |
| `BinaryOperator` | `:567-571` | `tag K`, `aux BinaryOp`, `child?` of `a`, `child?` of `b` | `BinaryOperator` |
| `ArraySubscriptExpr` | `:567-571` | `tag K`, `child?` of `a`, `child?` of `b` | `ArraySubscriptExpr` |
| `PackIndexingExpr` | `:567-571` | `tag K`, `child?` of `a`, `child?` of `b` | `PackIndexingExpr` |
| `CompoundAssignOperator` | `:572-575` | `tag K`, `aux BinaryOp`, `child?` of `a`, `child` of the right operand | `CompoundAssignOperator` |
| `ConditionalOperator` | `:576-580` | `tag K`, `child?` of the condition, `child` of each arm | `ConditionalOperator` |
| `CXXFoldExpr` | `:581-584` | `tag K`, `aux BinaryOp`, `child?` of `a`, `child?` of `b` | `CXXFoldExpr` |
| `CallExpr` | `:585-591` | `tag K`, `child?` of the callee, `list` of the arguments | `CallExpr` |
| `CXXMemberCallExpr` | `:585-591` | `tag K`, `child?` of the callee, `list` of the arguments | `CXXMemberCallExpr` |
| `CXXOperatorCallExpr` | `:585-591` | `tag K`, `aux OverloadedOperator`, `child?` of the callee, `list` of the arguments | `CXXOperatorCallExpr` |
| `PseudoObjectExpr` | `:592` | transparent: `child` of the syntactic form | none: built for properties and subscripts of other languages' objects |
| `CXXRewrittenBinaryOperator` | `:593-597` | `tag K`, `tag BinaryOp` of the written operator, `list` of the written operands | `CXXRewrittenBinaryOperator` |
| `BinaryConditionalOperator` | `:598-601` | `tag K`, `list` of `a` | `BinaryConditionalOperator` |
| `ParenListExpr` | `:598-601` | `tag K`, `list` of `a` | `ParenListExpr` |
| `InitListExpr` | `:598-601` | `tag K`, `list` of `a` | `InitListExpr` |
| `DesignatedInitExpr` | `:602-623` | `tag K`, `count` of the designators, each: a field `uv 1`, `sym`; an index `uv 2`, `child`; a range `uv 3`, `child` of each bound; then `child?` of the initialiser | `DesignatedInitExpr` |
| `CStyleCastExpr` | `:624-636` | `tag K`, `aux CastKind`, `tag ValueCategory`, `type`, `child?` | `CStyleCastExpr` |
| `CXXFunctionalCastExpr` | `:624-636` | as `CStyleCastExpr` | `CXXFunctionalCastExpr` |
| `CXXStaticCastExpr` | `:624-636` | as `CStyleCastExpr` | `CXXStaticCastExpr` |
| `CXXConstCastExpr` | `:624-636` | as `CStyleCastExpr` | `CXXConstCastExpr` |
| `CXXReinterpretCastExpr` | `:624-636` | as `CStyleCastExpr` | `CXXReinterpretCastExpr` |
| `CXXDynamicCastExpr` | `:624-636` | as `CStyleCastExpr` | `CXXDynamicCastExpr` |
| `BuiltinBitCastExpr` | `:624-636` | as `CStyleCastExpr` | `BuiltinBitCastExpr` |
| `CXXUnresolvedConstructExpr` | `:637-641` | `tag K`, `bool` list-initialisation (`aux` bit 0), `type` written, `ctad`, `list` of `a` | `CXXUnresolvedConstructExpr`, `ctad` |
| `CXXParenListInitExpr` | `:642-646` | `tag K`, `type`, `list` of `a` | `CXXParenListInitExpr` |
| `CXXConstructExpr` | `:647-651` | `tag K`, `type`, `decl` of the constructor, `list` of `b` | `CXXConstructExpr` |
| `CXXTemporaryObjectExpr` | `:647-651` | as `CXXConstructExpr` | `CXXTemporaryObjectExpr` |
| `CompoundLiteralExpr` | `:652-655` | `tag K`, `type`, `child?` | `CompoundLiteralExpr` |
| `CXXDefaultArgExpr` | `:656-659` | `tag K`, `decl` of the parameter | `CXXDefaultArgExpr` |
| `CXXDefaultInitExpr` | `:660-663` | `tag K`, `decl` of the field | none: a default member initialiser's use is in a constructor or an aggregate initialisation after the profile is formed |
| `ArrayInitLoopExpr` | `:664-667` | `tag K`, `child?` of `a`, `child?` of `b` | none: built for an implicit copy of an array member, never in a profile |
| `DesignatedInitUpdateExpr` | `:664-667` | `tag K`, `child?` of `a`, `child?` of `b` | none: built for a C designator that overrides an earlier one, not in C++ |
| `ArrayInitIndexExpr` | `:668` | `tag K` | none: as `ArrayInitLoopExpr` |
| `NoInitExpr` | `:668` | `tag K` | none: as `DesignatedInitUpdateExpr` |
| `CXXNewExpr` | `:671-684` | `tag K`, `flags(global, array)`, `tag NewInitStyle`, `type` allocated, `child?` of the array size, `child?` of the initialiser, `list` of the placement arguments (`uv 0` when none) | `CXXNewExpr`, `CXXNewExpr-braces` |
| `CXXDeleteExpr` | `:685-690` | `tag K`, `flags(global, array)`, `child?` | `CXXDeleteExpr` |
| `CXXThrowExpr` | `:691-695` | `tag K`, `child?` of the operand | `CXXThrowExpr` |
| `CXXPseudoDestructorExpr` | `:696-700` | `tag K`, `bool` arrow, `type` destroyed, `child` of the base | `CXXPseudoDestructorExpr` |
| `CXXTypeidExpr` | `:701-709` | `tag K`, then a type operand `uv 1`, `type`; an expression `uv 0`, `child` | `CXXTypeidExpr`, `CXXTypeidExpr-type` |
| `UnaryExprOrTypeTraitExpr` | `:710-717` | `tag K`, `aux TraitKind`, then a type argument `uv 1`, `type?`; an expression `uv 0`, `child?` | `UnaryExprOrTypeTraitExpr`, `UnaryExprOrTypeTraitExpr-type` |
| `TypeTraitExpr` | `:718-724` | `tag K`, `aux TypeTrait`, `count` and `type` of each argument | `TypeTraitExpr` |
| `ArrayTypeTraitExpr` | `:725-728` | `tag K`, `aux TypeTrait`, `type`, `child?` of the dimension | `ArrayTypeTraitExpr` |
| `ExpressionTraitExpr` | `:729-732` | `tag K`, `aux TypeTrait`, `child` | `ExpressionTraitExpr` |
| `IntegerPackExpr` | `:733-736` | `tag K`, `child?` | `IntegerPackExpr` |
| `RequiresExpr` | `:737-768` | `tag K`, `count` of the parameters and `type?` of each, `count` of the requirements, each: simple `uv 1`, `child`; type `uv 2`, `type`; compound `uv 3`, `uv` 1 when it is `noexcept` else 0, `child`, `child?` of the type-constraint; nested `uv 4`, `child`. A reference to a parameter is its `decl parm`. Without Kiln's `RequiresInfo`, in full | `RequiresExpr`, `RequiresExpr-compound` |
| `ConceptSpecializationExpr` | `:769-781` | `tag K`, then the concept: absent `uv 0`; a concept template parameter `uv 1`, its `decl`; another `uv 2`, `ref` of its first declaration; then `opt` and §4.6 `enum ArgView*` of the arguments | `ConceptSpecializationExpr`, `ConceptSpecializationExpr-ttp` |
| `CXXReflectExpr` | `:792-828` | in full (no profile form yet) | none: reflection is off in the pinned samples' language mode |
| `CXXSpliceExpr` | `:829-837` | in full (no profile form yet) | none: as `CXXReflectExpr` |
| `UserDefinedLiteral` | `:838` | in full (Kiln: unique) | none: Kiln builds a user-defined literal as a `CallExpr` (`kiln_sema` `literal_op.rs:332-339`) |
| `ImaginaryLiteral` | `:839` | in full (Kiln: unique) | `ImaginaryLiteral` |
| `EmbedExpr` | `:840` | in full (Kiln: unique) | none: `#embed` data is in an initialiser, not a profile |
| `UnresolvedMemberExpr` | `:841` | in full (Kiln: unique) | none: Kiln builds none (`kiln_ast` `builder.rs` has no constructor; a member call on a non-dependent object with dependent arguments is a `CallExpr` of a `MemberExpr`) |
| `SubstNonTypeTemplateParmPackExpr` | `:842` | in full (Kiln: unique) | none: built in an instantiation, which a profile of a pattern does not hold |
| `FunctionParmPackExpr` | `:843` | in full (Kiln: unique) | none: as `SubstNonTypeTemplateParmPackExpr` |
| `CXXInheritedCtorInitExpr` | `:844` | in full (Kiln: unique) | none: in an inheriting constructor's implicit definition |
| `CXXStdInitializerListExpr` | `:845` | in full (Kiln: unique) | none: needs `std::initializer_list`, which the samples do not declare |
| `OffsetOfExpr` | `:846` | in full (Kiln: unique) | `OffsetOfExpr` |
| `LambdaExpr` | `:847` | in full (Kiln: unique) | `LambdaExpr` |
| `CoawaitExpr` | `:848` | in full (Kiln: unique) | none: in a coroutine body |
| `DependentCoawaitExpr` | `:849` | in full (Kiln: unique) | none: in a coroutine body |
| `CoyieldExpr` | `:850` | in full (Kiln: unique) | none: in a coroutine body |
| `StmtExpr` | `:851` | in full (Kiln: unique) | none: in a function body |
| `ChooseExpr` | `:852` | in full (Kiln: unique) | `ChooseExpr` |
| `GenericSelectionExpr` | `:853` | in full (Kiln: unique) | `GenericSelectionExpr` |
| `VAArgExpr` | `:854` | in full (Kiln: unique) | none: in a variadic function's body |
| `AtomicExpr` | `:855` | in full (Kiln: unique) | none: an atomic builtin's operand needs a pointer to `_Atomic`, not in C++ profiles |
| `ShuffleVectorExpr` | `:856` | in full (Kiln: unique) | none: needs vector types |
| `ConvertVectorExpr` | `:857` | in full (Kiln: unique) | none: needs vector types |
| `ExtVectorElementExpr` | `:858` | in full (Kiln: unique) | none: needs `ext_vector_type` |
| `RecoveryExpr` | `:859` | in full (Kiln: unique) | none: built only after an error |
| `CXXExpansionSelectExpr` | `:860` | in full (Kiln: unique) | none: in an expansion statement |

The rest of profile mode:

- **The mark.** When the walk leaves out a fact a spelling may hold, it
  counts it (`Walk::left_out`), whatever the first spelling's value: every
  `UnresolvedLookupExpr` (an empty found set too), every expression with an
  `oper_lookups` entry (an empty set too), every `CXXNewExpr` and
  `CXXDeleteExpr`, a fold that rule 9 makes indeterminate, a resolved
  `DeclRefExpr` or `MemberExpr` whose referent is a member of a class or
  enumeration or a specialisation (rule 10: it may have been named through
  a qualifier or arguments that hold more), a qualifier with a level that
  names a type (rule 11), and an
  operation that may round (a `BinaryOperator`, `CompoundAssignOperator`,
  `UnaryOperator` or cast whose type is floating or dependent: rule 8;
  whether the first spelling's node has an `fp_round` does not count). The mark is so a function of the
  profile, not of the spelling Kiln met first. A type digest whose
  stream counted one is marked, and a memo hit of a marked type counts
  again, so every enclosing type is marked too. A key ignores the count
  and the mark. A key and a content digest are computed with the type
  digests' state put aside (`Walk::take_types`): what they count is not
  the enclosing type's, so a type's mark does not depend on whether a key
  it names was computed first. It decides three things: a unit that holds
  a canonical operand and no spelling of its own is
  `Unaccounted("spelling/<site>")` when the operand's type is marked
  ("Spellings", rule 5); so is a profile written by `ProfileId` into a
  unit stream (`spelling/profile`); and a conversion function's name whose
  type is marked is `spelling/conversion-name` (Kiln interns the name by
  the canonical type, so the name holds the first spelling only).
- **Spellings** (WP-041). What a profile leaves out is written by the
  unit that owns the spelling, from that spelling, not from the type's
  first one (`own_spelling.rs`):
  1. *The left-out facts of an expression*, in the order of profile
     mode's walk of it (pre-order, children in the order of the table
     above): per `UnresolvedLookupExpr` (not a template template
     parameter's `V<args>`) a `Found` entry, the set of declarations
     found (possibly empty); per expression with an `oper_lookups` set an
     `Oper` entry (possibly empty); per resolved `CXXNewExpr` or
     `CXXDeleteExpr` a `Chosen` entry. Types inside the expression are
     read as written (rule 2), an argument list and a qualifier as
     written; a parameter's and a template parameter's type are their
     declarations'. A kind walked in full has its facts in its bytes; a
     requires-expression without its body is rule 5's site `spelling/full`
     when the type around it is marked, and a kind walked in full is
     `spelling/full` when its own profile walk counts a fact (the mark of
     the node, not of the type around it).
  2. *The spellings of a written type*, through sugar, in the order of its
     structure, for a type that is instantiation-dependent: pointee,
     referent, element, pointee and class of a member pointer, the
     pattern of a pack expansion, a substituted parameter's replacement,
     an attributed type's modified type; a dependent array or vector
     bound, an address space and a pack index (each as written, `Profile`
     operands are rule 5: `spelling/array-bound`, `vector-size`,
     `addr-space`, `pack-index`); a constant array's bound as written
     (`ConstArray::size_expr`, rule 7); a function type's return type,
     parameter types and exception specification (`noexcept(e)`,
     `throw(T...)`; `spelling/noexcept` when canonical; a resolved
     `noexcept(e)` by rule 7). For a function declaration's own type the
     exception specification is not read (its unit walks it in full, read
     from the written node: §4.2 rule 3), and the declared parameters are
     not read from the type: each `ParmVarDecl`'s unit writes its type as
     written (`ParamData::original_ty`), since an adjusted parameter type
     holds the canonical element type; a `decltype`'s and a `typeof`'s
     operand (`spelling/decltype`, `spelling/typeof` when canonical); the
     template name and written argument lists of a `TemplateSpecialization`
     (with or without `underlying`), a dependent template specialisation
     and a placeholder's constraint; the types of a qualifier and of a
     template name's qualifier. A deduced type, a resolved pack index, a
     computed unary transform and a substituted parameter pack are
     `spelling/deduced` or `spelling/subst-pack`; a dependent `_BitInt`
     (whose width Kiln keeps only from the first spelling) is
     `spelling/bit-int`. A type that is instantiation-dependent and not
     dependent through an operand Kiln computed and does not keep as
     written (a `_BitInt` width, a constant array bound with no
     `size_expr`) is `Unaccounted("spelling/bit-int-width")` or
     `spelling/array-size`, marked or not.
  3. *Where a unit reads them.* (a) `Walk::absorb_type` in a unit stream
     (`in_key == 0`, `type_nesting == 0`, not inside a profile written
     into the unit), from the written type its caller holds;
     (b) `expr_dep::profile_expr` written straight into a unit stream
     (a template argument, a table row), from that very expression;
     `expr_dep::profile` of a `ProfileId` in a unit stream has only the
     first spelling, and is `spelling/profile` when it counted a fact;
     `Walk::write_args` in a unit stream, from the list as the unit holds
     it, and a default template argument, from the argument as written;
     (c) `templ::partial_spec_info`, from the partial specialisation's
     arguments as written (`PartialSpecInfo::args`). A canonical argument
     with no spelling is `spelling/arg`.
  4. *Sugar that names another unit stops the walk.* A typedef or alias
     (a `Typedef` type, a `Using` type naming one) and an alias template
     specialisation are an `Alias` entry with the `Ref` of the
     typedef-name, or of the alias template's pattern, when that one's own
     aliased type as written holds a non-empty fact (or a cause); its
     unit writes the facts. The walk does not descend into what it
     aliases; an alias template specialisation's own arguments are the
     unit's and are walked. A block-scope typedef or alias has no unit of
     its own: its declaration writes its aliased type, profile and list,
     into the enclosing unit (`local::write_decl`), and a use descends
     into the aliased type as written instead of an `Alias` entry.
  5. *What stays indeterminate.* A canonical operand where the unit has no
     spelling of its own, whose type is marked, is
     `Unaccounted("spelling/<site>")` with the sites named above
     (`spec/landed/digest-kinds/WP-041.md` lists them, a pair test for
     each site a source reaches, and the attempts at the others).
  6. *The list.* The unit's facts are collected in walk order and
     written once, after the unit stream (`UnitState::facts`,
     `units::write`, `Walk::content_hash`): `count`, then per entry
     `uv` kind (`1` `Found`, `2` `Oper`, `3` `Alias`, `4` `Chosen`,
     `5` `Profile`, `6` `Round`) and its payload: `ref*sorted` (sorted by
     the `Ref`'s bytes, duplicates removed, a using-shadow written as its
     target, as the full walk writes an `UnresolvedLookupExpr`) for `Found`
     and `Oper`, `ref` for `Alias`, `count` and `ref?` each (allocation
     then deallocation function) for `Chosen`, `bytes` for `Profile`, and
     for `Round` `uv` the reading's ordinal in the unit, `count`, then per
     node `uv` its pre-order ordinal in the reading and `tag` its
     `FpRound`. The list is written only when an entry is due: a non-empty
     set, a chosen function, an alias, a profile, a rounding. A unit with no
     such entry writes nothing more, so its bytes are those it had before
     spellings were written. The trigger is the unit's own spelling, not
     the mark: a unit whose own spelling found declarations where the
     profile's first spelling found none writes its list too (the mark
     alone would have hidden it, a wrong `equal`).
  7. *Instantiation-dependent values.* An expression a written type holds
     where the canonical type holds only its value (a constant array's
     `size_expr`, an argument of a `TemplateSpecialization` with
     `underlying`, a resolved `noexcept(e)`) that is
     instantiation-dependent and neither type- nor value-dependent has
     meaning the value does not hold (`sizeof(f(T()))` is `1` whatever `f`
     names). Its profile bytes are a `Profile` entry (taken with the
     spelling reading off, its cause absorbed), then the expression is read
     as written (rule 1). Kiln builds an instantiation-dependent constant
     bound without its `size_expr` and with the element's flags
     (`kiln_sema/src/decl.rs`), so nothing reaches such a bound: its pair
     tests are pending on Kiln (`For Kiln`).
  8. *Rounding.* Each reading of an expression by rule 1 has an ordinal
     in the unit (`UnitState::roots`); the nodes of the reading whose
     `fp_round` is not `FpRound::Dynamic` (Kiln's default) are a `Round`
     entry, by pre-order position. Every node of an expression in a unit is
     either in its bytes (walked in full) or read by rule 1, so every
     rounding mode a unit's spellings were written under is in its digest.
  9. *Folds.* A `CXXFoldExpr` over an operator other than `.*` finds its
     operator's candidates at instantiation, from the operands' types and
     the definition context. Since Kiln WP-2625 Kiln keeps the fold's
     definition-context set in `oper_lookups`, which is written and left
     out like a dependent operator's (rule 6; `OperLookups`); what the
     operator finds beyond it is still not read (`For WP-138` in
     `spec/landed/digest-kinds/WP-066.md`). In profile mode
     and in the full walk it is `Unaccounted("spelling/fold-operator")`,
     and it raises the mark, when some declaration in the translation unit
     that is not a member function (a function template's pattern
     included) is named for its operator, or for a comparison for any
     comparison operator (C++20 rewrites them)
     (`expr_profile::fold_left_out`). With no such declaration the
     definition context finds nothing in any spelling, and only members
     of the operands' classes, which the profile holds, can be found.
  10. *Prefixes and arguments of a resolved name.* A resolved name's
     qualifier as written (`DeclRefExtra` `qualifier`) and its own
     arguments when the referent is a specialisation (`template_args`)
     are read as written, in rule 1 (after the name, the qualifier
     outermost first, then the arguments) and in the full walk (§7
     `DeclRefExtra`, `own_spelling::name_parts`, outside a unit stream
     raising the mark instead): each type level by rule 2, each argument
     by rule 7 or rule 1. Their parts are read whatever Kiln's flags say;
     only what they hold beyond the referent reaches the list (a found set,
     an instantiation-dependent operand's profile), so requalifying a name
     that keeps its referent changes no digest. Profile mode raises the
     mark at every reference whose referent could have been named so (the
     mark above), a function of the referent and so of the profile.
  11. *Qualifiers of unresolved names and of declarations.* A qualifier the
     full walk writes in canonical form (`DependentScopeDeclRefExpr.a`,
     `TemplateTables` `expr_nns` of an unresolved name or a dependent
     member access, `ColdExprData` `member_overload` of an overloaded
     member access) is, in a unit stream, read as written into the list
     (`Walk::write_written_nns`): each level that names a type by rule 2
     (a `decltype` by its operand, a specialisation by its arguments, an
     alias by rule 4); a namespace, a namespace alias, `::` and a
     dependent identifier hold nothing beyond the canonical level; a
     level the reading does not know (a splice) is indeterminate,
     `spelling/nns-splice`. Outside a unit stream it raises the mark, as
     profile mode does for every qualifier with a level that names a
     type (a function of the canonical qualifier). A using-declaration's
     qualifier as written is read the same way into its unit's list (its
     key writes the canonical one). The canonical writer
     (`Walk::write_canonical_nns`) is called only where the canonical
     form loses nothing (`spec/landed/digest-kinds/WP-041.md`, the table
     of its callers).

     *Templated.* A declaration is templated when it, or a declaration
     on its semantic or its lexical context chain (each step taking
     either context, so that an out-of-line member is reached through its
     class and a friend defined in a class through the class it is
     written in), is a template, a template's pattern or a partial
     specialisation. Kiln records no template parameter lists on an
     out-of-line member: its semantic context is what tells it. A stream
     is templated when its unit (a content digest: its content entity)
     is, or while the walk is inside a generic lambda's body (its call
     operator is a template); a stream with neither is templated. A
     declaration whose chain cannot be followed to its end is
     templated. A form is in templated code when its stream is
     templated or, for a declaration's form, the declaration is: one
     question (`own_spelling::templated_at`), asked by every check
     below; no check asks either half alone (a lambda's or a local
     class's chain ends at the namespace its closure lives in, while its
     stream is the variable template's, the default argument's or the
     concept's). Only templated code can hold an instantiation-dependent
     operand, and there, where Kiln keeps no written form, the unit is
     indeterminate:
     - a `using enum` (`spelling/using-enum`): Kiln keeps the
       enumeration, not the qualifier or alias it was named through;
     - a qualified pseudo-destructor whose qualifier Kiln did not keep
       (neither `expr_nns` nor `pseudo_dtor_first`):
       `spelling/pseudo-destructor-qualifier`;
     - a destructor named by member access (`q->~X<..>()`): Kiln keeps
       the destructor, not the name as written: `spelling/destructor-name`,
       in the full walk and in rule 1's reading;
     - a friend whose befriended declaration, or unsugared class or
       enumeration type, is a class member or a template
       specialisation, or whose target is a function type (Kiln's form
       of `friend void h<..>();`): Kiln keeps no qualifier and no
       arguments: `spelling/friend-name`;
     - an explicit specialisation declared in a template (a member of a
       pattern): Kiln keeps its converted arguments, not the expressions
       written: `spelling/specialisation-arguments`.
     Off unit level each raises the mark instead.
  12. *Converted arguments of a dependent template-id* (WP-163 fix
     round 1). Since Kiln WP-3321 (`ea9a8e141`) the canonical type of a
     dependent class template-id holds a value argument converted
     (`It<T, n::k>` is `It<T, 0>`, `ArgView::Value`) where it held the
     expression's profile, which named the entity. The written argument
     is in the `TemplateSpecialization` sugar (`args`, its canonical form
     `underlying`): every position whose written argument is an expression
     and whose canonical argument is a converted value
     (`own_spelling::converted_args`, the one reader of such a list) is
     read as rule 7 reads an
     instantiation-dependent one, its profile a `Profile` entry, then the
     expression as written (rule 1), so two template-ids naming different
     entities of one value differ in the units that write them (member,
     parameter, return type, friend, base, alias, alias template,
     variable, variable template, nested argument, default template
     argument, requires-clause, local: `differ/_/converted-arg-*`). Kiln's
     conversion is not re-derived: the written and canonical lists are
     compared position by position. A dependent specialisation met with
     no sugar (its canonical type handed to the reading) that holds a
     converted value is `Unaccounted("spelling/converted-argument")`
     (`own_spelling::CONVERTED_ARGUMENT`): which entity was written is
     not known. No source the tests hold reaches it through a unit; the
     walk test `a_converted_argument_with_no_written_form_is_a_cause`
     meets it through a canonical type. The same reader takes two more
     places (fix round 2): a partial specialisation's own list, of a
     class or a variable template, is read from Kiln's list as written
     (`args_as_written`, Kiln WP-3016/3142) against its canonical list,
     position by position (`differ/_/converted-arg-partial-own*`; of a
     variadic template, whose canonical list holds its pack converted,
     `T, {0}`, the written arguments the pack covers are read against
     its elements when they are as many, and the unit is
     `spelling/converted-argument` when they are not, fix round 3,
     `differ/_/converted-arg-partial-pack-*`); and a
     declaration written outside a partial specialisation it is a member
     of (an out-of-line member function, static data member or nested
     class, `template<class T> void P<T, n::k>::f() {}`), whose
     canonical arguments hold a converted value, has its qualifier's
     template-id in no list Kiln keeps: the unit is
     `spelling/converted-argument` (`own_spelling::out_of_line_member`,
     `indeterminate/_/converted-arg-out-of-line-*`), never equal. The
     profile holds the written expression's form, so a redundant
     parenthesis (`It<T, (n::k)>` against `It<T, n::k>`) is a difference:
     a known false mismatch, the loud direction, and the profile of the
     pin before (`differ/_/converted-arg-paren`).
  A key reads no spelling: two declarations are one entity by
  [temp.over.link] whatever their found sets, and a difference in the
  sets is a difference of the units. No hazard of first spellings is
  accepted: where a unit's meaning depends on a fact its digest does not
  hold, the unit is indeterminate with a named cause (rule 5, rule 7's
  `spelling/array-size` and `spelling/bit-int-width`, rule 9,
  `spelling/profile`, `spelling/conversion-name`), and the mark that
  decides rule 5 is a function of the profile.
- **Kinds Kiln profiles as unique.** Kiln gives such a node a word of its
  own id, so two declarations whose signatures hold one (a lambda:
  `template<int N> void foo(const char(*)[([]{}, N)]);` twice) are two
  templates in Kiln ([temp.over.link]/5, [expr.prim.lambda.closure]/1).
  The digest walks the node in full, a function of its spelling, so both
  declarations have one key, and the key is indeterminate with cause
  `collision` (digest §10): never two entities merged in silence. This
  stays so when `local` lands (WP-040): the closure types are two types,
  but the key is written from the spelling, which is the same.
- If `first_expr` is absent the unit is indeterminate.
- A reference to a local entity declared outside the walked expression
  uses the unit's ordinal (digest §4.3) and a type digest that holds one
  is memoised for the unit only; in a key it is indeterminate. A use
  inside a nested profile or type counts for every enclosing one.
- **Owned expressions are not profiles.** The expressions a declaration
  owns outside its type (its requires-clauses, default arguments,
  `noexcept` and `explicit` operands, default template arguments) are
  walked in full by §7 in the declaration's unit; only the key holds
  their profile.

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
| `DependentOperator` | name | unsupported | unbuilt | nothing in Kiln builds one outside the canonicaliser, which only copies one it is given |
| `DependentOperator.nns` | name | unsupported | unbuilt | as `DependentOperator` |
| `DependentOperator.op` | name | unsupported | unbuilt | as `DependentOperator` |
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
| `Expr` | value | in | tag 6 | an expression argument Kiln never converted (rule below) |
| `Expr.expr` | child | in | child | in a unit stream the expression walked in full; inside a type, a key or a profile its canonical operand (`profile`, §4.4); rule below |
| `Expr.profile` | bookkeeping | out | - | absent wherever the walk reads `ArgView`: it reads canonical lists only, and `canonical_arg` turns an `Expr` with a profile into `Profile` |
| `Profile` | value | in | tag 7 |  |
| `Profile.0` | value | in | profile |  |
| `Pack` | value | in | tag 8 |  |
| `Pack.0` | value | in | enum ArgView* |  |

`Expr` (WP-123). A canonical list holds `Expr` with no profile where
Kiln kept the argument as it was written and never converted it to the
parameter's type: it is not value- or instantiation-dependent, so Kiln
gives it no profile (`profile_value_args`, `kiln_sema` `templ/actions.rs`,
profiles only dependent ones), and no template was chosen for it to be
converted against. The forms are the explicit arguments of a dependent
name in a template's body (`TemplateTables` `expr_targs`: `get<0>(t)`,
`this->template f<false>()`), which the unit's stream writes, and the
arguments of a befriended specialisation in a class template
(`friend struct It<T, true>;`), which Kiln does not profile and which is
written inside the type. The expression names entities (`get<n::k>`:
which `k`), so it is walked, never written by tag alone:

- Written straight into a unit stream (outside a type, a key and a
  profile; `Walk::write_args` under its own-spelling guard counts as
  the unit's), it is the unit's own expression, walked in full (`child`,
  digest §7), as the declaration's own default template argument is
  (`TemplateTables` `defaults`); the unit's reading of the written list
  (`own_spelling::left_out_of_args`) is unchanged.
- Inside a type, a key or a profile, it is written by its canonical
  operand (`profile`, §4.4): the bytes `Profile` would have if Kiln had
  profiled it, the form Kiln's own profile gives such an argument
  (`args each` above); inside a profile, in that profile's frame. A
  type digest puts the unit's frame aside, but a value-dependent argument
  of the type begins a profile of its own, so the frame is met inside a
  type too: a concept-id's list in `X<C<T, n::k>>`, as in
  `getb<C<T, n::k>>(t)` at the unit level and in `requires C<T, n::k>`
  in a key (the pairs `differ/_/argview-expr-concept-in-*`).
  Inside a type or a key the walk puts the unit's local state aside
  (holder, content, local types, scopes) while it writes, so nothing of
  the unit that met the type first is read: a rule-11 check then takes
  the conservative answer (`own_spelling::templated_at` with no holder),
  a function name a program reads (`__func__`) is
  `Unsupported("local/detached-scope-names")`, a lambda-expression is
  `local/lambda-in-type` as anywhere in a type, and a reference to a local
  object of the unit raises `local_uses`, so the type is memoised for that
  unit only (digest §4.2).

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
properties of the command line, and a digest is relative to its command
(digest §1 rule 8): only digests of one observer are compared.

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
| `pack_bounds` | bookkeeping | out | - | the constant array bounds as written that hold an unexpanded pack (Kiln WP-2720, `kiln_ast/src/types/context.rs:285`): the array type's `TypeFlags` carry it |
| `inst_bounds` | bookkeeping | out | - | the constant array bounds as written that are instantiation-dependent (Kiln WP-2720, `context.rs:292`): the array type's `TypeFlags` carry it, and §4.4 "Spellings" rule 7 reads the bound itself (`ConstArray` `size_expr`) |
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
   (§9.5);
5. the rows `points` and `explicit_defs` of `ModuleTable` (§10.7);
6. for a unit at namespace scope, and not for a declaration walked in
   place, the walk's row `enclosing` (§1.7).

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
| `CXXRecord` | unit, local | `class` | identifier, or digest §3.3; a closure type is always local (§10.4) |
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
  its `DeclKind`, unless its `SpecKind` is `Undeclared` (§9); one whose
  kind is `MemberOfSpecialization` has its ordinary key under the
  specialisation's key. A `PartialSpecialization` is a
  `partial_specialisation` unit.
- A block-scope function or `extern` variable declaration belongs to the
  entity's unit at namespace scope (§2.4).
- An implicit special member, an implicit comparison or an implicit
  deduction guide is a unit only if it has a body or some unit's stream
  holds a `Ref` to it; the walk finds that out from the streams it writes,
  not from `DeclFlags` (`0-used`, `0-referenced`). The same holds for a
  lazily declared builtin and for a `TemplateParamObject`.
- A unit under `Parent::c_linkage` is a function or variable whose first
  declaration is lexically inside a `LinkageSpec` of language `C`
  (`ScopeData`, `aux-lang`; a block-scope declaration looks through the
  functions around it), that is not a class member, and whose name has
  external linkage (not `static`, not a member of an unnamed namespace,
  not a namespace-scope `const` or `constexpr` variable without `extern`):
  only such names have a language linkage ([dcl.link]/1).
- An injected-class-name is not a unit and a `Ref` never names one.
- An entity with an effective kind (§11.10) is a declared unit when that
  kind is an explicit instantiation (declaration or definition) or an
  explicit specialisation, and a derived unit when it is
  `ImplicitInstantiation`. So the members of `template struct X<int>;`
  and of `extern template struct X<int>;` are declared units, all of
  them (fields, functions, static data members, member classes and
  enumerations, member templates), and are listed in their class, and a
  member class used before the explicit instantiation of its class is
  declared all the same.

### 5.3 Headers

### `kiln_ast::decl::DeclKind`

`kind=enum; pin=schema; base=256`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `TranslationUnit` | bookkeeping | out | - | the root scope: `Parent::root` |
| `Namespace` | flag | in | tag 258 |  |
| `NamespaceAlias` | flag | in | tag 259 |  |
| `LinkageSpec` | flag | out | - | transparent: the language is written by each declaration inside (`ScopeData`, `aux-lang`) |
| `Export` | flag | out | - | transparent: module ownership is written by the module table (`ModuleTable` `points`) for each declaration inside |
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
| `0-reserved` | bookkeeping | out | - | bits 5-7, zero: they held the module ownership until Kiln WP-2477, which keeps a declaration's owner in `ModuleTable` (`points`, §13); a declaration with one of them set is `Unaccounted("DeclMisc/0-reserved")` |
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
| `0-return-deduced` | flag | in | bit | in `definition` for a function of the definition class (§11.9) |
| `0-exc-spec-computed` | written | out | - | bit 25 (Kiln WP-2505): the exception specification was computed from a deferral (an implicit or defaulted special member, a defaulted comparison, an inheriting constructor), not written. Sema sets it when it resolves the deferral, which use may trigger; the resolved specification is the fact (§11.2). Kiln's only readers are its module images and `Ast::exception_spec_computed`, which no part of code generation or of sema's decisions calls |

### `kiln_ast::decl::FnStorage`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `None` | flag | in | tag 1 |  |
| `Static` | flag | in | tag 2 |  |
| `Extern` | flag | in | tag 3 | written with the tag of `None` unless the function is inline: `extern` on a function changes nothing but what `inline` means under C99 and `gnu_inline` rules, which code generation reads |

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
| `original_ty` | type | in | type? | the type before an array or function type is adjusted to a pointer; present when it differs from `DeclNode` `ty-type`. Instantiation and the mangling of a dependent array bound read it |
| `index` | bookkeeping | out | - | the position in `FunctionData` `params` |
| `flags-explicit-object` | flag | in | bit |  |
| `flags-pack` | flag | in | bit |  |
| `flags-default-arg-pending` | bookkeeping | demand | - | §11.4: the default argument is not parsed or instantiated yet |
| `flags-static-array` | flag | in | bit |  |
| `flags-scope-depth` | value | in | uv | high byte: the function-prototype scope depth, the `level` of `Ref::param` |

A parameter has no redeclarations of its own. Its attributes (§10.1) and
its annotations (§10.5) are read from parameter *i* of each declaration
of the function's chain as §10.1 defines it (Kiln's `redecls_of`, first
declaration first, without the declarations in a templated body and,
unless all are, those instantiated outside their class), leaving out the
declarations at block scope; §10.1 keeps the defining declaration's
parameter apart. `default_arg` reads the same declarations, leaving out
also the instantiated ones (§11.4).

### `kiln_ast::decl::VarData`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `redecl` | bookkeeping | out | - |  |
| `init` | child | in | child? |  |
| `spec` | flag | in | record VarSpec |  |
| `eval` | value | demand | - | §11.3: the `resolved` unit `value`; a block-scope variable's in place |
| `templ` | entity | in | record TemplLink |  |

A variable's stream also holds its language linkage (`ScopeData`,
`aux-lang`).

### `kiln_ast::decl::VarSpec`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `0-storage` | flag | in | enum VarStorage | bits 0-2 |
| `0-tls` | flag | in | enum TlsKind | bits 3-4 |
| `0-init-style` | flag | in | enum InitStyle | bits 5-6; it selects the form of initialisation. In `definition` for a variable of the definition class (§11.9) |
| `0-constexpr` | flag | in | bit |  |
| `0-constinit` | flag | in | bit |  |
| `0-inline` | flag | in | bit | in `definition` for a variable of the definition class (§11.9) |
| `0-nrvo` | flag | in | bit |  |
| `0-exception-var` | flag | in | bit |  |
| `0-cxx-for-range` | flag | in | bit |  |
| `0-inline-implied` | flag | in | bit | as `0-inline` |
| `0-placeholder` | written | out | - | bits 14-15: the placeholder (`auto`, `decltype(auto)`, `__auto_type`) the type was deduced from (Kiln WP-3583, `kiln_ast/src/decl.rs:1102`, `VarPlaceholder`): the deduced type is the fact (the declaration's type row), and an undeduced placeholder is in the type (`TypeView` `Auto`). Reading it is `docs/wp/WP-156.md`'s |

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
| `value` | value | demand | - | §11.3: written once, by a strict evaluation whose result points at the temporary; written in the extending variable's `resolved` unit `value` |

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
| `0-defaulted-default-ctor-deleted` | flag | in | bit | "known deleted": set when the class is completed, whether or not the member is ever declared |
| `0-defaulted-copy-ctor-deleted` | flag | in | bit | "known deleted": set when the class is completed, whether or not the member is ever declared |
| `0-defaulted-move-ctor-deleted` | flag | in | bit | "known deleted": set when the class is completed, whether or not the member is ever declared |
| `0-defaulted-dtor-deleted` | flag | in | bit | "known deleted": set when the class is completed, whether or not the member is ever declared |
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
initialisation order. Kiln stores it only for a definition it checks
(`Sema::act_on_mem_initializers`, `check_mem_inits_of`); for a
constructor definition that is `INVALID`, whose class is
`DEPENDENT_CTX`, or that is `DEPENDENT_CTX` itself (made while a
template head was open), the written list is kept outside the AST, and
the stream that writes `ctor_inits` for it is indeterminate,
`Unsupported("kiln/pattern-mem-initializers")`, with a ctor-initializer
or without (`spec/landed/digest/WP-124.md`).

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
| `aux-braceless` | written | out | - | bit 8: `extern "C" int x;` without braces, which makes the declaration `extern` ([dcl.link]/8). For a function it means nothing; for a variable its effect is whether the declaration is a definition, which the variable's unit holds already (`DeclMisc` `definition`, §2.3), and its linkage, which the row `enclosing` of its unit holds (§1.7; `spec/landed/digest-kinds/WP-033.md`, `spec/landed/digest/WP-113.md`) |

Every function and variable stream holds one `enum LinkageLang?`: the
language of the innermost `LinkageSpec` that lexically encloses the
entity's first declaration, through enclosing classes and functions;
absent when there is none. Whether the entity then has C
language linkage (a class member has not, nor has an entity with internal
linkage) follows from the rest of its stream; the key's parent uses the
rule of §5.2.

### `kiln_ast::decl::LinkageLang`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `C` | flag | in | tag 1 |  |
| `Cxx` | flag | in | tag 2 |  |

### `kiln_ast::linkage::Linkage`

`kind=enum; pin=none`

The value `Ast::linkage` returns, written in the row `enclosing` of §1.7.

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `None` | flag | in | tag 1 |  |
| `Internal` | flag | in | tag 2 |  |
| `Module` | flag | in | tag 3 |  |
| `External` | flag | in | tag 4 |  |

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

### `kiln_ast::friend_form::FriendForm`

`kind=struct; pin=schema`

A friend's written form (Kiln WP-3164, `kiln_ast/src/friend_form.rs:113`,
Kiln `spec/ast.md` §23): kept for a friend named by a qualified name, a
template-id, a constructor, destructor or conversion function, or a
class friend with a template-head of its own. What the friend declares
is the friend declaration, its target (`FriendTarget`) and the class's
`friends` row; the form is how it was written. Where the written form
can hold an operand the target does not (an instantiation-dependent
qualifier or argument in templated code), the friend is indeterminate,
`spelling/friend-name` (§4.4 "Spellings"); reading the form there is
`docs/wp/WP-126.md`'s.

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `kind` | written | out | - | which form was written: it follows from the name and the target |
| `flags` | written | out | - | `FriendFormFlags` |
| `name` | written | out | - | the befriended name as written; the target is the fact |
| `name_loc` | location | out | - |  |
| `qualifier` | written | out | - | the qualifier as written, sugar kept; the target is the fact |
| `qualifier_loc` | location | out | - |  |
| `args` | written | out | - | a template-id's arguments as written; the target, a specialisation, holds the converted ones |
| `entity` | entity | out | - | what the written name refers to, when it is not the target: a candidate per specialisation; the friend declaration and its target are written |
| `found` | entity | out | - | the function templates lookup of a template-id's name found ([temp.friend]/1.3): the target is the specialisation chosen |
| `heads` | written | out | - | the friend's own template-heads, which the friend declaration holds and its unit writes |
| `requires` | child | out | - | the friend's trailing requires-clause, which the friend declaration holds (`TemplateTables` `trailing_requires`) |

### `kiln_ast::friend_form::FriendFormFlags`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `0` | written | out | - | a leading `::`, a `template` keyword, a friend of every specialisation (its own head deduces the qualifier's arguments, which the friend declaration's head holds), matched per specialisation, accepted with clang's `-Wunsupported-friend` warning: the form, read with `FriendForm` |

### `kiln_ast::friend_form::FriendFormKind`

`kind=enum; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Function` | flag | out | - | read with `FriendForm` |
| `Specialization` | flag | out | - | read with `FriendForm` |
| `Constructor` | flag | out | - | read with `FriendForm` |
| `Destructor` | flag | out | - | read with `FriendForm` |
| `Conversion` | flag | out | - | read with `FriendForm` |
| `Class` | flag | out | - | read with `FriendForm` |

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
| `lexical_ctx` | entity | in | ref*sorted | where the declarations were written, when that is not their semantic context (rule below). Mangling, visibility, code generation and instantiation read it |
| `begin_loc` | location | out | - |  |
| `delete_message` | child | in | child? |  |
| `explicit_expr` | child | in | child? |  |
| `ctor_inits` | child | in | record CtorInit* |  |
| `overrides` | entity | in | ref*sorted | the functions this one overrides: filled when the member is declared, also for an implicitly declared one (§11.5). Read from the first declaration, whose list a redeclaration copies |
| `friends` | entity | in | key*sorted | the class's `Friend` units |
| `key_function` | entity | demand | - | §11.5 |
| `dtor_delete` | entity | demand | - | §11.5 |
| `dtor_delete_this` | child | demand | - | §11.5 |
| `record_pack` | value | in | uv? |  |
| `contracts` | child | in | child* | the function's contract assertions, in order |
| `asm_label` | name | in | string? |  |
| `alignment` | child | in | child? |  |
| `deprecation` | value | in | string? |  |
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
| `using_shadows` | entity | out | - | the shadows a using-declaration introduced, in introduction order: each shadow is a unit whose key holds its scope, its name and what it names (§5.2), and its own `DeclMisc` `0-access` holds the access a class-scope using-declaration gives it; the order of introduction is not behaviour |
| `shadow_using` | bookkeeping | out | - | inverse of `using_shadows` |
| `conversion_spec_target` | written | out | - | the type a conversion function template specialisation's name prints with; the declaration keeps the pattern's name. It reaches the program only as text made from a name (digest §9) |
| `compound_literal` | entity | unsupported | - | the implicit variable of a file-scope compound literal has no name to key it by; a GNU extension in C++ |
| `init_fp` | flag | in | record FpOverride? | the floating-point pragma state a constructor's member initialisers, a field's default member initialiser or a parameter's default argument were parsed in, when it is not the command line's: code generation emits the initialiser under it, not under the state of the place that uses it |
| `closure_owner` | entity | in | keyed | the declaration whose text holds a closure's lambda-expression when that is not the closure's context: the parent of the closure's key (§10.4, digest §4.3); `docs/wp/WP-040.md` implements it, and until then a closure is `Pending(Area::Local)` |
| `written_ns` | entity | out | - | the namespace opening a namespace-scope declaration belongs to when it is not `ctx` (Kiln WP-2725, `kiln_ast/src/decl.rs:1786`): the latest opening when a qualified declarator's qualifier named the namespace, the one an explicit specialisation of a function template is written in. Its one reader is code generation's walk for visibility (`kiln_codegen/src/visibility.rs:455-464`), and what that walk gives is the row `enclosing` (§1.7 item 3), which starts at the same opening |
| `shadow_named` | entity | out | - | the using-shadow a using-declaration's lookup found when it found one (Kiln WP-3401, `kiln_ast/src/decl.rs:1889`): the declaration the using-declaration introduces is its target, which each shadow's key holds (§5.2); the shadow it was found through is how the name was reached (digest §1 rule 2), as `oper_lookups`' `found` |
| `friend_form` | bookkeeping | out | - | index of the friend's written form in `DeclArena` `friend_forms` (Kiln WP-3164, `decl.rs:1921`); the form's rows are `FriendForm`'s |
| `subst_failed` | flag | unsupported | - | what failed to form, silently, in an invalid declaration (Kiln WP-3281, `decl.rs:1925`): read by `docs/wp/WP-136.md`. Kiln sets it only on a declaration with `DeclFlags` `0-invalid` (`decl.rs:393-400`), which makes the walk indeterminate (`Cause::Error`) wherever it is read, so it needs no reading of its own |

`lexical_ctx` is written as `ref*sorted`: for each declaration counted,
a `Ref` to the nearest context that encloses the place it was written
and is not transparent (§5.2), when that context is not the entity's
semantic context (`Ref::none` for the global namespace). With a
definition the one declaration counted is the one the walk reads (§2.3),
so the row holds at most one `Ref`; without one, every declaration of the
entity counts, except a class declaration that a friend declaration made
(`friend struct H;` declares `H` only when no `H` is found), so that the
row does not depend on which declaration came first (digest §2.1). It is
non-empty for a friend declared or defined in a class, for a function or
an `extern` variable declared in a block, and for a member defined
outside its class or its namespace. A function defined in the class that befriends it
differs from one only declared there by this row, which is why
`FriendKind` needs no variant for it.

### `kiln_ast::decl::SubstFailedKind`

`kind=enum; pin=schema`

What failed to form in a declaration Kiln made invalid while its errors
were trapped (Kiln WP-3281, `kiln_ast/src/decl.rs:401`). Read only with
`ColdDeclData` `subst_failed` (`unsupported`, `docs/wp/WP-136.md`).

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `FunctionSignature` | flag | out | - | read with `ColdDeclData` `subst_failed` |
| `ExplicitSpecifier` | flag | out | - | read with `ColdDeclData` `subst_failed` |
| `VariablePartialAmbiguity` | flag | out | - | read with `ColdDeclData` `subst_failed` |
| `ClassPartialAmbiguity` | flag | out | - | read with `ColdDeclData` `subst_failed` |
| `LocalOfFailedSignature` | flag | out | - | read with `ColdDeclData` `subst_failed` |
| `OtherTrapped` | flag | out | - | read with `ColdDeclData` `subst_failed` |

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
| `friend_forms` | bookkeeping | out | - | arena of friends' written forms (Kiln WP-3164, `kiln_ast/src/decl.rs:2062`): reached through `ColdDeclData` `friend_form` |
| `friend_form_decls` | bookkeeping | out | - | the friend of each `friend_forms` row: the inverse of `ColdDeclData` `friend_form` |

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
`BreakStmt`, `ContinueStmt`) is its tag alone. A statement the stream
has written already, in the same state, is written as `backref` (§1.7):
`tag 1537` and the count of its first writing (digest §4.3
"Back-references"); statements and expressions are counted in one
sequence.

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
| `then` | child | in | child? | always present; a branch discarded in an instantiation is an empty block |
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
- **Parentheses.** A `ParenExpr` around an operand that is not
  instantiation-dependent is not written outside a canonical operand: the
  walk writes its operand in its place (digest §6). Every other
  `ParenExpr` is written with its rows: around a dependent operand, where
  a parenthesis can change the meaning (`decltype((x))`, `&(T::m)`);
  everywhere in profile mode (§4.4), as Kiln's profile keeps them; and
  the outermost one at the root of a walk (the whole expression of a
  `return`, an initialiser, a template argument: what `expr::child` is
  given) whose operand, through parentheses only, is an id-expression or
  a class member access (`DeclRefExpr`, `MemberExpr`,
  `UnresolvedLookupExpr`, `UnresolvedMemberExpr`,
  `DependentScopeDeclRefExpr`, `CXXDependentScopeMemberExpr`, or a
  substituted template parameter, `SubstNonTypeTemplateParmExpr`,
  `SubstNonTypeTemplateParmPackExpr`, `FunctionParmPackExpr`): that is
  the one operand whose parenthesis `decltype` reads ([dcl.type.decltype]/1.3
  against /1.5), and `decltype(auto)` deduces from the whole expression of
  a `return` or an initialiser ([dcl.type.auto.deduct]/5): `return (x);`
  against `return x;` is `int&` against `int`. The parentheses inside it
  are skipped as redundant. Inside an operator a parenthesis around a
  name is never read, and is skipped.
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
- **Nodes reached twice.** An expression the stream has written already,
  in the same state, is written as `backref` (§1.7): `tag 1537` and the
  count of its first writing, in place of all its rows, those of its maps
  included (digest §4.3 "Back-references"). An `OpaqueValueExpr`, and a
  node numbered as an opaque value's source when the walk reaches it, are
  written by the rule above and never as `backref`: the opaque rule
  first. Nothing of this is written in profile mode (§4.4), whose walk
  writes every node in full.
- **Overloaded member access.** A `MemberExpr` whose type is the
  overloaded-function placeholder names the first candidate of its set.
  The first candidate is written; the rest of the set is the result of
  looking that member's name up in the naming class, whose unit is
  compared separately. The names overload resolution reads besides,
  the qualifier, the explicit template arguments and the naming class,
  are `ColdExprData` `member_overload`.
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
| `DeclRefExpr.a` | entity | in | ref | the entity named, not the declaration it was found through; where name-derived values are written by value, a block-scope variable's `value?` follows (§11.3) |
| `DeclRefExpr.b` | written | in | record DeclRefExtra | index into `ExprArena` `declref_extras`; 0 is written as a record with nothing present |
| `MemberExpr.bits-arrow` | flag | in | bit | bit 4 |
| `MemberExpr.bits-spliced` | flag | unsupported | - | bit 5: written as a member splice; reflection (§12) |
| `MemberExpr.bits-had-multiple-candidates` | bookkeeping | out | - | bit 6: the member chosen is the fact (digest §6) |
| `MemberExpr.aux` | flag | in | enum NonOdrUse |  |
| `MemberExpr.a` | child | in | child | the object expression |
| `MemberExpr.b` | entity | in | ref | the member; for an overloaded access the first candidate (rule above) |
| `CXXThisExpr.aux-implicit` | flag | in | bit | bit 0, written only for an instantiation-dependent node: Kiln's template substitution, its mangling of dependent expressions and its profiles read it; elsewhere nothing but diagnostics does (pitfalls E07) |
| `PredefinedExpr.aux` | flag | in | enum PredefinedKind |  |
| `PredefinedExpr.a` | value | symbolic | - | the string literal holding the enclosing function's name; absent in a dependent context. The node's `ty` is written as §4.2 rule 8 says |
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
| `CallExpr.bits-adl` | bookkeeping | out | - | bit 5: found by argument-dependent lookup; the callee is the fact (digest §6). Only Kiln's dump reads it, not instantiation or code generation |
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
| `CXXRewrittenBinaryOperator.b` | child | in | bool | the two operands as written: they are nodes of the semantic form `a`, where they are written; written is whether their order is the reverse of their order in `a` (a pre-order walk of `a` that does not enter them), which instantiation rebuilds from; one not found under `a` is `Unaccounted` |
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
| `InitListExpr.a` | child | in | child* | runs of one node: the count of runs, then for each its length and the node once; a GNU range designator's initialiser is one node in every slot it fills |
| `InitListExpr.b` | child | in | record InitListExtra | index into `init_lists` |
| `InitListExpr.bits-designated` | written | out | - | bit 4 (Kiln WP-2350): a list without braces of its own that a designator opens (`.a.y = 3`'s list of `a`); it has no written extent. The list's elements and designators are the fact |
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
| `CXXDefaultArgExpr.a` | entity | in | uv | the parameter, by its position (`ParamData` `index`). The callee or constructor of the enclosing call names the function; the default is that of the declaration name lookup found at the call (`kiln_sema` expr.rs:6189), which a block-scope redeclaration with defaults of its own is, and that declaration writes it (in its unit, or for a block-scope redeclaration where `local::write_decl` meets it) |
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
| `CXXDeleteExpr.bits-array-as-written` | flag | in | bit | bit 6: `delete[]` was written. In a template `bits-array-form` is not known yet and instantiation reads this bit |
| `CXXDeleteExpr.aux-sized` | flag | in | bit | bit 0: the deallocation function takes the size |
| `CXXDeleteExpr.aux-aligned` | flag | in | bit | bit 1: it takes the alignment |
| `CXXDeleteExpr.a` | child | in | child |  |
| `CXXDeleteExpr.b` | entity | in | ref | the deallocation function |
| `CXXPseudoDestructorExpr.aux-arrow` | flag | in | bit | bit 0 |
| `CXXPseudoDestructorExpr.aux-qualified` | flag | in | bit | bit 1, written only for an instantiation-dependent node: the destructor name was qualified; Kiln's template substitution, its mangling of dependent expressions and its profiles read it; elsewhere nothing does |
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
| `OffsetOfExpr.aux` | flag | in | bool | non-zero: the designator passes through an invalid class (`invalid_record`); Kiln has reported an error, and the unit is indeterminate (`Cause::Error`) |
| `OffsetOfExpr.a` | child | in | child* | the index expressions |
| `OffsetOfExpr.b` | value | in | uv | the stored result |
| `SizeOfPackExpr.a` | entity | in | ref | the pack |
| `RequiresExpr.aux` | value | in | uv | the stored result: 1 false, 2 true; 0 for a dependent expression and after a substitution error, never for want of asking |
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
| `CoawaitExpr.a` | child | in | child* | elements two to five of the list (`CoawaitParts`): the first, the operand as written, is a node of `common`; the sixth, the placeholder for the awaiter, is reached from the calls |
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
| `operand` | child | out | - | first element: the operand as written, not written: `common` is built from it (for `co_yield {…}`, a placeholder `InitListExpr` of type `void` whose elements are nodes of `common`) |
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
| `qualifier` | written | out | - | how the name was qualified: the entity found is the fact, and each thing Kiln reads the qualifier for is written elsewhere (rule below); what its types hold beyond the referent (an instantiation-dependent operand of a template argument, with the found sets in it) is the unit's list (§4.4 "Spellings", rule 10) |
| `template_args` | written | in | enum ArgView*? | written only when the entity named is a template, where the arguments select what the name means; for a specialisation the entity is the fact, and what the arguments hold beyond it is the unit's list (rule 10) |
| `found_decl` | entity | out | - | the using-declaration the name was found through: a reference names the entity (digest §4.1). Only the dump and the module image read it |

Once a name is resolved Kiln reads `qualifier` for four things, and
another row writes each: that a qualified call of a virtual function is
not dispatched (`ColdExprData` `member_extra-qualified`); that a
qualified name in a dependent call gets no argument-dependent lookup at
instantiation (`UnresolvedLookupExpr.aux`); the conversion of an object
expression to the class the qualifier names (an implicit cast node and
its base path); and a qualifier that is itself dependent (`ColdExprData`
`expr_nns`). Adding, removing or respelling a qualifier that leaves all
four alone changes no digest, inside a template as well: the tests of
the row hold both cases (and `equal/_/spelling-requalified-name*`). What
a qualifier's types and a specialisation's arguments hold beyond the
referent is not a property of the qualifier: `Bq<sizeof(sizeof(f(t)))>::value`
names `Bq<8>::value` whatever `f` finds, while what `f` finds decides
whether the program is well-formed at instantiation. The full walk reads
those parts as written into the unit's list, as profile mode's units do
(§4.4 "Spellings", rule 10).

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
| `Index.index` | child | in | uv | the position of the index in `OffsetOfExpr.a`, where it is written |
| `Index.start` | location | out | - |  |
| `Index.end` | location | out | - |  |

### `kiln_ast::expr::OffsetofStep`

`kind=enum; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Offset` | flag | in | tag 1 |  |
| `Offset.0` | value | in | uv | bytes |
| `Index` | flag | in | tag 2 |  |
| `Index.index` | child | in | uv | the position of the index in `OffsetOfExpr.a`, where it is written |
| `Index.elem_size` | value | in | uv |  |

### `kiln_ast::expr::OffsetofNode`

`kind=enum; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Base` | flag | in | tag 1 |  |
| `Base.class` | entity | in | ref | the base class stepped through |
| `Field` | flag | in | tag 2 |  |
| `Field.field` | entity | in | ref | the member |
| `Field.start` | location | out | - |  |
| `Field.end` | location | out | - |  |
| `Index` | flag | in | tag 3 |  |
| `Index.position` | child | in | uv | the position of the index in `OffsetOfExpr.a`, where it is written |
| `Index.start` | location | out | - |  |
| `Index.end` | location | out | - |  |

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

### `kiln_ast::expr::SrcLocRole`

`kind=enum; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `File` | flag | out | - | read with `StringMaker` |
| `FileName` | flag | out | - | read with `StringMaker` |
| `Function` | flag | out | - | read with `StringMaker` |
| `PrettyFunction` | flag | out | - | read with `StringMaker` |

### `kiln_ast::expr::StringMaker`

`kind=enum; pin=schema`

Read only with `ColdExprData` `string_maker` (`out`).

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `SourceLoc` | value | out | - | a source-location builtin's string (Kiln WP-3000, `kiln_ast/src/expr.rs:1736`) |
| `SourceLoc.role` | value | out | - | which value of the use: `SourceLocValues` holds them, symbolic |
| `SourceLoc.decl` | entity | out | - | the declaration the use is evaluated in: `SourceLocValues` `ctx` |
| `Reflection` | value | out | - | a string of reflection (§12) |
| `ContractSite` | value | out | - | a contract violation's string: the contract's site, `ContractSite` |

### `kiln_ast::expr::UseImmediate`

`kind=struct; pin=schema`

One element of `ColdExprData` `use_imm` (Kiln WP-3278, `expr.rs:1706`).

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `invocation` | child | out | - | the immediate invocation of the shared default argument or initialiser: written with it |
| `value` | value | out | - | its value at the use: `out`, as `use_imm` |

### `kiln_ast::expr::ColdExprData`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `end_loc` | location | out | - |  |
| `member_extra-qualified` | flag | in | bool | a `MemberExpr` whose `DeclRefExtra` has a qualifier and whose member is a virtual function: a qualified name suppresses virtual dispatch (digest §6); a qualifier on any other member is not a fact |
| `member_extra-record` | written | in | record DeclRefExtra | no entry is written as a record with nothing present |
| `member_overload` | entity | in | record OverloadedMember? | an overloaded `MemberExpr`: the names overload resolution reads (§7, "Overloaded member access") |
| `designators` | name | in | enum Designator* | outermost first |
| `syntactic_list` | child | in | child? | the list as written, in a template pattern: instantiation rebuilds from it |
| `array_trait` | value | in | record ArrayTraitExtra? |  |
| `trait_rhs_loc` | location | out | - |  |
| `source_loc` | value | symbolic | - | the rows of `SourceLocValues` |
| `string_maker` | value | out | - | what made a string object Kiln made itself, keyed by its `StringLiteral` (Kiln WP-3000, WP-3280, `kiln_ast/src/expr.rs:1530`): a source-location value's role and the declaration it is evaluated in, reflection, a contract's site. The object's content is the fact, written as a string by the rows of `Entry`; which object one use shares with another follows from the role and the declaration, which `source_loc` (symbolic) and the using unit hold |
| `default_imm` | value | demand | - | §11.4 |
| `use_imm` | value | out | - | the value at one use of each immediate invocation of a default argument or default member initialiser with source-location values (Kiln WP-3278, `expr.rs:1546`; CWG 2631). Nothing reads it: each value is a function of where the use is (a location, `out`) and of the declaration it is in (the using unit, whose key holds it), and the default argument or initialiser is written once, its source-location builtins symbolic (`source_loc`). `equal/_/use-imm-line` moves the use |
| `union_field` | entity | in | ref |  |
| `cast_written-unresolved-construct` | type | in | type? | on a `CXXUnresolvedConstructExpr`: the type as written, which instantiation and mangling use |
| `cast_written-cast` | type | in | type? | on an explicit cast: the type as written, with the qualifiers and the reference the node's type drops. Partial ordering, the search for unexpanded packs and the normal form of a constraint read it |
| `compound_literal_written` | written | out | - | the type a compound literal wrote when its node's type is not it (Kiln WP-3589, `expr.rs:1559`): `(T[]){1, 2}`, whose node type is `T[2]`. The node's type is what code generation reads; reading the written type is `docs/wp/WP-156.md`'s (operand units) |
| `offsetof_pattern-type` | type | in | type? | a value-dependent `OffsetOfExpr`: the operand type |
| `offsetof_pattern-components` | name | in | enum OffsetofComponent* | its designator as written |
| `offsetof_layout` | value | in | enum OffsetofStep* |  |
| `offsetof_designator-type` | type | in | type? | a non-dependent `OffsetOfExpr`: the operand type (Kiln WP-2360, `kiln_ast/src/expr.rs:1554`) |
| `offsetof_designator-nodes` | name | in | enum OffsetofNode* | its designator as resolved: the bases stepped through, the members, the indices |
| `splice_args` | value | unsupported | - | reflection (§12) |
| `default_arg_rewritten` | child | in | child? | the initialiser rebuilt for this use |
| `fp_round` | flag | in | enum FpRound? |  |

### `kiln_ast::expr::OverloadedMember`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `qualifier` | name | in | enum Nns? | the qualifier, by what it names (an alias of the class is the class): lookup is in its class, and a qualified call is not dispatched |
| `template_args` | written | in | enum ArgView*? | the explicit template arguments, which select the candidates a call deduces from |
| `naming_class` | entity | in | ref | the class access to the member is checked in ([class.access.base]/5): the qualifier's class, else the object's, so it is implied by `qualifier` and the object expression and no source changes it alone |

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
| `Pointer.unions` | entity | in | ref* | the members of unions the designator path enters that are not their union's default member, outermost first (Kiln WP-2200, `kiln_ast/src/consts/value.rs:487`): `&u.b` against `&u.a` of `union { int a; int b; } u`; canonical |
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
value (digest §7): a temporary can hold a pointer to itself, and its value
is already written, with its `LetData`, at the `MaterializeTemporaryExpr`
in the unit of the declaration that extends it. The pair written here
names the temporary within that declaration.

A name-derived object (digest §9) is written as the object it is: the
entity whose name it holds and which builtin, so that two are written
alike only if they are the same string (or `__impl` object) of the same
entity. A string is written so only where its bytes are what Kiln prints
for that entity (the walk prints the name again with Kiln's printer,
`kiln_sema/src/predefined.rs` `compute_predefined_name`, and compares);
anywhere else, and for a file name, its bytes are written as the table
says. The offset into the string is written by value (`Pointer.offset`).

- `StringLit.lit` of a string a `PredefinedExpr` holds (its literal, the
  expression before the `PredefinedExpr` that names it in `a`): `bytes`
  empty, `uv 0` where a string's encoding tag stands (no encoding has tag
  0), `uv 1`, the `PredefinedKind` tag, then `ref` of the function whose
  name it is: of the functions whose definition holds the
  `PredefinedExpr` (expansion locations; a definition extends from its
  first written mem-initializer, or the `try` of a function-try-block,
  or its body's `{`, to its body's `}` or its last handler's) and whose
  name of that kind the bytes are, the innermost; none, or two of one
  extent that are not one entity, and the bytes are written. Then the
  local scope names around that function (below).
- `StringLit.lit` of a function name of the source-location builtins
  (`SourceLocValues` `function`, the plain name, or the function name an
  `__impl` object points to, the pretty name; its uses taken by content,
  `spec/digest.md` §9):
  `bytes` empty, `uv 0`, `uv 2`, `uv 0` (plain) or `uv 1` (pretty), then
  `ref` of the context of the first use (by `ExprId`) that points to it,
  if the bytes are that context's name of that kind, then the local
  scope names around it (below). A string some use
  points to as its file (`file`, `file_name`, an `__impl` object's file)
  is written by its bytes.
- `Const.0` of an `__impl` object (`SourceLocValues` `location`): the
  digest `H("slip.value.v1", uv 0, uv 3, type)` of its type (a value's
  own stream begins with a `ValueKind` tag, never 0, so no value written
  by content has it), then `ref?` of the function of the use that built
  it (`Ref::none` outside a function), the local scope names around that
  function (`count` 0 outside one), `uv` its ordinal among the uses of
  that function (of every context that is not a function) that build
  one, in `ExprId` order, then the file name it points to (the use's
  `SourceLocValues` `file`): `opt`, and the literal's entry as `string`
  writes any literal's, bytes then encoding tag, the treatment a
  `__FILE__` literal's bytes get. Its line and column are not written.
  Kiln builds one object per use
  (`kiln_sema/src/builtins.rs:3752` `source_location_object`); an object
  no recorded use names (one a deduced return type replaced, `:3660`) is
  written by content.

The local scope names of the entity a `ref` names (WP-040,
`write_scope_names`) are part of a name string's identity and are
written in both modes: `count`, then the name of each named local class
or enumeration around the entity, outermost first (`local.rs`
`write_names_around`), since a reference to a local entity holds no
name. Where the walk writes such values by value (§4.2
rule 8) the rows are written as the table says.

### `kiln_ast::consts::value::ByteImage`

`kind=struct; pin=none`

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
| `upaths` | bookkeeping | out | - | arena: the union paths (`UnionPath`), spans of `paths` |

### `kiln_ast::consts::UpathRec`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `start` | bookkeeping | out | - | arena |
| `len` | bookkeeping | out | - | arena |

### `kiln_ast::consts::ConstStore`

`kind=root; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `pools` | bookkeeping | out | - |  |
| `table` | bookkeeping | out | - | the intern table; not stored (`snap(skip)`) |
| `upath_ix` | bookkeeping | out | - | the union paths' intern table; not stored (`snap(skip)`) |

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
| `DependentFriend` | flag | unsupported | unbuilt | retired |
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
partial specialisation. For a specialisation of the definition class
(kind `ImplicitInstantiation` or `ExplicitInstantiationDeclaration`)
`from` and `from_args` are written in the resolved unit `definition` and
not in the specialisation's own stream (§11.9), because they are known
only once the definition exists; the own stream writes `Ref::none` and
an absent list, without a cause.

`MemberSpecInfo` `kind` is written as the member's effective kind
(§11.10), not as Kiln holds it: Kiln's depends on what was instantiated
first.

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `template` | entity | in | keyed | the primary template: the key's parent |
| `args` | value | in | keyed | the discriminator, in canonical form (`TypeContext::canonical_args`); the sugar is not written |
| `kind` | flag | in | enum SpecKind |  |
| `from` | entity | in | ref | rule above (digest §8); in `definition` for the definition class (§11.9) |
| `from_args` | value | in | enum ArgView*? | the arguments deduced for the partial specialisation `from`; as `from` |
| `point_of_instantiation` | location | out | - |  |
| `print_args` | written | out | - | the arguments names print with: `args` without trailing defaults. It reaches the program only as text made from a name (digest §9) |
| `defaulted` | flag | in | uv | bit i: argument i came from, or equals, its parameter's default; debug information reads it |

### `kiln_ast::templates::MemberSpecInfo`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `instantiated_from` | entity | in | ref | the member of the pattern |
| `kind` | flag | in | enum SpecKind | the effective kind (§11.10) |
| `point_of_instantiation` | location | out | - |  |

### `kiln_ast::templates::PartialSpecInfo`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `partial` | bookkeeping | out | - | the partial specialisation itself |
| `template` | entity | in | keyed | the primary template: the key's parent |
| `params` | entity | in | record TemplateParamList | the head; also in the key (§3.3) |
| `args` | written | out | - | the argument pattern as written; `canonical` carries it, except for what an alias template specialisation says only in its arguments ("Hazards accepted for now") |
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

`member_like_friend` names the class specialisation in which a friend
function template whose constraint depends on the class's parameters
([temp.friend]/9) is mangled: such a friend is declared by each
specialisation apart and exists only through instantiations, so it is a
derived unit (digest §2.2) and that specialisation is its key's parent. A
non-template friend with a requires-clause, which Kiln's mangler scopes
the same way, has as parent the class specialisation that declares it.
`instantiated_from` is written as it is for a declaration instantiated
outside its class (digest §2.2) and for any other instantiated
declaration; on a declaration written in source, an entry naming a
friend's pattern (a declaration lexically in a class and semantically in
a namespace), which an instantiation set there, is written absent.

### `kiln_ast::templates::PatternMemInits`

`kind=struct; pin=schema`

A pattern constructor's ctor-initializer as written (Kiln WP-3026,
WP-3200, `kiln_ast/src/templates.rs:293`). Read only with
`TemplateTables` `pattern_mem_inits` (`unsupported`, `docs/wp/WP-124.md`
item 5), so no row below reaches a stream.

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `colon` | location | out | - |  |
| `failed` | flag | out | - | read with `pattern_mem_inits` |
| `inits` | child | out | - | read with `pattern_mem_inits` |

### `kiln_ast::templates::PatternMemInit`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `id` | entity | out | - | read with `pattern_mem_inits` |
| `loc` | location | out | - |  |
| `end` | location | out | - |  |
| `pack_expansion` | flag | out | - | read with `pattern_mem_inits` |
| `init` | child | out | - | read with `pattern_mem_inits` |

### `kiln_ast::templates::PatternMemInitId`

`kind=enum; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Member` | entity | out | - | read with `pattern_mem_inits` |
| `Member.0` | entity | out | - | read with `pattern_mem_inits` |
| `Type` | entity | out | - | read with `pattern_mem_inits` |
| `Type.ty` | type | out | - | read with `pattern_mem_inits` |
| `Type.ident` | name | out | - | read with `pattern_mem_inits` |
| `Ident` | name | out | - | read with `pattern_mem_inits` |
| `Ident.0` | name | out | - | read with `pattern_mem_inits` |

### `kiln_ast::templates::PatternMemInitArgs`

`kind=enum; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Paren` | child | out | - | read with `pattern_mem_inits` |
| `Paren.args` | child | out | - | read with `pattern_mem_inits` |
| `Paren.lparen` | location | out | - |  |
| `Paren.rparen` | location | out | - |  |
| `List` | child | out | - | read with `pattern_mem_inits` |
| `List.0` | child | out | - | read with `pattern_mem_inits` |

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
| `pattern_mem_inits` | child | unsupported | - | the ctor-initializer of a constructor definition in a template, as written (Kiln WP-3026, WP-3200, `kiln_ast/src/templates.rs:396`): reading it is `docs/wp/WP-124.md` item 5. Every constructor definition that has an entry is indeterminate (`kiln/pattern-mem-initializers`, `decl_class.rs` `mem_inits_outside_ast`, the condition under which Kiln keeps the list here and not in `ctor_inits`) |
| `expr_targs` | value | in | enum ArgView*? | the explicit template arguments of a dependent or unresolved name |
| `expr_nns` | name | in | enum Nns? | the qualifier of a dependent or unresolved name |
| `oper_lookups` | entity | in | record OperLookups | the definition-context lookup set of a dependent operator expression (rule below) |
| `expr_begin` | location | out | - |  |
| `size_of_pack_origin` | entity | in | ref | the pack a substituted `sizeof...` named: the literal still mangles by it |
| `size_of_pack_partial` | value | in | enum ArgView*? | the partly substituted arguments of a `sizeof...`: mangled |
| `pack_index_origin` | flag | in | bool | the expression is the selected element of a substituted `pack...[i]`; the pattern's expression it maps to belongs to the template's unit |
| `pack_index_written` | written | out | - | the substituted index of the selected element a `pack...[i]` became (Kiln WP-3582, `templates.rs:425`): the written form clang keeps; the element is the fact (`pack_index_origin`, and the element itself). Reading it is `docs/wp/WP-156.md`'s |
| `member_op_loc` | location | out | - |  |
| `pseudo_dtor_name` | name | in | sym? | the identifier after `~` that named no type at the definition |
| `pseudo_dtor_first-type` | type | in | type? | the first type-name of `p->S::F::~Q()` |
| `pseudo_dtor_first-position` | location | out | - |  |
| `template_kw` | written | out | - | the `template` keyword after a qualifier: read by the printer only |
| `unresolved_found` | entity | out | - | per element of an `UnresolvedLookupExpr`'s declarations, the using-shadow it was found through (Kiln WP-3400, `templates.rs:449`): the element's target is written (`UnresolvedLookupExpr.b`, a shadow written as its target); the shadow is how the name was reached (digest §1 rule 2), as `oper_lookups`' `found` |
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
| `written_args` | written | out | - | how many arguments were written before defaults filled the rest: text made from a name (digest §9) |
| `args_as_written` | written | out | - | the arguments of an explicit specialisation or instantiation as written: text made from a name (digest §9); `SpecInfo` `args` is the fact. A partial specialisation's list as written is read for its left-out facts (§4.4 "Spellings", rule 12), not as a fact of the record |
| `late_explicit_insts` | bookkeeping | out | - | functions explicitly instantiated before their template was defined: emitted at the end of the translation unit |
| `late_explicit_set` | bookkeeping | out | - | the same functions as a set |
| `preferred_named` | written | out | - | which specialisations print by their template's `preferred_name`: the attribute is written with the template, and the rest is text made from a name (digest §9) |
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

`oper_lookups` holds, for a unary, binary or compound-assignment
operator expression with a type-dependent operand in a template, the
non-member operator functions unqualified lookup found at the definition
([temp.dep.candidate]/1, [over.match.oper]/3.2). Instantiation uses that
set and argument-dependent lookup at the point of instantiation, never a
new unqualified lookup. The expression writes `by_expr` as the
declarations of its candidates, each a `ref`, sorted and without
duplicates as `UnresolvedLookupExpr.b` is. Absent and empty differ:
instantiation looks up again for an expression with no set and uses
argument-dependent lookup alone for an empty one. `found`, the
using-declaration's shadow a candidate was found through, is `out`: Kiln
keeps one candidate per function, so two shadows of one function are
never both in a set, and the shadow reaches only a deprecation warning
and `DeclRefExtra` `found_decl`, which is `out`.

### `kiln_ast::oper_lookup::OperLookups`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `by_expr` | entity | in | ref*sorted? | by expression: the `decl` of each candidate of its set (rule above) |
| `sets` | bookkeeping | out | - | interning: equal sets are stored once |
| `cands` | bookkeeping | out | - | interning |
| `index` | bookkeeping | out | - | interning; not stored (`snap(skip)`) |
| `indexed` | bookkeeping | out | - | interning; not stored (`snap(skip)`) |

### `kiln_ast::oper_lookup::OperCandidate`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `decl` | entity | in | ref | the function or function template |
| `found` | entity | out | - | the shadow it was found through, else `decl`: rule above |

### `kiln_ast::oper_lookup::OperSetId`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `0` | bookkeeping | out | - | interning: the set is written by its members |

### `kiln_ast::oper_lookup::SetSpan`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `start` | bookkeeping | out | - | interning |
| `len` | bookkeeping | out | - | interning |

## 10. The other tables of `Ast`

### 10.1 Attributes

The encoding `attrs` writes an attribute list: a count, then each node as
`record AttrNode`.

- For a statement (`AttributedStmt`) the nodes are those of its list, in
  stored order.
- For a declaration, the lists are read over the entity's **chain**: the
  declarations of Kiln's `redecls_of` of the entity (for a template, the
  templates its pattern's declarations describe), first declaration
  first, leaving out a declaration in a templated body and, unless every
  declaration is one, a declaration instantiated outside its class
  (`spec/digest.md` §2.2; `Walk::redecls`). A declaration's **own list**
  is the count and the nodes of its `AttrArena` `decl_attrs` in stored
  order, leaving out each node whose `inherited` flag is set: it is a
  copy from an earlier declaration and says nothing new (Kiln makes such
  a copy for an `ASM_LABEL` and for a parameter's attribute). One
  exception: a parameter of a declaration made by instantiation
  (`templ::made_by_instantiation`) keeps its `inherited` nodes, since
  Kiln WP-3407 (`14b25bd70`, in the pin since WP-163) gives it the
  attributes of the template's latest declaration, where the first
  declaration's are such copies, and nothing else in its chain holds
  them (`attrs::param_nodes`). The entity's **defining declaration**
  is its definition (for a template, the template its pattern's
  definition describes) when the chain holds it and the entity is not of
  the definition class (§11.9); otherwise there is none. For an entity
  of the definition class, the own list of a declaration made by
  instantiation (`templ::made_by_instantiation`, §11.9) leaves out what
  Kiln copies onto it when it instantiates the definition, which is
  `definition`'s: for a class every node, for a function the nodes of
  kinds `WEAK` and `PRAGMA_CLANG_SECTION`; and the annotations of a
  class's declaration made by instantiation are `definition`'s too.
- For a declaration other than a parameter, `attrs` is three parts:
  the own lists of the declarations before the defining one (all of them
  when there is none), empty lists left out, preceded by their count;
  `opt`, present when there is a defining declaration, with its own
  list; the own lists of the declarations after it, as those before.
  Which declaration holds a node is kept because Kiln's code generation
  reads it:
  - `annotate` with its multiplicity: `ann_attrs` (kiln_codegen
    `annotate.rs:124`) walks the chain from the first declaration to the
    emitted one (the definition), each declaration inheriting the
    strings it does not have itself, so `annotate("a")` on each of two
    declarations is one global annotation and twice on one declaration
    two (Kiln keeps no `annotate` node on a declaration after the
    definition, as clang 19, which warns "attribute declaration must
    precede definition");
  - `fn_attr` (kiln_codegen `new_delete.rs:169`) reads the first node of
    a kind on the latest declaration that has one, so of two
    `patchable_function_entry` nodes it reads the second when they are on
    two declarations and the first when they are on one;
  - `target_version` is read on the declarations up to the emitted one
    (`target_version_so_far`, kiln_codegen `module.rs:2122`).

  Other readers read the chain as a set or as one list
  (`has_fn_attr`, `module.rs:2132`; `fn_attrs_of`, `class_b.rs:321`,
  which sorts by location), and the `enable_if` lists of all
  declarations are the same (differing lists declare overloads,
  kiln_sema `enable_if.rs:75`); for them the parts say more than Kiln
  reads, which makes a move between declarations a reported difference.
  Within a list the order is kept: Kiln mangles the `enable_if`
  conditions of a function in it (kiln_abi `mangle.rs:1904`).
- For a parameter of a function, the lists are those of parameter *i*
  (its `ParamData` `index`) of the declarations of the function's chain
  except those at block scope (`decl_func::param_decls`), instantiated
  declarations included: the declarations whose defaults `ParamData`
  `default_arg` reads (§5.4). A declaration of a function has as many
  parameters as every other, so each has a parameter *i*. `attrs` is two
  parts: `opt`, present when the function has a defining declaration,
  with the own list of its parameter *i*; then the own lists of the
  other declarations' parameters *i*, in chain order, empty lists left
  out, preceded by their count. The definition's is apart because Kiln's
  function prolog reads `nonnull`, `align_value` and the parameter's
  `annotate` on the definition's own parameter only (kiln_codegen
  `class_b.rs:715`, `:718`, `annotate.rs:327`), while `noescape` and the
  `nonnull` of the sanitiser read every declaration's
  (`redecl_params`, `class_b.rs:591`; `ubsan.rs:2150`): a node moved
  between two declarations that are not the definition says nothing
  Kiln reads, and a node moved to or from the definition is a
  difference. The attributes on a parameter of a block-scope declaration
  are that declaration's own: they are written with it in its block
  (§2.4), not in the entity's unit ("Hazards accepted for now"). A
  parameter that is not at the position its `index` names in its
  function's list makes the unit `Unaccounted("ParamData/index")`, and a
  parameter of a declaration (not at block scope) that is not in the
  function's chain `Unaccounted("ParamData/chain")`. In the resolved
  unit `definition` the chain also holds the defining declaration it
  reads when an instantiation made that outside its class (a friend's
  definition, §11.9), as the last declaration.

A function with no defining declaration writes every list among the
others: a program in which it has a definition differs from one in which
it has none in `DeclMisc` `0-definition` already. A function
of the definition class is written as one with none, so its unit does
not depend on a use; its definition's attributes are the pattern's,
which the template's unit holds, or an implicit member's, which no one
writes. A definition instantiated outside its class (a friend's) is the
class specialisation's (`spec/digest.md` §2.2), not the entity's.

An instantiated declaration carries the attributes Kiln instantiated from
its pattern; they are written as found.

A kind with no row for `a` or for `b` stores 0 there. A non-zero word, like
a flag bit no row names, makes the unit indeterminate (§2.1): Kiln's
operand roles are documented kind by kind and nothing in its types
enforces them ("Open questions"). The roles below are those of
`kiln_ast::attr` and of the operand table of Kiln's module image
(`kiln_sema::bmi_image::attrs`, which a Kiln test keeps complete).

For an attribute kind, `unbuilt` means that no site in Kiln attaches a
node of the kind to a declaration or a statement.

### `kiln_ast::attr::AttrKind`

`kind=consts; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `ALIGNED` | flag | in | tag 1 |  |
| `ASSUME` | flag | in | tag 2 |  |
| `CARRIES_DEPENDENCY` | flag | in | tag 3 |  |
| `DEPRECATED` | flag | in | tag 4 |  |
| `FALLTHROUGH` | flag | in | tag 5 |  |
| `INDETERMINATE` | flag | in | tag 6 |  |
| `LIKELY` | flag | in | tag 7 |  |
| `UNLIKELY` | flag | in | tag 8 |  |
| `MAYBE_UNUSED` | flag | in | tag 9 |  |
| `NODISCARD` | flag | in | tag 10 |  |
| `NORETURN` | flag | in | tag 11 |  |
| `NO_UNIQUE_ADDRESS` | flag | in | tag 12 |  |
| `ANNOTATION` | flag | unsupported | unbuilt | sema moves each into `AnnotArena` (§10.5); no declaration keeps one |
| `ALWAYS_INLINE` | flag | in | tag 14 |  |
| `NOINLINE` | flag | in | tag 15 |  |
| `PACKED` | flag | in | tag 16 |  |
| `USED` | flag | in | tag 17 |  |
| `UNUSED` | flag | in | tag 18 |  |
| `WEAK` | flag | in | tag 19 |  |
| `VISIBILITY` | flag | in | tag 20 |  |
| `SECTION` | flag | in | tag 21 |  |
| `FORMAT` | flag | in | tag 22 |  |
| `CONST` | flag | in | tag 23 |  |
| `PURE` | flag | in | tag 24 |  |
| `COLD` | flag | in | tag 25 |  |
| `NOTHROW` | flag | in | tag 26 |  |
| `ENABLE_IF` | flag | in | tag 27 |  |
| `DIAGNOSE_IF` | flag | in | tag 28 |  |
| `TRIVIAL_ABI` | flag | in | tag 29 |  |
| `PREFERRED_NAME` | flag | in | tag 30 |  |
| `EXCLUDE_FROM_EXPLICIT_INSTANTIATION` | flag | in | tag 31 |  |
| `ABI_TAG` | flag | in | tag 32 |  |
| `INTERNAL_LINKAGE` | flag | in | tag 33 |  |
| `LIFETIMEBOUND` | flag | in | tag 34 |  |
| `NO_SANITIZE` | flag | in | tag 35 |  |
| `MODE` | flag | in | tag 36 | kept on a declaration only while its type is dependent (`MODE.flags-deferred`); otherwise consumed into the declared type |
| `VECTOR_SIZE` | flag | unsupported | unbuilt | consumed into the declared type, never attached |
| `ASM_LABEL` | flag | in | tag 38 |  |
| `BUILTIN` | flag | in | tag 39 | the implicit attribute of a lazily declared builtin |
| `EXT_VECTOR_TYPE` | flag | unsupported | unbuilt | consumed into the declared type, never attached |
| `MALLOC` | flag | in | tag 41 |  |
| `ALLOC_SIZE` | flag | in | tag 42 |  |
| `ALLOC_ALIGN` | flag | in | tag 43 |  |
| `USING_IF_EXISTS` | flag | in | tag 44 |  |
| `HOT` | flag | in | tag 45 |  |
| `FLAG_ENUM` | flag | in | tag 46 |  |
| `ENUM_EXTENSIBILITY` | flag | in | tag 47 |  |
| `CORO_ONLY_DESTROY_WHEN_COMPLETE` | flag | in | tag 48 |  |
| `OPTNONE` | flag | in | tag 49 |  |
| `MINSIZE` | flag | in | tag 50 |  |
| `NAKED` | flag | in | tag 51 |  |
| `MAY_ALIAS` | flag | in | tag 52 |  |
| `NONNULL` | flag | in | tag 53 |  |
| `RETURNS_NONNULL` | flag | in | tag 54 |  |
| `INIT_PRIORITY` | flag | in | tag 55 |  |
| `LOOP_HINT` | flag | in | tag 56 |  |
| `UNINITIALIZED` | flag | in | tag 57 |  |
| `TYPE_VISIBILITY` | flag | in | tag 58 |  |
| `TYPE_NULLABLE` | flag | in | tag 59 |  |
| `NEON_VECTOR_TYPE` | flag | unsupported | unbuilt | consumed into the declared type, never attached |
| `NEON_POLYVECTOR_TYPE` | flag | unsupported | unbuilt | consumed into the declared type, never attached |
| `TARGET` | flag | in | tag 62 |  |
| `GNU_INLINE` | flag | in | tag 63 |  |
| `NODEBUG` | flag | in | tag 64 |  |
| `MIN_VECTOR_WIDTH` | flag | in | tag 65 |  |
| `WEAKREF` | flag | in | tag 66 |  |
| `ALIAS` | flag | in | tag 67 |  |
| `ANALYZER_NORETURN` | flag | in | tag 68 |  |
| `DISABLE_TAIL_CALLS` | flag | in | tag 69 |  |
| `IFUNC` | flag | in | tag 70 |  |
| `ARTIFICIAL` | flag | in | tag 71 |  |
| `CONSTRUCTOR` | flag | in | tag 72 |  |
| `DESTRUCTOR` | flag | in | tag 73 |  |
| `CLEANUP` | flag | in | tag 74 |  |
| `COMMON` | flag | unsupported | unbuilt | rejected in C++ and never attached |
| `TLS_MODEL` | flag | in | tag 76 |  |
| `RETURNS_TWICE` | flag | in | tag 77 |  |
| `PASS_OBJECT_SIZE` | flag | in | tag 78 |  |
| `OVERLOADABLE` | flag | in | tag 79 |  |
| `MUSTTAIL` | flag | in | tag 80 |  |
| `TRANSPARENT_UNION` | flag | in | tag 81 |  |
| `SELECTANY` | flag | in | tag 82 |  |
| `MS_STRUCT` | flag | in | tag 83 |  |
| `GCC_STRUCT` | flag | in | tag 84 |  |
| `RETAIN` | flag | in | tag 85 |  |
| `NO_DESTROY` | flag | in | tag 86 |  |
| `ALWAYS_DESTROY` | flag | in | tag 87 |  |
| `CALLING_CONV` | flag | unsupported | unbuilt | consumed into the function type (`FuncInfo`), never attached |
| `NO_CALLER_SAVED_REGS` | flag | unsupported | unbuilt | consumed into the function type (`FuncInfo`), never attached |
| `REGPARM` | flag | unsupported | unbuilt | consumed into the function type (`FuncInfo`), never attached |
| `NO_BUILTIN` | flag | in | tag 91 |  |
| `ARM_SVE_VECTOR_BITS` | flag | unsupported | unbuilt | consumed into the declared type, never attached |
| `BUILTIN_ALIAS` | flag | in | tag 93 |  |
| `TARGET_VERSION` | flag | in | tag 94 |  |
| `TARGET_CLONES` | flag | in | tag 95 |  |
| `CPU_DISPATCH` | flag | unsupported | unbuilt | never attached |
| `CPU_SPECIFIC` | flag | unsupported | unbuilt | never attached |
| `NO_STACK_PROTECTOR` | flag | in | tag 98 |  |
| `PATCHABLE_FUNCTION_ENTRY` | flag | in | tag 99 |  |
| `ZERO_CALL_USED_REGS` | flag | in | tag 100 |  |
| `SPECULATIVE_LOAD_HARDENING` | flag | in | tag 101 |  |
| `NO_SPECULATIVE_LOAD_HARDENING` | flag | in | tag 102 |  |
| `NO_SPLIT_STACK` | flag | in | tag 103 |  |
| `NO_INSTRUMENT_FUNCTION` | flag | in | tag 104 |  |
| `NO_PROFILE_INSTRUMENT_FUNCTION` | flag | in | tag 105 |  |
| `XRAY_INSTRUMENT` | flag | in | tag 106 |  |
| `XRAY_LOG_ARGS` | flag | in | tag 107 |  |
| `DISABLE_SANITIZER_INSTRUMENTATION` | flag | in | tag 108 |  |
| `NOCF_CHECK` | flag | in | tag 109 |  |
| `FUNCTION_RETURN` | flag | in | tag 110 |  |
| `CFI_CANONICAL_JUMP_TABLE` | flag | in | tag 111 |  |
| `FORCE_ALIGN_ARG_POINTER` | flag | in | tag 112 |  |
| `FLATTEN` | flag | in | tag 113 |  |
| `LEAF` | flag | in | tag 114 |  |
| `NOMERGE` | flag | in | tag 115 |  |
| `NOT_TAIL_CALLED` | flag | in | tag 116 |  |
| `NODUPLICATE` | flag | in | tag 117 |  |
| `CONVERGENT` | flag | in | tag 118 |  |
| `NOUWTABLE` | flag | in | tag 119 |  |
| `ASSUME_ALIGNED` | flag | in | tag 120 |  |
| `ALIGN_VALUE` | flag | in | tag 121 |  |
| `CALLBACK` | flag | in | tag 122 |  |
| `NOESCAPE` | flag | in | tag 123 |  |
| `MAYBE_UNDEF` | flag | in | tag 124 |  |
| `CODE_ALIGN` | flag | in | tag 125 |  |
| `LOADER_UNINITIALIZED` | flag | in | tag 126 |  |
| `LTO_VISIBILITY_PUBLIC` | flag | in | tag 127 |  |
| `OMP_ASSUME` | flag | in | tag 128 |  |
| `STANDALONE_DEBUG` | flag | in | tag 129 |  |
| `CORO_AWAIT_ELIDABLE` | flag | in | tag 130 |  |
| `CORO_AWAIT_ELIDABLE_ARGUMENT` | flag | in | tag 131 |  |
| `GSL_OWNER` | flag | in | tag 132 |  |
| `GSL_POINTER` | flag | in | tag 133 |  |
| `UNAVAILABLE` | flag | in | tag 134 |  |
| `REQUIRE_CONSTANT_INIT` | flag | in | tag 135 |  |
| `ANNOTATE` | flag | in | tag 136 |  |
| `SME_TYPE` | flag | unsupported | unbuilt | consumed into the function type (`FuncInfo`), never attached |
| `ARM_LOCALLY_STREAMING` | flag | in | tag 138 |  |
| `ARM_NEW` | flag | in | tag 139 |  |
| `NONSTRING` | flag | in | tag 140 |  |
| `NOCOMMON` | flag | in | tag 141 |  |
| `PRAGMA_CLANG_SECTION` | flag | in | tag 142 |  |
| `CODE_MODEL` | flag | in | tag 143 |  |
| `X86_INTERRUPT` | flag | in | tag 144 | a node whose `a` is 1 failed its check and is dropped before it is kept |
| `WEAK_IMPORT` | flag | in | tag 145 |  |
| `SWIFT_PARAM_ABI` | flag | in | tag 146 |  |
| `ADDRESS_SPACE` | flag | unsupported | unbuilt | consumed into the declared type, never attached |

### `kiln_ast::attr::AttrNode`

`kind=struct; pin=schema; selector=kiln_ast::attr::AttrKind`

The rows give the node as Kiln leaves it once the declaration it is on
has been checked, which the bit `CHECKED` says. Before that, several
kinds hold their arguments as written (`CLEANUP`, `TLS_MODEL`,
`CONSTRUCTOR`, `DESTRUCTOR` the first argument's expression and the
argument count; `VISIBILITY` `u32::MAX`). Every attribute of a
declaration's list or of an `AttributedStmt` is checked once the
translation unit is done, and an implicit one is built checked; a node
the walk reaches without the bit makes its unit indeterminate
(`Unaccounted`). An argument that depends on a template parameter keeps
its deferred form under its own per-kind bit (`flags-priority-dependent`,
`MODE.flags-deferred`, `ALIGN_VALUE.flags-value-dependent`) on a checked
node.

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `kind` | flag | in | enum AttrKind |  |
| `syntax` | flag | in | enum AttrSyntax | `[[...]]`, `__attribute__`, a keyword, `__declspec` or a pragma: code generation tells `__attribute__((noreturn))` from `[[noreturn]]` by it |
| `flags-implicit` | flag | in | bit | bit 0: added by sema, not written. An attribute from `#pragma clang attribute` is stored as written, without this bit (`kiln_sema` `pragma_attr.rs:503-518`) |
| `flags-inherited` | bookkeeping | out | - | bit 1: a copy from an earlier declaration, which the list rule leaves out. Kiln sets it for an inherited `ASM_LABEL` and, since WP-2476, for a parameter's attribute copied from the corresponding parameter of the previous declaration |
| `flags-gnu-scoped` | written | out | - | bit 3: written with the `gnu::` scope |
| `flags-checked` | bookkeeping | out | - | bit 6: the declaration checks ran on the node; set on every node of a finished translation unit, and a node without it makes the unit indeterminate (rule above) |
| `loc` | location | out | - |  |
| `ASSUME.flags-ignored` | flag | in | bit | bit 2: the assumption may have side effects, so it is neither assumed nor checked |
| `FALLTHROUGH.flags-scoped` | written | out | - | bit 2: written with an attribute scope |
| `NODISCARD.flags-clang-scoped` | written | out | - | bit 2: spelled `clang::warn_unused_result` |
| `LIFETIMEBOUND.flags-args` | written | out | - | bit 2: an argument list was written, which Kiln does not check in a type position |
| `CONSTRUCTOR.flags-priority-dependent` | flag | in | bit | bit 2: the priority is value-dependent |
| `DESTRUCTOR.flags-priority-dependent` | flag | in | bit | bit 2: the priority is value-dependent |
| `MODE.flags-deferred` | flag | in | bit | bit 2: applies at instantiation |
| `ALIGN_VALUE.flags-value-dependent` | flag | in | bit | bit 2: the alignment is value-dependent (Kiln WP-2476, `kiln_ast/src/attr.rs:884-888`), so `a` is the expression |
| `GSL_OWNER.flags-has-type` | flag | in | bit | bit 2: a type argument was written |
| `GSL_POINTER.flags-has-type` | flag | in | bit | bit 2: a type argument was written |
| `DIAGNOSE_IF.flags-error` | flag | in | bit | bit 2: the diagnostic is an error, not a warning |
| `DIAGNOSE_IF.flags-arg-dependent` | flag | in | bit | bit 4: the condition names a parameter or `this` |
| `DIAGNOSE_IF.flags-group` | flag | in | bit | bit 5: a warning group was written; the node does not keep its name |
| `DEPRECATED.a` | child | in | child? | the message |
| `DEPRECATED.b` | child | in | child? | a second argument, if any |
| `NODISCARD.a` | child | in | child? | the message |
| `NODISCARD.b` | child | in | child? | a second argument, if any |
| `UNAVAILABLE.a` | child | in | child? | the message |
| `UNAVAILABLE.b` | child | in | child? | a second argument, if any |
| `ALLOC_SIZE.a` | child | in | child? | the one-based parameter index, an integer literal |
| `ALLOC_SIZE.b` | child | in | child? | the second index |
| `ALLOC_ALIGN.a` | child | in | child? | the one-based parameter index, an integer literal |
| `ALLOC_ALIGN.b` | child | in | child? |  |
| `ENABLE_IF.a` | child | in | child? | the condition; mangled |
| `ENABLE_IF.b` | child | in | child? | the message |
| `DIAGNOSE_IF.a` | child | in | child? | the condition |
| `DIAGNOSE_IF.b` | child | in | child? | the message |
| `ALIGNED.a` | child | in | child? | the alignment expression |
| `ALIGNED.b` | type | in | type? | the type of `alignas(type-id)` |
| `ASSUME.a` | child | in | child | the assumption |
| `ASSUME.b` | location | out | - | a raw position |
| `PREFERRED_NAME.b` | type | in | type | the typedef's type; its name is spelling |
| `BUILTIN.a` | bookkeeping | out | - | the builtin's id in Kiln's numbering; the unit's key names the builtin (§11.6) |
| `NONNULL.a` | child | in | child* | the parameter indices, integer literals, of which the check keeps the valid ones; empty names every parameter |
| `ASM_LABEL.a` | value | in | string | the symbol name |
| `SECTION.a` | value | in | string? | the section name |
| `TARGET.a` | value | in | string? | the feature string |
| `WEAKREF.a` | value | in | string? | the target as written; 0 once Kiln has split it into an `ALIAS` |
| `OMP_ASSUME.a` | value | in | string? | the assumption string |
| `PRAGMA_CLANG_SECTION.a` | value | in | string? | the section name |
| `PRAGMA_CLANG_SECTION.b` | flag | in | uv | which section kind: Kiln's index into `CLANG_SECTION_KINDS` ("Open questions") |
| `ALIAS.a` | value | in | string | the aliased symbol's name |
| `ALIAS.b` | location | out | - | a raw position |
| `IFUNC.a` | value | in | string | the resolver's symbol name |
| `IFUNC.b` | location | out | - | a raw position |
| `TARGET_VERSION.a` | value | in | string | the version string |
| `TARGET_VERSION.b` | location | out | - | a raw position |
| `CLEANUP.a` | entity | in | ref | the function called with the variable's address at the end of its scope: a `DeclId` on a checked node |
| `CLEANUP.b` | location | out | - | the argument's raw position on a checked node |
| `ANNOTATE.a` | child | in | child? | the string |
| `ANNOTATE.b` | child | in | child* | the extra arguments; 0 is the empty list |
| `CALLBACK.a` | value | in | uv* | the argument indices, checked in place against the declaration |
| `VISIBILITY.a` | flag | in | uv | 0 default, 1 hidden, 2 protected on a checked node; another value makes the unit indeterminate |
| `VISIBILITY.b` | written | out | - | 1 when `hidden` was written `internal` |
| `TYPE_VISIBILITY.a` | flag | in | uv | as `VISIBILITY.a` |
| `FORMAT.a` | flag | in | uv | the archetype: 0 is `printf` |
| `FORMAT.b` | value | in | uv | the format-string index and, shifted by 16, the first data argument index |
| `LIFETIMEBOUND.a` | flag | in | uv | 1 for the implicit object parameter |
| `NO_SANITIZE.a` | flag | in | record SanitizerSet | the excluded set: `a` with the set bits of `b` |
| `NO_SANITIZE.b-set` | bookkeeping | out | - | the high bits of the set, which `NO_SANITIZE.a` writes |
| `NO_SANITIZE.b-spelling` | written | out | - | which `no_sanitize*` spelling |
| `NO_SANITIZE.b-coverage` | flag | in | bool | bit 31: the attribute names `coverage` |
| `ENUM_EXTENSIBILITY.a` | flag | in | uv | 1 open, 0 closed |
| `INIT_PRIORITY.a` | value | in | uv | the priority |
| `MIN_VECTOR_WIDTH.a` | value | in | uv |  |
| `CONSTRUCTOR.a-priority` | value | in | uv? | when `flags-priority-dependent` is clear: the priority, on a checked node (65535 when none is written) |
| `CONSTRUCTOR.a-expression` | child | in | child? | when it is set: the expression |
| `DESTRUCTOR.a-priority` | value | in | uv? | when `flags-priority-dependent` is clear: the priority, on a checked node (65535 when none is written) |
| `DESTRUCTOR.a-expression` | child | in | child? | when it is set: the expression |
| `TLS_MODEL.a` | flag | in | uv | 1 to 4 on a checked node: global-dynamic, local-dynamic, initial-exec, local-exec; another value makes the unit indeterminate |
| `PASS_OBJECT_SIZE.a` | value | in | uv | the object-size type, 0 to 3 |
| `PASS_OBJECT_SIZE.b` | flag | in | uv | 1 for `pass_dynamic_object_size` |
| `PATCHABLE_FUNCTION_ENTRY.a` | value | in | uv |  |
| `PATCHABLE_FUNCTION_ENTRY.b` | value | in | uv |  |
| `ZERO_CALL_USED_REGS.a` | flag | in | uv | Kiln's index into `ZERO_CALL_USED_REGS_KINDS` ("Open questions") |
| `XRAY_INSTRUMENT.a` | flag | in | uv | 1 always, 0 never |
| `XRAY_LOG_ARGS.a` | value | in | uv |  |
| `FUNCTION_RETURN.a` | flag | in | uv | 1 thunk-extern, 0 keep |
| `ASSUME_ALIGNED.a` | value | in | uv | the alignment, folded |
| `ASSUME_ALIGNED.b` | value | in | uv | the offset, folded |
| `ALIGN_VALUE.a-alignment` | value | in | uv? | when `flags-value-dependent` is clear: the alignment |
| `ALIGN_VALUE.a-expression` | child | in | child? | when it is set: the expression |
| `CODE_ALIGN.a` | value | in | uv |  |
| `ARM_LOCALLY_STREAMING.b` | location | out | - | the keyword's index and offsets of its strings from `loc` |
| `ARM_NEW.a` | flag | in | uv | bit 0 ZA, bit 1 ZT0 |
| `ARM_NEW.b` | location | out | - | as `ARM_LOCALLY_STREAMING.b` |
| `CODE_MODEL.a` | flag | in | uv | 1 small, 2 large |
| `SWIFT_PARAM_ABI.a` | flag | in | enum ParamAbi |  |
| `ABI_TAG.a` | name | in | sym | the tags: mangled |
| `NO_BUILTIN.a` | name | in | sym | the builtin names, sorted and joined by Kiln; `*` for all |
| `BUILTIN_ALIAS.a` | name | in | sym | the builtin called |
| `BUILTIN_ALIAS.b` | written | out | - | 1 for the `__clang_arm_builtin_alias` spelling |
| `TARGET_CLONES.a` | name | in | sym | the clone list as Kiln normalises it |
| `TARGET_CLONES.b` | flag | in | uv | 1 when the list was kept |
| `LOOP_HINT.a` | child | in | child? | the value as written |
| `LOOP_HINT.b` | flag | in | record LoopHint |  |
| `MODE.a` | value | in | uv | the mode: width and class, in Kiln's packing ("Open questions") |
| `MODE.b` | written | out | - | the mode name as written; `MODE.a` is the fact |
| `GSL_OWNER.b` | type | in | type? | the type argument; absent when `flags-has-type` is clear |
| `GSL_POINTER.b` | type | in | type? | the type argument; absent when `flags-has-type` is clear |

### `kiln_ast::attr::AttrSyntax`

`kind=enum; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Cxx11` | flag | in | tag 1 |  |
| `Gnu` | flag | in | tag 2 |  |
| `Keyword` | flag | in | tag 3 |  |
| `Declspec` | flag | in | tag 4 |  |
| `Pragma` | flag | in | tag 5 |  |

### `kiln_ast::attr::LoopHint`

`kind=struct; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `option` | flag | in | enum LoopHintOption |  |
| `state` | flag | in | enum LoopHintState |  |
| `spelling` | flag | in | enum LoopHintSpelling | which pragma: kept because nothing shows that code generation ignores it |

### `kiln_ast::attr::LoopHintOption`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Vectorize` | flag | in | tag 1 |  |
| `VectorizeWidth` | flag | in | tag 2 |  |
| `Interleave` | flag | in | tag 3 |  |
| `InterleaveCount` | flag | in | tag 4 |  |
| `Unroll` | flag | in | tag 5 |  |
| `UnrollCount` | flag | in | tag 6 |  |
| `UnrollAndJam` | flag | in | tag 7 |  |
| `UnrollAndJamCount` | flag | in | tag 8 |  |
| `PipelineDisabled` | flag | in | tag 9 |  |
| `PipelineInitiationInterval` | flag | in | tag 10 |  |
| `Distribute` | flag | in | tag 11 |  |
| `VectorizePredicate` | flag | in | tag 12 |  |

### `kiln_ast::attr::LoopHintState`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Enable` | flag | in | tag 1 |  |
| `Disable` | flag | in | tag 2 |  |
| `Numeric` | flag | in | tag 3 |  |
| `FixedWidth` | flag | in | tag 4 |  |
| `ScalableWidth` | flag | in | tag 5 |  |
| `AssumeSafety` | flag | in | tag 6 |  |
| `Full` | flag | in | tag 7 |  |

### `kiln_ast::attr::LoopHintSpelling`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `ClangLoop` | flag | in | tag 1 |  |
| `Unroll` | flag | in | tag 2 |  |
| `Nounroll` | flag | in | tag 3 |  |
| `UnrollAndJam` | flag | in | tag 4 |  |
| `NounrollAndJam` | flag | in | tag 5 |  |

### `kiln_ast::sanitize::SanitizerSet`

`kind=struct; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `0` | flag | in | bytes*sorted | the `-fsanitize=` name of each member (`SANITIZERS`); the bit positions are Kiln's |

### `kiln_ast::attr::AttrArena`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `nodes` | bookkeeping | out | - | arena: reached through `AttrId` |
| `kinds` | bookkeeping | out | - | which kinds occur at all: a cache |
| `decl_attrs` | flag | in | attrs | by declaration |

### 10.2 Names

A `DeclName` is read through `NameTable::view`. Where a row writes a
`name` it is the `Name` of §3.2, whose first seven tags are the tags of
`DeclNameKind`, followed by the payload rows of `DeclName`. A `Name` in a
key has no payload for a constructor, a destructor or a deduction guide:
the parent key and the discriminator already name the class and the
template (§3.2, §3.3).

### `kiln_ast::names::DeclNameKind`

`kind=enum; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Identifier` | name | in | tag 1 |  |
| `Operator` | name | in | tag 2 |  |
| `Constructor` | name | in | tag 3 |  |
| `Destructor` | name | in | tag 4 |  |
| `Conversion` | name | in | tag 5 |  |
| `LiteralOperator` | name | in | tag 6 |  |
| `DeductionGuide` | name | in | tag 7 |  |

### `kiln_ast::names::DeclName`

`kind=struct; pin=schema; selector=kiln_ast::names::DeclNameKind`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `0-kind` | name | in | enum DeclNameKind | bits 29-31 |
| `Identifier.0-payload` | name | in | sym | the symbol's index plus 1; 0 is the empty name, which no stream row writes (an unnamed entity's key: §3.2) |
| `Operator.0-payload` | name | in | enum OverloadedOperator |  |
| `Constructor.0-payload` | type | in | type | index into `NameTable` `special`: the class; in a stream only |
| `Destructor.0-payload` | type | in | type | as `Constructor.0-payload` |
| `Conversion.0-payload` | type | in | type | index into `NameTable` `special`: the type converted to |
| `LiteralOperator.0-payload` | name | in | sym | the suffix |
| `DeductionGuide.0-payload` | entity | in | ref | index into `NameTable` `special`: the template; in a stream only |

### `kiln_ast::names::OverloadedOperator`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `New` | name | in | tag 1 |  |
| `Delete` | name | in | tag 2 |  |
| `ArrayNew` | name | in | tag 3 |  |
| `ArrayDelete` | name | in | tag 4 |  |
| `Plus` | name | in | tag 5 |  |
| `Minus` | name | in | tag 6 |  |
| `Star` | name | in | tag 7 |  |
| `Slash` | name | in | tag 8 |  |
| `Percent` | name | in | tag 9 |  |
| `Caret` | name | in | tag 10 |  |
| `Amp` | name | in | tag 11 |  |
| `Pipe` | name | in | tag 12 |  |
| `Tilde` | name | in | tag 13 |  |
| `Exclaim` | name | in | tag 14 |  |
| `Equal` | name | in | tag 15 |  |
| `Less` | name | in | tag 16 |  |
| `Greater` | name | in | tag 17 |  |
| `PlusEqual` | name | in | tag 18 |  |
| `MinusEqual` | name | in | tag 19 |  |
| `StarEqual` | name | in | tag 20 |  |
| `SlashEqual` | name | in | tag 21 |  |
| `PercentEqual` | name | in | tag 22 |  |
| `CaretEqual` | name | in | tag 23 |  |
| `AmpEqual` | name | in | tag 24 |  |
| `PipeEqual` | name | in | tag 25 |  |
| `LessLess` | name | in | tag 26 |  |
| `GreaterGreater` | name | in | tag 27 |  |
| `LessLessEqual` | name | in | tag 28 |  |
| `GreaterGreaterEqual` | name | in | tag 29 |  |
| `EqualEqual` | name | in | tag 30 |  |
| `ExclaimEqual` | name | in | tag 31 |  |
| `LessEqual` | name | in | tag 32 |  |
| `GreaterEqual` | name | in | tag 33 |  |
| `Spaceship` | name | in | tag 34 |  |
| `AmpAmp` | name | in | tag 35 |  |
| `PipePipe` | name | in | tag 36 |  |
| `PlusPlus` | name | in | tag 37 |  |
| `MinusMinus` | name | in | tag 38 |  |
| `Comma` | name | in | tag 39 |  |
| `ArrowStar` | name | in | tag 40 |  |
| `Arrow` | name | in | tag 41 |  |
| `Call` | name | in | tag 42 |  |
| `Subscript` | name | in | tag 43 |  |
| `Coawait` | name | in | tag 44 |  |

### `kiln_ast::names::NameTable`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `special` | bookkeeping | out | - | arena: the operands of constructor, destructor, conversion and deduction-guide names |
| `index` | bookkeeping | out | - | interning |

### 10.3 Strings

`string` writes the rows of the literal's `Entry`.

### `kiln_ast::strings::Entry`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `start` | value | in | bytes | the `len` bytes of `StringPool` `bytes` from this offset: the code units as Kiln stores them |
| `len` | bookkeeping | out | - | the length `start` writes |
| `enc` | flag | in | enum StringEncoding |  |

### `kiln_ast::strings::StringEncoding`

`kind=enum; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Ordinary` | flag | in | tag 1 |  |
| `Utf8` | flag | in | tag 2 |  |
| `Utf16` | flag | in | tag 3 |  |
| `Utf32` | flag | in | tag 4 |  |
| `Wide16` | flag | in | tag 5 |  |
| `Wide32` | flag | in | tag 6 |  |

### `kiln_ast::strings::StringPool`

`kind=root; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `bytes` | bookkeeping | out | - | arena: reached through `Entry` |
| `entries` | bookkeeping | out | - | arena: reached through `StringLitId` |
| `table` | bookkeeping | out | - | interning |

### 10.4 Lambdas

A `LambdaExpr` writes its closure type by `decl` (§7); the closure's
`RecordData` `lambda` row writes the rows of `LambdaData`. A closure type
is a local type (§2.4), written in the stream that walks its
`LambdaExpr`, wherever that is: a function body, the initialiser of a
variable at namespace or class scope, a default member initialiser, a
default argument (digest §4.3). It is never a unit and never a member of
its semantic context's lists (`RecordData` `members-other`, a
namespace's members), although Kiln puts a closure made outside a
function body in the member list of its namespace, class or
enumeration.

The mangling number and context are written as facts of the closure, since
mangling reads them, and are not its key: Kiln numbers the lambdas of one
context per signature, as the ABI does, so two closures of one context can
share a number, and a closure the ABI gives no context has none.

A closure's key is `local(n)` under its owner (digest §4.3). For a
closure in a function body the owner is the function, or the call
operator of the lambda whose body holds it: Kiln's semantic context.
For a closure outside a function body it is what Kiln's
`Ast::closure_owner` answers (Kiln WP-2023), taken to the entity that
has a key: a parameter's function, a template parameter's template, and
otherwise the field, variable, alias, function or template itself (a
lambda-expression in a bit-field width is the field's: Kiln gives the
field as owner, and the field's stream walks the width). A consteval
block's synthesized lambda is the block's (digest §4.3 "An owner keyed
by its content"). An owner that is a namespace, a class, an
enumeration or the translation unit (a lambda-expression in a
`static_assert`, in a base-specifier, in an enumerator's initialiser)
has no stream that numbers the closure: the closure has no key, and
what refers to it is indeterminate
(`Unsupported("local/closure-owner")`).

A capture's `var` is `Ref::local` in the stream that holds the
lambda-expression. An init-capture declares its variable: the variable
is a local object owned by the call operator, it has no declaration
statement, and its initialiser is the corresponding entry of `inits`.

### `kiln_ast::lambda::LambdaData`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `closure` | bookkeeping | out | - | the class whose row is writing this record |
| `call_op` | bookkeeping | out | - | the call operator: a member of the closure, written with it |
| `default` | flag | in | enum CaptureDefault | Kiln's ODR hash reads `[=]` with an implicit capture of `x` as `[x]`; the digest does not, on purpose: in a template the default decides what an instantiation captures, and `[=]` captures `this` differently |
| `default_loc` | location | out | - |  |
| `captures` | entity | in | record LambdaCapture* | in closure member order |
| `inits` | child | in | child*? | the initialiser of each capture's member |
| `body` | bookkeeping | out | - | the call operator's body, which `FunctionData` `body` writes; for a coroutine the statement that body wraps |
| `lbracket` | location | out | - |  |
| `rbracket` | location | out | - |  |
| `explicit_params` | written | out | - | a parameter clause was written: `[]{}` against `[](){}`. Instantiation copies it and the module image stores it; nothing in Kiln decides anything by it |
| `explicit_result` | flag | in | bit | a trailing return type was written: instantiation substitutes into the return type only then, and deduces it otherwise |
| `mutable` | flag | in | bit |  |
| `mangling` | value | in | record LambdaMangling |  |

### `kiln_ast::lambda::CaptureDefault`

`kind=enum; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `None` | flag | in | tag 1 |  |
| `ByCopy` | flag | in | tag 2 |  |
| `ByRef` | flag | in | tag 3 |  |

### `kiln_ast::lambda::LambdaCapture`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `kind` | flag | in | enum CaptureKind |  |
| `var` | entity | in | ref |  |
| `field` | entity | in | ref | the member holding the capture |
| `loc` | location | out | - |  |
| `ellipsis-present` | flag | in | bit | a capture pack |
| `ellipsis-position` | location | out | - |  |
| `implicit` | flag | in | bit | see `LambdaData` `default` |
| `init` | flag | in | bit |  |

### `kiln_ast::lambda::CaptureKind`

`kind=enum; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `This` | flag | in | tag 1 |  |
| `StarThis` | flag | in | tag 2 |  |
| `ByCopy` | flag | in | tag 3 |  |
| `ByRef` | flag | in | tag 4 |  |
| `VlaBound` | flag | in | tag 5 |  |

### `kiln_ast::lambda::LambdaMangling`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `number` | value | in | uv | read by mangling; not the closure's key (rule above) |
| `context` | entity | in | ref | read by mangling; names the unit a closure outside a function body is local to, where Kiln stores it |

### `kiln_ast::lambda::LambdaArena`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `records` | bookkeeping | out | - | arena: reached through `LambdaId` |

### 10.5 Annotations

A declaration's stream holds the annotations of every declaration of its
chain (§10.1), in chain order, as one list (`of_decl`); a parameter's are
those of its parameters *i* (§10.1), as one list, the definition's not
apart. Reflection reads them so: `annotations_of` reads every
declaration's, first first, for an entity and for parameter *i* alike
(kiln_sema `meta/annot.rs:147`), and nothing else reads them. A class's
stream then holds one list
for each base specifier of its definition, in order (`of_base`).

### `kiln_ast::annot::Annotation`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `decl` | bookkeeping | out | - | the declaration it is written on, whose stream this is |
| `value` | value | in | value? | the underlying constant; absent while the operand is dependent |
| `loc` | location | out | - |  |
| `written_type` | type | in | type |  |
| `expr` | child | in | child | the operand; the only carrier while it is dependent |

### `kiln_ast::annot::AnnotArena`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `nodes` | bookkeeping | out | - | arena: reached through `AnnId` |
| `of_decl` | value | in | record Annotation* | by declaration; rule above |
| `of_base` | value | in | record Annotation* | by class and base index; rule above |

### 10.6 Tables computed for classes

Everything in this section is computed by Kiln the first time something
needs it, from facts that units hold: a class's bases and virtual
functions, the definitions of the `<compare>` category classes. None of
it is written (§11.7).

### `kiln_ast::class::ClassCache`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `map` | value | demand | - | §11.7: `Ast` `virtuals` and `vtables` |

### `kiln_ast::class::SubobjectId`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `0` | value | demand | - | §11.7 |

### `kiln_ast::class::Subobject`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `class` | entity | demand | - | §11.7 |
| `parent` | value | demand | - | §11.7 |
| `base_index` | value | demand | - | §11.7 |
| `is_virtual` | flag | demand | - | §11.7 |

### `kiln_ast::class::ClassVirtuals`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `subobjects` | value | demand | - | §11.7 |
| `overriders` | entity | demand | - | §11.7 |

### `kiln_ast::class::VTableComponent`

`kind=enum; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `VCallOffset` | value | demand | - | §11.7 |
| `VCallOffset.0` | value | demand | - | §11.7 |
| `VBaseOffset` | value | demand | - | §11.7 |
| `VBaseOffset.0` | value | demand | - | §11.7 |
| `OffsetToTop` | value | demand | - | §11.7 |
| `OffsetToTop.0` | value | demand | - | §11.7 |
| `Rtti` | value | demand | - | §11.7 |
| `Rtti.0` | entity | demand | - | §11.7 |
| `Func` | value | demand | - | §11.7 |
| `Func.0` | entity | demand | - | §11.7 |
| `CompleteDtor` | value | demand | - | §11.7 |
| `CompleteDtor.0` | entity | demand | - | §11.7 |
| `DeletingDtor` | value | demand | - | §11.7 |
| `DeletingDtor.0` | entity | demand | - | §11.7 |
| `UnusedFunc` | value | demand | - | §11.7 |
| `UnusedFunc.0` | entity | demand | - | §11.7 |

### `kiln_ast::class::Thunk`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `this` | value | demand | - | §11.7 |
| `ret` | value | demand | - | §11.7 |
| `method` | entity | demand | - | §11.7 |
| `this_class` | entity | demand | - | §11.7 |

### `kiln_ast::class::VTableLayout`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `components` | value | demand | - | §11.7 |
| `vtable_starts` | value | demand | - | §11.7 |
| `address_points` | value | demand | - | §11.7 |
| `thunks` | value | demand | - | §11.7 |
| `method_thunks` | entity | demand | - | §11.7 |
| `vbase_offset_offsets` | value | demand | - | §11.7 |
| `method_indices` | value | demand | - | §11.7 |

### `kiln_ast::class::VttComponent`

`kind=struct; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `vtable_index` | value | demand | - | §11.7 |
| `base` | value | demand | - | §11.7 |

### `kiln_ast::class::CmpCategories`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `infos` | value | demand | - | §11.7 |

### `kiln_ast::class::CmpCategoryInfo`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `kind` | flag | demand | - | §11.7 |
| `record` | entity | demand | - | §11.7 |
| `field` | entity | demand | - | §11.7 |
| `values` | entity | demand | - | §11.7 |
| `ints` | value | demand | - | §11.7 |

### `kiln_ast::class::CmpCategory`

`kind=enum; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Partial` | flag | demand | - | §11.7 |
| `Weak` | flag | demand | - | §11.7 |
| `Strong` | flag | demand | - | §11.7 |

### `kiln_ast::class::CmpResult`

`kind=enum; pin=extra`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Equal` | flag | demand | - | §11.7: indexes `CmpCategoryInfo` `values` |
| `Equivalent` | flag | demand | - | §11.7: indexes `CmpCategoryInfo` `values` |
| `Less` | flag | demand | - | §11.7: indexes `CmpCategoryInfo` `values` |
| `Greater` | flag | demand | - | §11.7: indexes `CmpCategoryInfo` `values` |
| `Unordered` | flag | demand | - | §11.7: indexes `CmpCategoryInfo` `values` |

### 10.7 Modules

When the translation unit is a module unit or imports one, a
declaration's stream holds its owner (`ModuleTable::owner`, from the
first declaration, §2.3) and, for a specialisation or member explicitly
instantiated, the unit that holds the definition. Otherwise both rows
have their empty form: when `active` is clear, and when it is set but
unit 0 is of kind `NonModule` with no imports. Kiln sets `active` for
any line of the main file that starts with `module`, `import` or
`export`, a skipped line included (`kiln_pp/src/modules.rs:403-411`), so
for such a unit it says only how the text is laid out
(`spec/landed/digest-kinds/WP-033.md`).

Declarations of an imported unit are in the importing translation
unit's `Ast` like its own: `ModuleTable` `owner` of one has a `unit`
other than 0 (Kiln's `is_imported_named_unit_decl` says the same), and
`UnitInfo` `merged` lists the ranges of ids the main unit declared
again. Whether they are units of the importing translation unit is not
decided ("Hazards accepted for now", modules).

### `kiln_ast::modules::ModuleTable`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `units` | bookkeeping | out | - | arena: reached through `Owner` `unit` |
| `points` | entity | in | record Owner? | by declaration: the owner of every declaration from an index on |
| `active` | bookkeeping | out | - | whether ownership is tracked at all; set by the text alone for a unit that is no module unit (see above) |
| `links` | bookkeeping | out | - | state of a sema check, drained by it |
| `explicit_defs` | entity | in | record UnitInfo? | by declaration: the unit whose text holds its explicit instantiation definition |

### `kiln_ast::modules::Owner`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `unit` | entity | in | record UnitInfo | index into `ModuleTable` `units` |
| `purview` | flag | in | bit |  |
| `exported` | flag | in | bit |  |

### `kiln_ast::modules::UnitInfo`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `key` | location | out | - | the preprocessor's key: the module name again, or a header's path |
| `kind` | flag | in | enum UnitKind |  |
| `module` | name | in | bytes? | the module's name |
| `partition` | name | in | bytes? |  |
| `imports` | entity | in | bytes* | the units it imports, in order: the order fixes that of the module initialisers. Written in the order unit (rule below) |
| `merged` | bookkeeping | out | - | ranges of declaration ids |

`imports` is written once, in the order unit (digest §2.1), for the
`UnitInfo` of the translation unit itself: a count, then for each import
in stored order its key as `bytes` and `bool` for `export import`. The
key of a header unit is a path, written through the path rule of digest
§9. A `UnitInfo` written for an `Owner` or for `explicit_defs` writes no
imports.

### `kiln_ast::modules::UnitKind`

`kind=enum; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `NonModule` | flag | in | tag 1 |  |
| `Header` | flag | in | tag 2 |  |
| `Interface` | flag | in | tag 3 |  |
| `Implementation` | flag | in | tag 4 |  |
| `PartitionInterface` | flag | in | tag 5 |  |
| `PartitionImplementation` | flag | in | tag 6 |  |

### 10.8 Reflection

No part of reflection is digested by the first implementation (§12). The
tables are reached only through `ReflectionId`, which only `unsupported`
rows hold.

### `kiln_ast::reflect::Reflectee`

`kind=enum; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Null` | value | unsupported | - | reflection (§12) |
| `Type` | value | unsupported | - | reflection (§12) |
| `Type.0` | type | unsupported | - | reflection (§12) |
| `Decl` | value | unsupported | - | reflection (§12) |
| `Decl.0` | entity | unsupported | - | reflection (§12) |
| `Namespace` | value | unsupported | - | reflection (§12) |
| `Namespace.0` | entity | unsupported | - | reflection (§12) |
| `NamespaceAlias` | value | unsupported | - | reflection (§12) |
| `NamespaceAlias.0` | entity | unsupported | - | reflection (§12) |
| `Template` | value | unsupported | - | reflection (§12) |
| `Template.0` | entity | unsupported | - | reflection (§12) |
| `Value` | value | unsupported | - | reflection (§12) |
| `Value.0` | value | unsupported | - | reflection (§12) |
| `Object` | value | unsupported | - | reflection (§12) |
| `Object.0` | value | unsupported | - | reflection (§12) |
| `BaseSpec` | value | unsupported | - | reflection (§12) |
| `BaseSpec.0` | entity | unsupported | - | reflection (§12) |
| `BaseSpec.1` | value | unsupported | - | reflection (§12) |
| `DataMemberSpec` | value | unsupported | - | reflection (§12) |
| `DataMemberSpec.0` | value | unsupported | - | reflection (§12) |
| `Annotation` | value | unsupported | - | reflection (§12) |
| `Annotation.0` | value | unsupported | - | reflection (§12) |
| `FnParam` | value | unsupported | - | reflection (§12) |
| `FnParam.0` | entity | unsupported | - | reflection (§12) |
| `FnParam.1` | value | unsupported | - | reflection (§12) |

### `kiln_ast::reflect::DmsDesc`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `ty` | type | unsupported | - | reflection (§12) |
| `name` | name | unsupported | - | reflection (§12) |
| `alignment` | value | unsupported | - | reflection (§12) |
| `bit_width` | value | unsupported | - | reflection (§12) |
| `nua` | flag | unsupported | - | reflection (§12) |
| `annotations` | value | unsupported | - | reflection (§12) |

### `kiln_ast::reflect::ReflectionTable`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `items` | bookkeeping | out | - | arena: reached through `ReflectionId` |
| `map` | bookkeeping | out | - | interning |
| `by_decl` | bookkeeping | out | - | interning |
| `limit` | bookkeeping | out | - | a capacity |
| `dms` | bookkeeping | out | - | arena: reached through `Reflectee` `DataMemberSpec` |

### 10.9 The root

Three fields of `Ast` are options of the compilation, not facts of any
declaration: `contract_semantic`, `lang_c` and `freestanding`. They are
not written: a digest is relative to its command (digest §1 rule 8).

### `kiln_ast::ast::Ast`

`kind=root; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `types` | bookkeeping | out | - | `TypeContext` (§4.7) |
| `decls` | bookkeeping | out | - | `DeclArena` (§5) |
| `exprs` | bookkeeping | out | - | `ExprArena` (§7.4) |
| `stmts` | bookkeeping | out | - | `StmtArena` (§6) |
| `attrs` | bookkeeping | out | - | `AttrArena` (§10.1) |
| `annots` | bookkeeping | out | - | `AnnotArena` (§10.5) |
| `lists` | bookkeeping | out | - | `IdLists` |
| `strings` | bookkeeping | out | - | `StringPool` (§10.3) |
| `names` | bookkeeping | out | - | `NameTable` (§10.2) |
| `lookup` | bookkeeping | out | - | `LookupTables` (§5) |
| `redecls` | bookkeeping | out | - | `RedeclTable` (§5) |
| `templates` | bookkeeping | out | - | `TemplateTables` (§9.5) |
| `consts` | bookkeeping | out | - | `ConstStore` (§8.1) |
| `reflections` | bookkeeping | out | - | `ReflectionTable` (§10.8) |
| `layouts` | value | demand | - | §11.7: `LayoutCache` (§4.7) |
| `virtuals` | value | demand | - | §11.7 |
| `vtables` | value | demand | - | §11.7 |
| `cmp_categories` | value | demand | - | §11.7 |
| `tu` | entity | out | - | the translation unit's declaration: `Parent::root` |
| `modules` | bookkeeping | out | - | `ModuleTable` (§10.7) |
| `contract_semantic` | flag | out | - | an option of the compilation: rule above |
| `lang_c` | flag | out | - | an option of the compilation: rule above |
| `freestanding` | flag | out | - | an option of the compilation: rule above |
| `contract_sites` | value | in | record ContractSite? | by statement: written by its `ContractAssertStmt` (§6) |
| `symbols` | bookkeeping | out | - | `Interner` |
| `inst` | bookkeeping | out | - | `InstantiationMeter` |
| `decl_extents` | location | out | - | `DeclExtents`: the extents of each declaration as written, off unless a client turns it on |
| `split_greaters` | location | out | - | the `>` tokens split off a `>>`, `>=` or `>>=` to close a template argument list, by location (Kiln WP-2542, `kiln_ast/src/ast.rs:110`): where a token ends, which no unit holds |

`Ast` `written_refs`, the table of references as written, is transient
and has no descriptor; like `decl_extents` it records positions only and
is off unless a client turns it on, so neither is written.

### `kiln_ast::decl_extents::DeclExtents`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `on` | bookkeeping | out | - | whether extents are recorded |
| `rows` | location | out | - |  |
| `index` | bookkeeping | out | - | by declaration: its row |
| `more` | location | out | - | the later rows of a declaration written more than once under one `DeclId` (Kiln WP-2470) |
| `modules` | location | out | - | `ModuleDeclExtent`: the module and import declarations as written (Kiln WP-2091); what they declare is in `ModuleTable` (§13) |
| `attrs` | location | out | - | `WrittenAttr`: the attributes written on an explicit instantiation, as written (Kiln WP-2541); the attributes the walk reads are the specialisation's own list (§10.1) |

### `kiln_ast::decl_extents::DeclExtent`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `full` | location | out | - |  |
| `own` | location | out | - |  |
| `name` | location | out | - |  |
| `qual` | location | out | - |  |
| `body` | location | out | - |  |
| `specs_end` | location | out | - |  |
| `end` | location | out | - | which token ends the declaration text |

### `kiln_ast::decl_extents::ModuleDeclExtent`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `full` | location | out | - |  |
| `export` | location | out | - |  |
| `keyword` | location | out | - |  |
| `name` | location | out | - |  |
| `partition` | location | out | - |  |
| `attrs` | location | out | - |  |
| `semi` | location | out | - |  |
| `before` | location | out | - | the next declaration's index when it was recorded: a position |
| `kind` | location | out | - | which kind of declaration the extent is of; the declaration itself is in `ModuleTable` (§13) |

### `kiln_ast::decl_extents::ModuleDeclKind`

`kind=enum; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Module` | location | out | - |  |
| `Import` | location | out | - |  |
| `HeaderImport` | location | out | - |  |

### `kiln_ast::decl_extents::WrittenAttr`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `kind` | location | out | - | of a row of the extents table, recorded only when a client turns it on |
| `syntax` | location | out | - | of a row of the extents table, recorded only when a client turns it on |
| `span` | location | out | - |  |

### `kiln_ast::decl_extents::DeclEnd`

`kind=enum; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `Other` | location | out | - |  |
| `Semi` | location | out | - |  |
| `Brace` | location | out | - |  |

### `kiln_source::loc::Span`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `lo` | location | out | - |  |
| `hi` | location | out | - |  |

### `kiln_ast::ast::InstantiationMeter`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `depth` | bookkeeping | out | - | a meter of memory retained by instantiation |
| `start` | bookkeeping | out | - |  |
| `retained` | bookkeeping | out | - |  |
| `count` | bookkeeping | out | - |  |

### `kiln_ast::lists::IdLists`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `spans` | bookkeeping | out | - | arena: reached through `ListId` |
| `data` | bookkeeping | out | - | arena |

### `kiln_source::symbol::Interner`

`kind=root; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `strings` | bookkeeping | out | - | arena: a `Symbol` is written by its spelling |

### `kiln_source::symbol::Symbol`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `0` | name | in | sym |  |

### `kiln_source::loc::Loc`

`kind=struct; pin=schema`

| Row | Class | Decision | Encoding | Note |
|---|---|---|---|---|
| `0` | location | out | - |  |

## 11. Demand-driven fields

Kiln fills the fields decided `demand` only when the translation unit
asks. This section gives, for each, the rule that makes the digest of
every unit independent of what was asked. There are three rules:

- the fact goes to a **resolved unit** (digest §2.2), which exists only
  if Kiln computed it, so that a unit present on both sides of a
  comparison holds it on both or lacks it on both (§11.1 to §11.5,
  §11.9);
- the fact is a property of a **derived unit** that exists only if
  something referenced the entity (§11.4, §11.6);
- the fact is **not written**, because it is computed from facts that
  units hold, or records only that something was asked (§11.7, §11.8).

### 11.1 Resolved properties

A resolved unit has the key kind `resolved`, its parent is the unit the
fact belongs to, and its name is `property` with one of these numbers.
The numbers are Slip's own and are never reused.

| Number | Property | Belongs to | Stream |
|---|---|---|---|
| 1 | `exception_spec` | a function of the deferrable class (§11.2) | `enum ExcSpec`, in normal form |
| 2 | `value` | a variable | `value?`, then `value?` per extended temporary (§11.3) |
| 3 | retired | | was `overrides`, now a row of the function's own unit (§11.5) |
| 4 | `key_function` | a class | `ref` |
| 5 | `dtor_delete` | a virtual destructor | `ref`: `ColdDeclData` `dtor_delete`; then `child?`: `dtor_delete_this` |
| 6 | `inherited_constructor` | an inheriting constructor | `ref`: `inheriting_ctor`; then `ref`: `inherited_ctor` |
| 7 | `default_immediates` | a function, a field or a `default_argument` unit | `count`, then `value` for each (§11.4) |
| 8 | `definition` | an entity of the definition class, or one whose definition an instantiation added outside its class (§11.9) | the rows §11.9 lists, in its order |

A resolved unit `default_immediates` has a discriminator: the index of
the parameter as `uv`, absent for a field. A discriminator is encoded as
the digest of a stream holding that `uv` alone, finished with the
discriminator tag `slip.disc.v1` (`DISC_TAG`), as the discriminator of
any key (§3.3); no other resolved
property has one.

How a resolved unit is written (`demand::write_resolved`), for a
re-implementer:

- **Its stream** starts from a fresh unit state: no local entity is
  numbered, no expression state carries over from the owner's own
  stream, and the walk's state is put back afterwards. Local entities
  met in it (a function's parameters, the body's locals) are numbered in
  it from zero. Nothing of the owner's stream is replayed into it.
- **Its key** is `{resolved, Parent::Unit(<owner's key id>),
  property(<number>), <discriminator>}`, with the rule table's mapping
  applied as for every key. A unit record computes it again from the
  owner's key and the property (`demand::part_key`).
- **Its cause** is the cause of its stream merged with the cause the
  owner's key carries (an entity whose key is indeterminate makes every
  resolved unit of it indeterminate), as a key's cause passes to its
  children.
- **Its file** is the origin file of the entity the owner's unit is read
  from (`units::origin_file`: through `SpecInfo` `template` and
  `MemberSpecInfo` `instantiated_from` to the first declaration of what
  the entity was instantiated from); a namespace's resolved unit, of the
  `tu` group, has none. It is a derived unit, whatever the owner's
  class (digest §2.2).

**A unit's file and instantiation.** The file of the declared unit of
an entity whose effective kind (§11.10) is an instantiation and which
has a `MemberSpecInfo` of its own (a member function, static data
member, member class or enumeration of a specialisation; a field has
none, and Kiln does not move one) or is a function is the origin file
as above,
never the file of the location the instantiation holds now: Kiln moves
such a function to the pattern's definition when it instantiates the
definition (`relocate_to_pattern_definition`,
`kiln_ast/src/templates.rs:1989`, called at
`kiln_sema/src/templ/class.rs:1597` and `:1700`), so the location
depends on whether the definition was instantiated. A class or variable
specialisation (with its own `SpecInfo`) keeps the file of its first
declaration (`units::declared_file`): Kiln moves only an implicit
instantiation of a class to its pattern's definition when it defines it
(`relocate_spec`, `kiln_sema/src/templ/class.rs:1311`; an explicit
instantiation stays at its own template-name), and the unit of an
implicit instantiation is derived, whose file is the origin file
already (digest §11).

### 11.2 Deferred exception specifications

Kiln stores `ExcSpec::Deferred(d)` in a function's type until something
needs the specification, and then re-points the declaration's type. A
function is of the **deferrable class** if Kiln can do that to it:

1. it is an implicitly declared special member function or comparison
   operator (every declaration of the entity is `IMPLICIT`), or is
   explicitly defaulted on the entity's representative declaration
   (`Defaulted` `Explicit` or `Implicit` there). Kiln defers one written
   without a specification and, once it resolves it, re-points every
   declaration's type, so `Ast` does not say whether a specification was
   written: every function explicitly defaulted on that declaration is of
   the class, and a written specification is in its resolved unit;
2. it is an inheriting constructor (by rule 1, or by its
   `ColdDeclData` `inheriting_ctor` entry);
3. its representative declaration was made by instantiation, of any form
   (an implicit instantiation, or a member of an explicit instantiation
   declaration or definition), as §11.9 tells it
   (`templ::made_by_instantiation`: the declaration's own `SpecInfo` or
   `MemberSpecInfo`, or the instantiation that governs its lexical
   context; a redeclaration written in source is not, whatever its
   entity's kind, and Kiln resolves the specification at it), and its pattern has a value-dependent
   `noexcept` operand, a dependent `noexcept` without an operand, a
   dynamic specification that names a dependent type, or a `Deferred`
   specification on any of its declarations, or is itself of the
   deferrable class. The pattern of a function template's specialisation
   is the primary template's pattern (`SpecInfo` `template`; `from` names
   the template until the definition is instantiated); of any other
   instantiated declaration, `MemberSpecInfo` `instantiated_from` or
   Kiln's map (`Ast::instantiated_from`); with no pattern found, or
   patterns deeper than 64, the function is of the class. Kiln defers the
   specification under every form of instantiation and resolves it when
   it defines the function, so an explicit instantiation definition
   resolves the members it defines and leaves the others deferred;
4. it is a destructor. Kiln defers one written without a specification,
   and once the specification is resolved `Ast` does not say whether it
   was written (`implicit_exc_dtors` is in Kiln's semantic analysis), so
   every destructor is of the class.

Every rule reads the entity's representative declaration
(`spec/digest.md` §2.1), not Kiln's first. The class over-includes: a
function in it whose specification Kiln never defers has its resolved
unit in every parse, which is safe; leaving out a function Kiln can defer
would not be.

The predicate reads only stored facts that do not change when the
specification is resolved. For a function of the class:

- its own unit never writes an exception specification
  (`DeclNode` `ty-exception-spec` is absent);
- if the specification is resolved (its type no longer holds `Deferred`),
  the resolved unit `exception_spec` holds it; if not, there is no such
  unit.

A member of a local class is the exception: it has no key, so no
resolved unit can hold its specification. Its own stream (in the stream
of the local class, inside its enclosing unit) writes the specification
as Kiln holds it whenever Kiln holds a written or resolved one, and is
indeterminate (`Pending(Local)`) only while Kiln still holds it
`Deferred` (`tests/differ/_/local-class-destructor-noexcept`,
`local-class-defaulted-noexcept`).

A type digest that meets `Deferred(d)` writes the tag of `Deferred` and
the `Ref` of `d`. A declaration outside the class whose type holds
`Deferred` makes its unit indeterminate: the predicate above was derived
from Kiln's deferral sites (`kiln_sema` `except.rs`, `templ/subst/decls.rs`,
`class/special/implicit.rs`, `class/inhctor.rs`) and is not a Kiln
interface ("Open questions").

Kiln also defers the `noexcept` of a member function or friend that is
parsed late (at the end of its class), and one nothing uses can stay
`Deferred` to the end of the translation unit. No stored fact that
survives resolution marks such a function, so it is outside the class:
resolved, its own unit writes the specification; still deferred, its
unit is indeterminate by the rule above ("Open questions" has what
would bring it into the class).

Kiln resolves a specification when something reads it and when it
defines the function, so a resolved `exception_spec` unit can exist on
one side only with no demand behind it (a destructor declared
`noexcept(false)` against one declared without, neither used). That is
safe: whatever reads the specification (a call, `noexcept`, the
function's type or address, a definition) resolves it on both sides, so
where the translation unit observes the difference both sides have the
unit and it differs (`tests/differ/_/resolved-spec-observed-both-sides`,
`resolved-spec-addr-deferred`, `resolved-spec-addr-deferred-dtor-member`,
`resolved-spec-addr-deferred-differs`; `spec/verify.md` §4).

A specification is not always resolved before a type that holds it is
formed: naming a function leaves a late `noexcept` that is not parsed
yet, and inside a template one that cannot be resolved yet, as
`Deferred`. So a type digest can meet `Deferred(d)` for a function that
is resolved by the end of the translation unit. It writes what it meets;
Kiln keeps the two types distinct, and so does the digest.

### 11.3 Evaluated values of variables

`VarData` `eval` holds what the Oven found when something needed the
variable's value. The resolved unit `value` exists if the state is
`NotConstant` or `Value`, and its stream is `value?`: absent for
`NotConstant`. A variable in state `NotEvaluated` has no such unit. The
state `Evaluating` cannot be met after the translation unit ends; a walk
that meets it makes the variable's unit indeterminate.

The state read is that of the declaration that holds the initialiser
(as `VarData` `init` is read, §2.3), else that of the declaration read.
The variable's own unit never says whether it was evaluated. A variable
whose initialiser is in the resolved unit `definition` (§11.9) has its
`value` under the variable as any other; nothing is under `definition`.

After the variable's own value the stream holds one `value?` for each
temporary whose lifetime the variable extends (each `LetData` whose
`extending` names a declaration of it), in `mangling_index` order:
`LetData` `value`. Kiln writes that value once, in any strict evaluation
whose result points at a temporary the variable being initialised
extends; a lax evaluation leaves it unset, and so do the two other
writers Kiln has (the code generator's fold prepass, which a Slip parse
does not run, and the module importer). So a variable in state `Value`
can have a temporary without a value: the `value?` is then absent, and a
transformation that changes which evaluation reaches the initialiser
first is a reported difference in this unit, never an unreported one.

At the pin Kiln writes `eval` when it checks a declaration's initialiser
(a `constexpr` variable, a `const` variable of integral or enumeration
type initialised where it is declared, a static data member initialised
out of line). For a declaration made by instantiation (a static data
member of a class template's instantiation, initialised in class or out
of line, and a variable template's specialisation) that check runs when
Kiln instantiates the initialiser, which a use causes: there a use makes
the unit appear, as for any resolved unit. The state is read over every
declaration of the variable, the out-of-line definition an instantiation
made for a static data member included (`Walk::redecls` in `definition`
mode), since Kiln records it on the declaration that holds the
initialiser
(`tests/resolved/value/instantiated-static-member`, `variable-template`,
`instantiated-out-of-line`). For any other variable the check runs with
the declaration and a later use does not make the unit appear. An
in-class `static const int x = 1;` is evaluated when it is checked, as
the out-of-line form is, so it has a `value` unit, used or not (Kiln
WP-2504, `kiln_sema/src/class/members.rs:560-570`, in the pin since
WP-066; at `913a490bb` it was never evaluated and had none:
`tests/differ/_/in-class-static-const-value-through-another-unit`,
`tests/resolved/value/in-class-static`).

A block-scope variable (not a block-scope `extern`) has no unit, and its
uses read its value without writing it (`DeclRefExpr` `non_odr_use_constant`).
Kiln evaluates it with its declaration, so its stream holds the value in
place, after `spec`: `value?`, present in state `Value`, written in the
function's mode (digest §9: `constexpr auto n = len(__func__)` is the
number, `constexpr const char *p = __func__` passes the name on). Where
name-derived values are written by value (§4.2 rule 8: a `static_assert`
condition, a key, a type), a `DeclRefExpr` that names such a variable is
followed by `value?` of its value, by value; elsewhere nothing follows it.

### 11.4 Default arguments and default member initialisers

A default argument of a function that is not an instantiation, and a
default member initialiser of a class that is not one, is parsed with its
declaration and is written in the function's or field's unit
(`ParamData` `default_arg`, `FieldData` `init`).

In a declaration made by instantiation Kiln instantiates each only when
a call or an initialisation uses it (`ParamData`
`flags-default-arg-pending`). There the rows `default_arg` and `init`
write an absent child whatever Kiln holds, and an instantiated one is a
derived unit of kind `default_argument` whose parent is the function or
field and whose name is `parameter` with the parameter's index (0 for a
field). That the parameter has a default argument at all is a fact of the
pattern, which the instantiation names (`SpecInfo` `from`,
`MemberSpecInfo` `instantiated_from`).

A parameter's default argument is instantiated when the parameter of an
instantiated declaration of the function holds an expression
(`ParamData` `default_arg`); `flags-default-arg-pending` stays set once
it is, so it is not the mark. The unit is that of the first such
declaration of the chain, first declaration first. It is written
whenever its function's or field's unit is (it does not wait for a
`Ref`), and its stream is the expression, walked as an initialiser is,
with a unit state of its own: parameter j of the function (that of every
declaration of the chain) is the local entity of ordinal j, as the
function's own stream numbers its parameters, and the locals the
expression declares are numbered from the parameter count on. So a
default argument that names an earlier parameter (`sizeof(p)`) holds the
same bytes whichever use instantiated it.

A local function that is no unit (a local class's member, the call
operator of a lambda that is not generic, written in place in its
owner's stream, `digest.md` §4.3) in an instantiated body has its
default arguments instantiated with that body, in local scope, not on
use. Each is a `default_argument` unit all the same (WP-158), keyed
`parameter(i)` under the function's key, derived, written once after
every other unit; its function's own stream writes the default absent
as above.

An inheriting constructor has no `default_argument` unit: a call to it
uses the base constructor's parameters and their default arguments, which
are the base constructor's units.

For a field the same holds under every form of instantiation: a field's
default member initialiser is absent for every field instantiated from a
pattern, whether its class is an implicit instantiation, an explicit
instantiation declaration or an explicit instantiation definition, or a
member class of one. Kiln instantiates the initialiser at its first use
in all three, so an explicit instantiation does not make it a fact of
the class. A field written in source (in a primary template, a partial
or explicit specialisation, a class that is not a template, or a local
class of any function) is written with its initialiser.

`ColdExprData` `default_imm` holds the value of an immediate invocation
inside a default argument or default member initialiser, computed at the
first use that has no source-location values. The resolved unit
`default_immediates` of the unit that holds the default argument (the
function or field for one written in source, the `default_argument` unit
for an instantiated one) lists the values of the argument's immediate
invocations that have an entry, in the order the expression walk meets
them (a node it writes as `backref`, §1.7, it does not meet again: a
shared node's invocations are listed once, at its first writing), as
`count` and then `value` for each; it exists when at least one
has an entry. Its discriminator is the parameter's index, absent for a
field. A walk that meets a `default_imm` entry inside a type (a
`decltype` operand) does not list it: type digests are written with an
expression state of their own.

### 11.5 Facts of classes and virtual functions

`key_function`, `dtor_delete`, `dtor_delete_this`, `inheriting_ctor` and
`inherited_ctor` of `ColdDeclData` are written in the resolved units of
§11.1 and nowhere else.

`overrides` is not demand-driven. Kiln fills it when the member function
is declared, and for an implicitly declared member when it declares it,
so it is there whenever the function's unit is: it is a row of that unit
(`ColdDeclData` `overrides`), and the property number it once had is
retired.

Kiln keeps "vtable used" marks in its semantic analysis and not in `Ast`;
there is nothing to write for them.

### 11.6 Builtins

A builtin function Kiln declares on first use is a derived unit: its key
is its name under `Parent::builtin`, and it exists if a stream holds a
`Ref` to it. `ColdDeclData` `builtin_id` and the operand of its `BUILTIN`
attribute are Kiln's numbering of the builtin and are not written; the
name says the same. `lib_builtin` marks a function declared in source
that Kiln recognises as a library builtin by its name and signature,
both of which its unit holds; it is not written.

### 11.7 Tables computed from facts that units hold

Not written: record layouts (`LayoutCache`, `RecordLayout`), the
subobject graphs and final overriders of polymorphic classes
(`ClassVirtuals`), vtable layouts (`VTableLayout`), the validated
comparison category classes (`CmpCategories`), and Kiln's caches of
concept satisfaction and normal forms, which are not in `Ast`. Each is a
function of declarations whose units are compared (digest §6, last
rule); the result of a satisfaction check that an expression stores is
written with the expression (§7).

### 11.8 Marks, caches and orders

Not written, because each records only that or when something was asked:
`DeclFlags` `0-used` and `0-referenced`; `DeclMisc` `0-linkage-cache`
and `0-visibility-cache`; `DefinitionBits`
`0-complete-definition-required`; `DeclArena` `completed_records` and
`fn_def_insts`; `TemplateTables` `end_of_tu_defs`, `ref_var_insts`,
`specializations`, `spec_order` and `member_map`. Whether an implicit
member or a builtin is a unit is decided from the streams the walk
writes, never from the marks (§5.2).

### 11.9 Definitions made on use

Kiln declares an implicit instantiation when something names it and
defines it only when something uses it; it declares an implicit special
member or comparison on lookup and defines it on odr-use; under an
explicit instantiation it defines a member whose pattern is not yet
defined at that point only on use. So that a unit's digest does not
depend on which of these happened (digest §1 rule 5, §2.2), a unit of
the **definition class** is written in two parts. The class is:

1. a specialisation whose `SpecInfo` kind is `ImplicitInstantiation` or
   `ExplicitInstantiationDeclaration`;
2. a member whose effective kind (§11.10) is an instantiation
   (`ImplicitInstantiation`, `ExplicitInstantiationDeclaration` or
   `ExplicitInstantiationDefinition`), whatever its kind of entity:
   under an explicit instantiation definition Kiln defines only the
   members whose patterns are defined at that point ([temp.explicit]/10)
   and the others on use, and a member class used before it stays as it
   was;
3. a function or deduction guide every declaration of which is implicit
   (a special member function, a comparison operator, a deduction guide,
   the conversion functions of a closure type), and a function
   explicitly defaulted on its first written declaration;
4. an entity all of whose declarations are instantiated outside their
   class (digest §2.2): a friend declared only by instantiations, whose
   definition is instantiated on use.

A specialisation whose `SpecInfo` kind is `ExplicitInstantiationDefinition`
is not of the class: its definition is made at the explicit
instantiation, used before it or not. An entity with a declaration
written in source is classified from the earliest such declaration
(`Walk::representative`), not from one an instantiation made before it.
The predicate (`demand::definition_class`) reads only facts that do not
change when the definition is made.

A unit of the class is read from that earliest written declaration, not
from the definition (§2.3 reads the definition otherwise), and its own
stream reads none of the declarations an instantiation makes out of line
for a member (`units::instantiated_out_of_line`: the definition Kiln
makes at namespace scope when a static data member of an instantiation
is used, `kiln_sema/src/templ/class.rs:1943`). Exactly, a declaration
`d` is instantiated out of line when all of these hold: its semantic
context is a class; it has a lexical context, which is not its semantic
context and is not a class (a friend's lexical context is the befriending
class: a friend is never out of line); and it was made by instantiation
(`templ::instantiated_declaration`: its `SpecInfo` kind, else its own or
its entity's `MemberSpecInfo` kind unless that is
`ExplicitSpecialization`, else the kind governing its lexical context,
is one of the three instantiation kinds). For such a unit the rows
below write their empty form in the unit's own stream, without a cause;
`DeclMisc` `0-definition` is written clear and is in no stream: that
`definition` exists says it. The rows are written, in the order given,
in the resolved unit `definition` (§11.1):

| Entity | Rows of `definition`, in order |
|---|---|
| every entity with a `SpecInfo` | `SpecInfo` `from`, `from_args` |
| a function | its parameters (`FunctionData` `params`), as the own unit writes them, so that the body's local entities are numbered in this stream; `FunctionData` `body`; `ColdDeclData` `ctor_inits`; one flag word: `FnSpec` `0-implicit-inline` (clear if any declaration is `inline`), `0-has-contracts`, `0-is-coroutine`, `0-escalated`; `ColdDeclData` `contracts` of the defining declaration (a count, then each as `child`); the attributes of kinds `WEAK` and `PRAGMA_CLANG_SECTION` on the declarations made by instantiation: per declaration, first to last in chain order, a list of its nodes of those kinds in stored order, not inherited; the non-empty lists, preceded by their count, each a count then its nodes as the own unit writes them (§10.1); and, when the pattern's declared return type holds a placeholder, `DeclNode` `ty-type` and one flag word: `FnSpec` `0-return-deduced` |
| a variable | `VarData` `init` and `VarSpec` `0-init-style` when the pattern has an initialiser, else absent and `C` (see below); one flag word: `0-inline`, `0-inline-implied`; and, when the pattern's declared type holds a placeholder or the own unit's type (of the earliest written declaration) is an array of unknown bound, `DeclNode` `ty-type` of the defining declaration |
| a class | `RecordData` `members-fields`, `members-virtual`, `members-other`, `bases`, `defn`; `ColdDeclData` `friends`, `record_pack`; every attribute on the declarations made by instantiation, as for a function, then their annotations (`AnnotArena` `of_decl`) |
| an enumeration | `EnumData` `members`, `int_type`, `promotion_type`; one flag word: `flags-fixed`, `flags-complete` |

Each row is written as the own unit would write it for an entity not of
the class, read from the defining declaration with the same merging over
the declarations: the flag bits are set if any declaration sets them,
`init` and `0-init-style` are those of the declaration that holds the
initialiser. Here the declarations an instantiation made out of line are
read too. The empty forms in the own stream are: `ty-type` absent, an
empty list for each list, `child?` and `uv?` absent, `0-init-style` `C`,
each moved flag bit clear, `SpecInfo` `from` `Ref::none` and `from_args`
absent.

**When `definition` exists** (`demand::defining_declaration`): when Kiln
holds the definition at the end of the translation unit.

- A function: `Ast::definition` (`kiln_ast/src/ast.rs:678`, recorded by
  `set_body`, `kiln_ast/src/builder.rs:907`), if that declaration has a
  body. A function deleted on its first declaration is not defined by
  that: `= delete` is a fact of the declaration.
- A variable: none while the pattern (as for the placeholder below) has
  a declaration with an initialiser and no declaration of the variable
  holds one: Kiln declares `v<int>` when it is named
  (`decltype(v<int>)`) and instantiates its initialiser only when it is
  used (`instantiate_var_spec_init`, `kiln_sema/src/templ/var.rs:229`;
  `instantiate_static_member_init`, `kiln_sema/src/templ/class.rs:2061`),
  and only then is the declaration a definition. The pattern of a
  variable specialisation is the declaration `SpecInfo` `from` names
  (the primary's or a partial specialisation's declaration with the
  initialiser, else its definition: Kiln sets it when it declares the
  specialisation, `kiln_sema/src/templ/var.rs:109-161`), of a member
  the declaration `MemberSpecInfo` `instantiated_from` names; the
  pattern "has an initialiser" if one of its declarations holds one
  (`demand::pattern_has_init`). Otherwise the first
  declaration, over every declaration Kiln chained, that is a definition (`units::is_var_definition`, Kiln's
  `Sema::is_var_definition`), else the one that holds an initialiser (an
  in-class `static const int x = 1;` of an instantiation, whose
  initialiser Kiln substitutes on use, `subst_static_member_init`).
  When the pattern has no initialiser, the variable is default-initialised
  and Kiln adds that initialisation (a constructor call for a class
  type) only when it instantiates the definition, on use
  (`kiln_sema/src/templ/var.rs:212-221`, `instantiate_var_spec_init`).
  `definition` does not write it: its `init` is absent and its
  `0-init-style` `C` whatever Kiln holds. Nothing is lost: which
  constructor default-initialisation calls is a function of the
  variable's type, which the own unit holds, and of the units of that
  type's class; two programs whose default-initialisations differ
  differ in one of those units.
- A class or an enumeration: `Ast::definition`, recorded when the
  definition begins (`begin_record_definition`,
  `begin_enum_definition`, `kiln_ast/src/builder.rs:585`, `:665`).
- Any other entity of the class (a field, a typedef, an enumerator, a
  member template) has none: nothing of it is made on use.
- An entity with a written declaration whose definition an instantiation
  added outside its class (WP-032 rule 4: a friend defined in a class
  template and declared in its namespace too) is not of the class, but
  that definition is not its own unit's (§2.2): it is its `definition`,
  read from the declaration `Ast::definition` names, if it has a body.

**The placeholder.** Whether the pattern's declared type holds a
placeholder (`auto`, `decltype(auto)`, a constrained placeholder, a
class template to deduce) is a fact of the pattern: the declaration
`MemberSpecInfo` `instantiated_from` names, for a variable
specialisation the declaration `SpecInfo` `from` names (above), for
another specialisation the pattern of the template `SpecInfo`
`template` names (a function's `from` is moved at definition), on any
of its declarations, walked
through pointers, references, member pointers, a function's return type
and arrays' elements, sugar included (`demand::pattern_holds_placeholder`,
as Kiln's `Sema::contains_placeholder`,
`kiln_sema/src/templ/auto.rs:588-619`). It is read from the pattern and
never from the instantiation: Kiln re-points the instantiation's
`DeclNode` `ty` when it deduces. `FnSpec` `0-return-deduced` is moved
with the type because nothing says it is set before the deduction.

The three `VarSpec` rows are the ones instantiating a static data
member's initialiser sets (Kiln's `VarSpec::LATE`). The other late bits
are not demand-driven: `ParenList`, and `Call` for a default-initialised
class variable, are set when the initialiser is checked, with the
declaration or with the instantiation of the initialiser; `0-nrvo` is
set when the variable's scope ends, inside the body that holds it. The
Oven sets no `VarSpec` bit.

**The principle.** For the definition class, the own unit holds only
what instantiating the *declaration* determines. Everything Kiln writes
onto a declaration when it instantiates the *definition* is read only by
`definition`, or by no unit when it is no fact of the program. A row
whose value Kiln sets both when it instantiates the declaration and when
it instantiates the definition, without recording which, is read only by
`definition` for the class, and the own unit writes it clear: that is
`FnSpec` `0-implicit-inline`, set by `function_decl`
(`templ/subst/decls.rs:1044`) and by `enter_definition_scopes`. What Kiln copies onto a declaration
at definition, without marking the copy, is told apart by the
declaration that carries it: a node on a declaration **made by
instantiation** is `definition`'s for the kinds Kiln copies there (for
a function `WEAK` and `PRAGMA_CLANG_SECTION`; for a class every
attribute and annotation, which `Substituter::class_attrs` copies from
the pattern's definition), and a node on a declaration written in
source stays in the own unit (`attrs::moved_kinds`). A declaration is
made by instantiation (`templ::made_by_instantiation`) when its own
`SpecInfo` or `MemberSpecInfo` (Kiln links only the declarations it
makes for a specialisation or a member of one, `Ast::templ_link`) has
an instantiation kind, or, having none or `ExplicitSpecialization`, its
lexical context is governed by an instantiation (`templ::governing_kind`
of the lexical context: a member or friend Kiln declared in an
instantiated class). A written friend that redeclares a member of an
instantiation (`struct A { friend void X<int>::f(); };`), and a
function defaulted on its first declaration, are written in source. A
node on a declaration made by instantiation is Kiln's: instantiated
from a declaration of the pattern, which the pattern's own unit holds,
with operands substituted from the arguments, which are in the key, or
copied from the pattern's definition, which the pattern's unit holds
too; nothing written in source puts a node on it (an attribute written
on an explicit instantiation is recorded nowhere by Kiln). So leaving
it out of the own unit loses nothing, and `definition` writes it when
the definition exists. The writes, at
Kiln's pin (`kiln_sema/src/` unless a crate is named):

| Write at definition instantiation | Where | Read by |
|---|---|---|
| `SpecInfo` `from` re-pointed to the friend's template definition | `templ/class.rs:1578` | `definition` (`SpecInfo` row) |
| location and `declarator_end` moved to the pattern's definition | `templ/class.rs:1597`, `:1700`; `kiln_ast/src/templates.rs:1989` | no unit: locations are not hashed, and a unit's file never reads it (§11.1) |
| `WEAK` copied from the out-of-line definition | `templ/class.rs:1603-1615` | `definition` (attributes); left out of the own unit's list |
| `PRAGMA_CLANG_SECTION` kinds copied when absent | `section.rs:204`, called at `templ/class.rs:2313`, `templ/funcs.rs:365` | `definition` (attributes) |
| `MemberSpecInfo` / `SpecInfo` `point_of_instantiation` | `templ/class.rs:1634-1635`, `templ/funcs.rs:359-360` | no unit (a location) |
| parameters renamed from the pattern's definition | `templ/pack.rs:629`, `templ/funcs.rs:394` | `definition` (parameters, by ordinal: names of locals are not hashed) |
| `FnSpec` `0-implicit-inline` | `class/members.rs:906`, `:921` (`enter_definition_scopes`) | `definition` |
| `DeclFlags` `0-invalid` when the definition fails | `templ/class.rs:2377`, `templ/funcs.rs:452` | indeterminate wherever read (§12): the translation unit did not compile |
| `ColdDeclData` `ctor_inits` | `Substituter::function_def`, `templ/class.rs:2310` | `definition` |
| `FunctionData` `body`, `Ast::definition` | `act_on_finish_function_def`, `decl.rs:9806`; `kiln_ast/src/builder.rs:907` | `definition` |
| `ColdDeclData` `contracts`, `FnSpec` `0-has-contracts` | `contracts.rs:681-687` | `definition` |
| `FnSpec` `0-is-coroutine`, `0-escalated`, `0-return-deduced`, deduced `ty` | `coroutine.rs:202`, `consteval.rs:1617`, `deduce.rs:789` | `definition` |
| `ColdDeclData` `dtor_delete`, vtable marks | `act_on_finish_function_def` (`check_destructor_delete`) | resolved `dtor_delete` (§11.1); marks are in no unit (§11.8) |
| `VarData` `init`, `VarSpec` `0-init-style` | `templ/subst/decls.rs:401` (`var_init`), `templ/class.rs:2061` | `definition` |
| `VarSpec` `0-inline`, `0-inline-implied` | `templ/class.rs:2061` | `definition` |
| a variable's type deduced or its array bound completed | `var_init` (`act_on_init`) | `definition` (`ty-type`, by the rule of the table) |
| a variable's `eval` state | `var_init`, and every constant evaluation | no own unit of the class: see below |
| a static data member's namespace-scope definition | `templ/class.rs:1943` | `definition` only (left out by `instantiated_out_of_line`) |
| a class's members, bases, `defn`, friends, record pack; `point_of_instantiation` | `instantiate_class_as`, `templ/class.rs:1198`, `:1294-1295`; `DeclFlags` `0-invalid` at `:1229` | `definition`; the location in no unit |
| a class's attributes and annotations, from the pattern's definition | `Substituter::class_attrs`, `templ/subst/anon.rs:140`, from `class_final_and_attrs`, `templ/class.rs:2530` | `definition` (attributes of declarations made by instantiation); left out of the own unit |
| a variable's default-initialisation when the pattern has no initialiser | `templ/var.rs:212-221` | not written (above) |

The `FnSpec::LATE` bits (`kiln_ast/src/decl.rs:818`) that are not in
this table are not definition writes: `0-not-selected` is set when the
class is completed (`class/prospective.rs:75`, `:143`), `0-deleted`,
`0-defaulted`, `0-trivial` and `0-constexpr-kind` of an implicit or
defaulted member when it is declared or its class is completed
(`class/implicit.rs:877`, `:1584`, `class/special/mod.rs:408-467`,
`class/deletion.rs:969`, `:984`, `class/inhctor.rs:76`, `:83`,
`compare/default.rs:104`, `:417`, `:471`), and `0-implicit-inline` at
`templ/subst/decls.rs:1044` is set when the declaration is instantiated.

**Evaluated values.** `VarData` `eval` writes no row in any unit: its
states only make a unit pending (`Value`, `NotConstant`, or
`NotEvaluated` with an initialiser: the value is the resolved unit
`value`'s, §11.3) or indeterminate (`Evaluating`). The own unit of a
variable of the class reads no `eval` state at all, and takes neither
mark from it. This is sound: the value is a function of the initialiser,
which is `definition`'s, and of the entities the initialiser names,
which have units of their own, so a difference of the value is a
difference of one of those units; and the value as a fact is the
resolved unit `value`, a part of its own, never a row of the own unit.
Reading `eval` in the own unit would make it depend on whether some use
had evaluated the initialiser (a constant expression, a `static_assert`,
a template argument).

The mark "emitted unreferenced" (flag bit 1, `digest.md` §11; WP-155,
`spec/landed/verify/WP-155.md` §1) is set on the `definition` unit of a
variable or function the translation unit emits whether or not anything
refers to it, and it does read `eval`: a variable whose state is not
`Value` is marked. It is a flag, part of no digest, so the paragraph
above does not apply to it; what it does inherit is that whether a use
evaluated the initialiser can decide it. That only adds marks: a
variable whose initialiser is a constant is `Value` once evaluated and
emits nothing either way, and one that is not evaluated is marked. A
mark too many is a finding too many (`verify.md` §4 step 3), never a
missing one.

A class template specialisation whose instantiation failed has a unit
and no `definition`, like a function declared and not defined. One
that was only named has `SpecInfo` kind `Undeclared` and no unit at all
(§11.10, `units::unit_class`): its unit and its `definition` appear
together when Kiln instantiates it.

The split costs one more unit for each defined instantiation. Without it
a use added to one function, which is the first to need the body of some
`std::vector<int>::size`, would be reported as a change of that member
as well. Decisions and Kiln's evidence:
`spec/landed/digest-kinds/WP-045.md`.

### 11.10 The effective kind

Kiln gives a member of an instantiated class a `MemberSpecInfo` whose
kind depends on what was instantiated first: a member class already
defined stays `ImplicitInstantiation` under a later explicit
instantiation of its class (`kiln_sema/src/templ/spec.rs:1366-1398`),
a member function keeps `ImplicitInstantiation` under one, and a field
has none. The digest reads one kind for every entity, its **effective
kind** (`demand::effective_kind`), computed from facts that do not move:

- an entity with its own `SpecInfo` (a specialisation of a template)
  has that kind, and nothing below applies to it;
- a member whose `MemberSpecInfo` kind is `ExplicitSpecialization` has
  that kind;
- any other member of a class or enumeration that has the effective kind
  of an instantiation has the greater of its own `MemberSpecInfo` kind
  and the effective kind of its class, in the order
  `ImplicitInstantiation` < `ExplicitInstantiationDeclaration` <
  `ExplicitInstantiationDefinition` (`Undeclared` and no kind count as
  least);
- a member of an explicit specialisation, or of a class that is not
  made by instantiation, has none (it is written in source);
- a declaration that is no member (a friend an instantiation declared)
  has none, whatever its `MemberSpecInfo` says: it may be written in
  source too, and whether it exists only through instantiations is the
  outside-only rule's (§2.2);
- a local entity has none: it is written in full in the stream of what
  holds it.

`MemberSpecInfo` and `SpecInfo` are read from the entity's first
declaration, which Kiln keys them by. What reads the effective kind:
the unit class (§5.2, digest §2.2: an entity with an effective kind is a
declared unit when the kind is an explicit instantiation or an explicit
specialisation, derived when it is `ImplicitInstantiation`), the kind
`MemberSpecInfo` writes (§9.4), the definition class (§11.9), and
whether a field was instantiated from a pattern (§11.4). Whether a
context is an implicit instantiation, for the declarations instantiated
outside their class (digest §2.2), is the effective kind of the nearest
context that has one (`units::in_implicit_instantiation`).

## 12. Unsupported

A unit whose walk meets one of these is indeterminate (digest §10). An
indeterminate derived unit is a finding on whichever side it appears
(`verify.md` §4), so a construct listed here that an instantiation
reaches is reported even when only one parse instantiates it.

| What | Rows | Why |
|---|---|---|
| Error recovery | `TypeView` `Invalid`; `BuiltinType` `Error`; `TypeFlags` `0-contains-error`; `TypeNode` `flags-contains-error`; `DeclFlags` `0-invalid`; `ExprKind` `RecoveryExpr`; `ExprDeps` `0-error` | the translation unit did not compile; what Kiln built for the erroneous part is not a program (digest §10) |
| Reflection and splices | `TypeView` `Splice`; `Nns` `Splice`; `TemplateName` `Splice`, `SpliceProfile`; `ExprKind` `CXXReflectExpr`, `CXXSpliceExpr`; `ExprNode` `MemberExpr.bits-spliced`; `ColdExprData` `splice_args`; `ValueKind` `Reflection`; `Reflectee`; `DmsDesc` | a reflection names an entity by an interning index, and the metafunctions read names, positions and declaration order, which the digest leaves out on purpose: what a reflection evaluates to is not bounded by the digests of the units it names. It needs a design of its own |
| Variable length arrays | `TypeView` `VariableArray` | the bound is an expression evaluated at run time inside one function, and a type digest has no unit to number its locals in |
| Functions without a prototype | `FuncInfo` `0-no-proto` | C only |
| Defaults too large to follow | none: the cause `default/reach-limit` | a use of a default whose own expression has more than 65,536 nodes, or past 65,536 nested defaults: what it passes on is not followed (digest §9 "Defaults at a use"), so the unit holding the use is indeterminate rather than silent |
| File-scope compound literals | `ExprNode` `CompoundLiteralExpr.aux`, `CompoundLiteralExpr.b`; `ColdDeclData` `compound_literal` | C only: the literal's object is a variable Kiln declares for it, whose key this file does not define |
| Not built by Kiln | `DeclKind` `UsingPack`, `FriendTemplate`, `ImplicitParam`, `ImplicitConceptSpecialization`; `ExprKind` `UserDefinedLiteral`, `EmbedExpr`, `UnresolvedMemberExpr`; `TemplateName` `DependentOperator`; fourteen rows of `AttrKind` | no construction site at the pinned revision, which Kiln confirms for all but the attribute kinds (KQ1 C17); the walk treats one as indeterminate if it ever appears. Kiln's WP-2026 removed `StmtNode` `flags` and `TemplLinkKind` `DependentFriend`, whose row is retired (§1.4) |

## 13. Differences between the inventory and the source

Where `research/kiln-ast-inventory.md` and the Kiln source at the pinned
revision disagree, the tables follow the source.

- `CastKind` has 47 variants, not 49.
- `DeclFlags::INJECTED` marks a member injected by `define_aggregate`, not
  an injected-class-name.
- A `Friend` naming a type stores the owned tag's `DeclId` in `data`, or 0
  with the type in `ty`.
- `FuncProto` stores the raw words of `FuncInfo` and of the exception
  specification, not decoded values; the accessors are `TypeContext::proto`
  and `exc_spec`. There is no `Ast::exception_spec_of`.
- `ColdDeclData` `overrides` is documented as written once at the end of
  the class, while digest §2.3 lists it as demand-driven (§11.5).
- Kiln has no declaration kind for an explicit instantiation: it is the
  `SpecKind` of the specialisation it names.
- `ExprKind::UnresolvedMemberExpr` is never built: an overloaded member
  access is a `MemberExpr`, whose names are `ColdExprData`
  `member_overload`.
- `CharKind`, `NonOdrUse` and the "part of an explicit cast" bit are
  listed as bookkeeping; nothing shows that code generation ignores them
  and they are `in`. `CXXThisExpr`'s implicit bit is listed as spelling;
  the Oven and substitution read it.
- `ExtVectorElementExpr` has the `ARROW` bit; `GenericSelectionExpr`
  uses the `ARG_IS_TYPE` bit; `CXXUnresolvedConstructExpr` `aux` is the
  list-initialisation form; `CXXDefaultInitExpr` `aux` is always 1.
- "vtable used" marks are in `kiln_sema::Sema`, not in `Ast`.
- The inventory describes attribute operands kind by kind from
  `kiln_ast::attr`. The source has a complete table of them
  (`kiln_sema::bmi_image::attrs::TABLE`), kept complete by a Kiln test,
  which §10.1 follows.

## Tests

- `crates/slip_digest/tests/tables.rs` — digest §13 obligation 4. It
  parses this file by the grammar of §1 and compares it with
  `kiln_snap::Schema::descriptors()` of `kiln_ast::ast::Ast` and of the
  types whose `pin` is `extra`: every field and variant has a row, every
  row's member exists, every `kind` prefix is a variant of the selector,
  every `enum T` and `record T` names a tabled type, tags are unique
  within a table and follow §1.4, and every `pin` value is right.
- The same test, obligation 5: for every row, the directory §1.5 derives
  from it exists under `tests/differ/`, `tests/equal/`, `tests/demand/` or
  `tests/indeterminate/`, and every directory there is named by a row.
- `crates/slip_digest/tests/differ.rs`, `equal.rs`, `demand.rs`,
  `indeterminate.rs` — run the pairs in those directories as §1.5 says.
- Digest §13 obligation 3 is not a test: `slip_digest` asserts at compile
  time the `SCHEMA_HASH` of every tabled type whose `pin` is `schema` or
  `extra`.
- `crates/slip_digest/tests/resolved.rs` — §11: for each property of
  §11.1, a source and the same source with a use that makes Kiln compute
  the fact; the resolved unit appears and no other unit changes.

## Open questions

### Words the pinned schemas cannot see

Digest §13 obligation 6. For each word below the tables give rows per
kind or per fact that no descriptor checks; until Kiln publishes a table
of operand roles (`docs/design/05-kiln-interface.md` KR5), a change of
meaning inside Kiln is caught only by the tests of §1.5.

| Type | Words |
|---|---|
| `Quals`, `TypeFlags`, `FuncInfo`, `DeclFlags`, `DeclMisc`, `FnSpec`, `VarSpec`, `DefinitionBits`, `ExprDeps`, `TemplLink`, `DeclName`, `VarEval` | `0` |
| `TypeNode` | `flags`, and the `aux` and `data` words behind `TypeView` |
| `DeclNode` | `data`, `ty` |
| `ParamData`, `FieldData`, `EnumData`, `NamespaceData` | `flags` |
| `RecordData` | `members` |
| `CtorInit` | `target` |
| `ScopeData` | `aux` |
| `StmtNode` | `a`, `b`, `aux` |
| `ExprNode` | `a`, `b`, `aux`, `bits` |
| `ColdExprData` | `cast_written`, `member_extra`, `offsetof_pattern` |
| `AttrNode` | `a`, `b`, `flags`; and every value of `AttrKind` |
| `TemplateTables` | `pseudo_dtor_first` |
| `TemplateArg`, `KeyedEntry`, `consts::Node` | the words behind `ArgView`, the canonical keys and `ConstValue` |

Some `in` rows write a word in Kiln's own numbering because Kiln has no
enumeration for it: `FuncInfo` `0-sme`, `AddrSpace` `0`, `FpOverride`
`mask`, `ExprNode` `AtomicExpr.aux`, and `AttrNode` `MODE.a`,
`PRAGMA_CLANG_SECTION.b` and `ZERO_CALL_USED_REGS.a`. A renumbering in
Kiln changes digests without changing the program. It is safe, since both
sides of a comparison use one Kiln, but it breaks the rule that tags are
Slip's.

### Types nothing pins

`TypeView`, `ArgView`, `ParamAbi`, `AddrSpace`, `ConstValue`, `Scalar` and
`ByteImage` are decoded forms without a schema descriptor (`pin=none`),
and `DeclNameView`, through which `DeclName` is read, is another;
`Linkage` is the value of a query (`Ast::linkage`), which no descriptor
describes either. A
variant added to one of them is caught by obligation 2 (the walk does not
compile) and by nothing else. `AttrKind` is a newtype: a new constant is
caught only when a walk meets it.

The `pin` values are checked by `tables.rs` against Kiln's descriptors.
One pinned type, `kiln_ast::oper_lookup::SetSpan`, is private to Kiln,
so its hash is compared when the test runs and not when `slip_digest`
compiles (`crates/slip_digest/tests/tables/pins.txt`).

### Questions for Kiln

They are in [`docs/requests/KQ1-digest-fields.md`](../docs/requests/KQ1-digest-fields.md),
sent to the Kiln agent on 2026-11-22 and answered the same day; the
answers are recorded there and folded into the rows, §11 and the hazards
below. What they left open:

- **Late `noexcept`** (§11.2). A member function or friend whose
  `noexcept` Kiln parses late is outside the deferrable class, because
  nothing stored marks it once resolved. If the corpus shows such
  functions left `Deferred` in numbers, the fix is a stored mark from
  Kiln, or widening the class to every member function and every friend
  defined in a class, at the cost of a resolved unit for each.
- **Evaluation of in-class static constants** (§11.3): closed. Kiln
  WP-2504 (in the pin since WP-066) records `eval` for the in-class
  `static const int x = 1;` as for `const int S::x = 1;`, so the two
  ways of writing one constant have one `value` unit
  (`tests/equal/_/var-init-in-class-or-out-of-line`).

### Defaults inside a template template parameter's head

A default template argument inside the head of a template template
parameter (`template <template <class = int> class P> struct X;`) is
written from the declaration the walk reads. When another declaration
of the template gives it, the unit does not hold it (WP-029): the two
ways of writing the same template then differ, which errs on the side
of a reported difference.

### Hazards accepted for now

- **Member classes used before an explicit instantiation** (§11.10). A
  member class defined, by a use, before the explicit instantiation of
  its enclosing class stays implicitly instantiated in Kiln, as in clang
  (its vtable is not required, `kiln_sema/src/templ/spec.rs:1366-1398`);
  the digest writes the kind the explicit instantiation gives, so its
  unit is declared and the same either way. What a use that moves across
  the explicit instantiation therefore does not show: that the member
  class's vtable and its virtual functions are no longer required at the
  explicit instantiation (code generation, not the digest, sees it), and
  that Kiln does not instantiate its members again there.
- **Literals from builtin macros** (§7) are written by value: moving a
  line that uses `__LINE__` as a value is a reported difference.
- **Overloaded member access** (§7): of the candidate set only the
  first candidate is written; the rest is the lookup of the member's
  name in the naming class, which `member_overload` writes with the
  qualifier and the template arguments. A change to the set is a change
  to a member of that class, which its unit sees, and the access's own
  unit sees it only when the first candidate changes. The instantiations
  that call through the access name their function.
- **Alias templates with dependent arguments** (§4.2). An alias template
  specialisation is digested as the type it names, so a condition that
  only its arguments carry (`void_t<decltype(…)>`) is in no digest of
  the pattern, and two partial specialisations that differ only there
  have one key: a collision, which makes both indeterminate and counts in
  `digest.collisions`. Kiln confirms it keeps nothing canonical that
  tells them apart (they are one declaration to Kiln too, as to clang;
  only `PartialSpecInfo` `args` as written differs). The corpus run says
  how often; the fix would be to stop treating such an alias as
  transparent.
- **Nullability on a dependent type** (§4.3). A nullability attribute is
  sugar and is not written; on a dependent type Kiln checks it at
  instantiation, where it can be a substitution failure. The
  instantiations show it.
- **Text made from names** (digest §9). `SpecInfo` `print_args`,
  `TemplateTables` `written_args`, `args_as_written` and
  `preferred_named`, and `ColdDeclData` `conversion_spec_target` reach
  `__PRETTY_FUNCTION__`, `display_string_of`, sanitiser type descriptors
  and the names of types in generated code. They are left out as every
  value that comes from a name is.
- **Attribute order** (§10.1) is kept: reordering the attributes of one
  declaration is a reported difference.
- **Parameter attributes of block-scope declarations** (§10.1, §2.4).
  The entity's unit leaves out the declarations at block scope and the
  block writes its own parameters' attributes, but Kiln's code generation
  reads every redeclaration's, block scope included, for `noescape` and
  the sanitiser's `nonnull` (kiln_codegen `class_b.rs:591`
  `redecl_params`, `ubsan.rs:2150`), and reflection reads their
  annotations (kiln_sema `meta/annot.rs:147`). A block-scope
  redeclaration that adds `noescape` changes the calls after it and is
  seen in the block's function only, not in the callers' units. Clang 19
  rejects `noescape` that differs across declarations; Kiln accepts it.
  Kiln is told (WP-100, "For Kiln").
- **Parameters of nested declarators** (§10.1). An attribute on the
  parameter of a function type that is itself a parameter's type
  (`void f(void (*cb)(int *q __attribute__((noescape))))`) is kept
  nowhere by Kiln: clang 19 mangles it into the type
  (`_Z1fPFvU8noescapePiE`), Kiln's type has no node for it, and the
  digest cannot see it. `differ/_/param-attr-nested-declarator` waits on
  Kiln.
- **A definition the chain leaves out** (§10.1). A function whose
  definition is instantiated outside its class (a friend's), or of the
  definition class, is written as having no defining declaration, so
  the written declarations' lists are all "before". Kiln's `ann_attrs`
  reads up to the emitted definition wherever the instantiation put it
  in the chain; an `annotate` on a written declaration after that point
  is then not emitted and the digest does not tell.
- **Unchecked attribute nodes** (§10.1): a node without `CHECKED` makes
  its unit indeterminate. Kiln checks every attribute of a declaration's
  list and of an `AttributedStmt` by the end of the translation unit, so
  only a node no list reaches could be unchecked, and the walk reaches
  nodes through lists.
- **A using-declaration and a member of the same name** (§5). In a
  class, a member declared after `using Bs::f;` against before it
  (`struct D : Bs { using Bs::f; int f(long); };` and the two swapped)
  reports `added D::f`: Kiln makes the shadow and hides it in one order
  and makes none in the other (`kiln_sema/src/using.rs:945`,
  `ColdDeclData` `hidden_shadow`). The programs mean the same
  ([namespace.udecl]/14); the difference is false, and accepted.
- **Names bound in a substitution whose expression is not kept**
  (digest §2.3, WP-153). A substitution whose expression is not kept is
  seen by its outcome (the argument's value or type, the alias's type,
  whether a constraint is satisfied, which partial specialisation or
  candidate was chosen), never by what its names bound: a default
  template argument at a use that takes it, an alias template's pattern
  at `A<X>`, a satisfaction check, a partial specialisation's match, a
  signature substituted during deduction, a `noexcept` or `explicit`
  operand, the type of a non-type template parameter, and an operand
  that only a type or a value holds in an instantiated declaration.
  After a transform, a name inside one can bind another entity (the
  argument-dependent lookup inside a trait finds another `swap` and
  the trait still says yes) with every digest equal. No call evaluated
  at run time is in such an expression. What the binding instantiated
  can change the program: a variable whose initialiser or destructor
  runs, a function with `constructor`, `destructor` or `used`. So can
  a candidate that is no longer considered, with no name bound to
  another entity; only WP-155 closes that (digest §2.3). Nothing
  counts these substitutions:
  the AST holds no fact to count. The design and the request to Kiln
  are `spec/landed/digest/WP-153.md` and
  `docs/requests/KR9-substitution-bindings.md`.
- **Modules** (§10.7) have rows and no experience behind them.
