# digest — the semantic digest

Status: draft (M1). Implemented by `slip_digest` (the walk over Kiln's AST)
and `slip_model` (the `digests` section). The per-kind and per-field rules
are in `digest-kinds.md`; this file defines everything they
plug into. How two digests are compared is [verify.md](verify.md). Design rationale:
[03-equivalence.md](../docs/design/03-equivalence.md). The Kiln facts behind
this spec are in `research/kiln-ast-inventory.md` of the context repository.

The digest is what makes "everything resolves as before" checkable: a
summary of one translation unit's resolved program such that two parses have
equal summaries exactly when they are the same program up to the things
listed in §1. `slip_digest` and `slip_verify` are Slip's trusted base. They
are specified first, kept small, and tested by tests whose purpose is to
make them fail (§14).

## 1. Principles

1. **The standard for inclusion is code generation.** A fact is in the
   digest if code generation, constant evaluation or overload resolution of
   a later declaration can read it. Kiln generates code from the same AST,
   so the rule is applied field by field (`digest-kinds.md`): every stored
   field is classified, and the classification, not a judgement made while
   coding, decides.
2. **Spelling is not meaning.** Excluded: source locations; whether and how
   a name is qualified; the alias, using-declaration or injected name a type
   or entity was reached through; redundant parentheses (those around an
   operand that is not instantiation-dependent, outside a canonical
   operand: `digest-kinds.md` §7); `struct` against
   `class`; `override`; the names of parameters and of block-scope entities;
   comments, whitespace and macro structure. Where a written form *does*
   change what code generation reads, that effect is included as a fact of
   its own: a qualified call to a virtual function is non-virtual; an
   unqualified dependent call uses argument-dependent lookup at
   instantiation; `decltype((x))` is a different type. A field that keeps
   a written form and that instantiation, mangling or code generation
   reads is therefore in the digest, unless writing it would make one of
   the edits excluded above a difference; the effect of each such field is
   then written by the rows `digest-kinds.md` names beside it.
3. **Nothing local to a parse.** No arena index, interning order, address or
   hash-map order reaches the digest. Entities are named by keys (§3) or by
   their position inside a unit (§4.3).
4. **End of translation unit only.** Kiln rewrites declarations in place
   until the unit ends (deduced types, resolved specifications, pending
   instantiations). The walk runs once, after the front end has finished the
   translation unit.
5. **Demand-driven state is kept apart.** What Kiln creates only when the
   translation unit uses it (implicit special members, implicit
   instantiations, lazily resolved specifications) is digested as *derived*
   units (§2.2), so that whether something was demanded never changes the
   digest of the thing it was demanded from.
6. **Unknown means different.** Anything the walk cannot digest exactly
   makes the unit *indeterminate* (§10), which compares unequal to
   everything, itself included. The walk has no default arm and no "other"
   case.
7. **Small on disk.** A shard carries one digest per file of declarations
   (§11), not one per entity, unless detail is asked for. Tens of thousands
   of observers that include the same headers cost tens of bytes each per
   header.
8. **Relative to the command.** The options of the compilation and the
   target (language mode, the sizes and signedness the target gives the
   builtin types, the contract semantic) are not digested. A digest is
   only ever compared with a digest of the same observer, which is the
   same command (`verify.md` §5). Two digests of different observers say
   nothing about each other ("Open questions").

## 2. Units

The digest of a translation unit is a set of **units**. A unit is one entity
with a key (§3) and the digest of everything that entity is, except the
content of other units, which it mentions by key. Comparing every unit
therefore compares the whole program: no unit needs to contain another.

### 2.1 Which entities are units

Every entity declared at namespace scope or class scope: namespaces,
classes and enumerations, enumerators, functions, variables, structured
bindings and the variable they decompose, data members, alias and typedef
names, templates of every kind, concepts, explicit and partial
specialisations, using-declarations and each name one brings into its
scope (a *using-shadow*), namespace aliases and deduction guides; and each
of the following, which declare no name: using-directives, using-enum
declarations, `static_assert` declarations, `consteval` blocks, friend
declarations, file-scope `asm`. A unit that declares no name is keyed by
its content (§3.3).

An explicit instantiation is not a unit of its own: it is a fact of the
specialisation it names, whose unit is then declared (§2.2) and says
whether it was explicitly instantiated, declared or defined. A
specialisation that was only named (a class template specialisation used
as an incomplete type) is not a unit either: nothing is known of it but
its key.

Entities declared inside a function body, a default argument, an
initialiser or a lambda are **local**: they are digested inside the unit
that contains them (§4.3) and are not units.

An entity declared more than once is one unit. Its digest covers the
entity as the declarations together define it (the definition; default
arguments from every declaration; attributes from every declaration), not
how many declarations there were or in what order. Attributes are the
exception: they are one list over the declarations, first to last, with
order and repetition kept (`digest-kinds.md` §10.1), because for some
kinds the first one written decides and for others each one counts.

One more unit exists per translation unit, the **order unit** (kind
`order`, parent `root`): the keys of the variables with static or thread
storage duration that are defined at namespace or class scope, in the order
of their definitions, and then the module units the translation unit
imports, in order. Initialisation order is program behaviour that no
other unit holds. An instantiated variable is not listed: its dynamic
initialisation is unordered ([basic.start.dynamic]), and where the front
end happens to emit it depends on where it was first used.

### 2.2 Declared and derived units

A unit is **declared** if the entity has a declaration written in source,
and **derived** otherwise. Derived units are:

- an implicitly declared special member function, an implicitly declared
  comparison operator or an implicitly generated deduction guide, **if it is
  referenced or defined** in the translation unit;
- an implicit instantiation: a class template specialisation's definition, a
  function or variable specialisation, a member of a specialisation, an
  instantiated default argument or default member initialiser;
- an entity every declaration of which an instantiation made outside its
  class (below), such as a friend declared only by a class template;
- a builtin function or type Kiln declares on first use;
- the object of a class-type constant used as a template argument, keyed
  by its content;
- a **resolved property** of a declared unit (kind `resolved`, parent the
  unit it belongs to, name the property): a fact Kiln computes for a
  declaration only when something needs it. Present only if computed.
  The **definition** of an implicit instantiation or of an implicitly
  defined member is one of these: such an entity is declared when it is
  named and defined only when it is used, so its unit holds the
  declaration and the resolved unit `definition` holds what the
  definition added (the body, the initialiser, a deduced type, the
  pattern a class was instantiated from). Otherwise a use added in one
  function would change the digest of every instantiation it is the
  first to define, against rule 5 of §1.

A declaration is **instantiated outside its class** when its lexical
context is an implicitly instantiated class specialisation or a member of
one, and its semantic context is not: the friends an instantiation
declares, whose semantic context is the namespace. Such a declaration is
no part of a declared unit: everything a declared unit reads over the
redeclaration chain (the declarations merged, the definition, the
pattern an entity was instantiated from) leaves it out, so a use that
instantiates a class does not change the units of the entities it
befriends (rule 5 of §1). An entity all of whose declarations are
instantiated outside their class is a derived unit; it is keyed as a
friend is (§3: a member-like friend in the class specialisation that
declares it, any other in its namespace). Its unit is read from one
declaration, its **canonical candidate**: the declaration whose declaring
class specialisation (its lexical context) has the least mapped key,
compared by the key's bytes (§3.4); among declarations one specialisation
makes (a friend declared twice), the one whose own declaration stream has
the least digest. The unit's stream, its origin file and its name are read
from the canonical candidate, once, so the unit does not depend on which
specialisation came first, and a definition another candidate holds is
no part of it (that definition is its class specialisation's).

A declaration in the body of a templated function, or of an instantiation
of one, that names an entity outside that function (a block-scope
`extern` declaration of a function or variable, a friend of a local
class) is no declaration a unit reads, and an entity declared only by
such declarations is no unit. Kiln chains the pattern's declaration to no
other ([temp.spec.general]/2), and an instantiation's declaration repeats
the pattern's: the entity it names is the one its pattern's declaration
names. Instantiating the function makes or moves no unit.

What an instantiation adds to an
entity that also has written declarations (a friend's definition) is the
class specialisation's: its friend unit refers to the entity, and the
definition is the entity's resolved unit `definition`.

An implicit member that Kiln has declared but nothing references is not a
unit: whether Kiln declared it depends on which lookups happened. What such
a member would be is determined by its class, whose unit holds the facts
(`DefinitionBits`: triviality, deletedness, constexpr-ness of each special
member).

### 2.3 Demand-driven state

Kiln's account of what in the AST exists only because the translation unit
used it (Kiln `62afe3289`), and where each goes. At the pinned
`913a490bb` (WP-101; its `kiln_ast` is that of `83566cc47`, WP-044) it
still holds: what Kiln added since (the definition-context operator sets,
the names of an overloaded member access, the shadows of a
using-declaration, the owner of a closure, the checked mark of an
attribute) is recorded where the source is parsed, whatever the unit
uses, and the written-reference and declaration-extent tables exist only
when a client turns them on, as does the preprocessing record, which is
the preprocessor's and not the AST's.

| State | In the digest as |
|---|---|
| Implicit special members: declared by lookups, defined on odr-use | derived unit if referenced or defined; else nothing |
| Implicit instantiations of every kind; the specialisation tables and their order | derived units; order not digested |
| Bodies of a closure's conversion functions; definitions of defaulted comparisons; the implicit `==` of a defaulted `<=>` | derived units |
| The definition of an implicit instantiation or implicit member: body, initialiser, deduced type, the pattern chosen for a class | `resolved` unit `definition` (§2.2) |
| An instantiated default argument or default member initialiser; whether one is still pending | derived unit; the pending mark is not digested |
| The value of an immediate invocation inside a default argument, computed at its first use | `resolved` unit |
| A function's deferred exception specification, once resolved | `resolved` unit; which functions can be deferred is a rule of `digest-kinds.md` §11.2 |
| A deduced return type of a template member | part of the instantiation's derived unit |
| A variable's evaluated constant value (`eval`), default-argument immediate values. At Kiln `913a490bb` `eval` is written when Kiln checks the declaration's initialiser: with the declaration, or, for one made by instantiation, when a use instantiates it (`digest-kinds.md` §11.3) | `resolved` unit |
| A class's key function; `dtor_delete`, `dtor_delete_this` (set when a virtual destructor is defined, which for an implicit or defaulted one is on use), `inheriting_ctor`, `inherited_ctor` (set when Kiln declares the inheriting constructor, on use) cold maps | `resolved` unit of the class or function |
| The values of the temporaries a variable's initialiser extends | with the variable's value, in its `resolved` unit |
| "vtable used" marks | not in the AST: nothing to digest |
| "A complete definition was required" marks; the lists of classes completed and of function definitions instantiated; the tables that record which specialisations exist and in what order they were made | not digested |
| Record layouts (`Ast::layouts`) | not digested (§6, last rule) |
| Concept satisfaction and normal-form caches | not digested; the result stored in each expression is (§6) |
| `USED` and `REFERENCED` marks; linkage and visibility caches | not digested |
| Lazily declared builtins (`builtin_id`, `lib_builtin`) | derived unit if referenced |
| `source_loc` values | symbolic (§9) |
| Arena indices and interning order of declarations, types, constants | never digested (§1 rule 3) |

`digest-kinds.md` marks each field behind a row of this table `demand`.
A field that turns out to be demand-driven and is not in this table is a
defect of this spec: the "demand independence" test (§14) exists to find
them.

Derived units are compared when both parses have them. A derived unit that
only one parse has is not by itself a difference (`verify.md` says when it
is reported): the reference that demanded it is in some unit, and that
unit's digest differs.

## 3. Keys

A **key** identifies an entity independently of the parse, of source
positions and of the translation unit. It is structural: an entity's key
contains the key of the entity it is a member of.

```
Key    := kind:   KeyKind
          parent: Parent
          name:   Name
          disc:   opt<digest>

Parent := root = 0                 the global namespace
        | c_linkage = 1            entities with C language linkage: one entity whatever namespace declares them
        | builtin = 2              entities the front end declares itself
        | unit = 3, id: KeyId      the enclosing namespace, class, function or template

KeyId  := H("slip.key.v1", Key)    the tagged digest of spec/model.md §3
```

`KeyKind`, `Name` and what `disc` holds for each kind are tables of
`digest-kinds.md`. The rules they follow:

### 3.1 Names

A name is an identifier; an operator (by Slip's own operator table, not
Kiln's numbering); `constructor`; `destructor`; a conversion function (with
the digest of the type converted to); a literal operator (its suffix); a
deduction guide; `anonymous` (the unnamed namespace); or one of the forms of
§3.3 and §4.3 for entities without a name.

### 3.2 Discriminators

`disc` distinguishes entities that share kind, parent and name:

- a function: the digest of its signature — parameter types, variadic,
  cv- and ref-qualifiers, explicit object parameter; and, for a function
  template, its template head and return type as well; and the trailing
  requires-clause if any ([over.load], [temp.over.link]). A function with C
  language linkage has no discriminator.
- a template: the digest of its template head.
- a specialisation: the digest of its canonical template arguments; its
  parent is the template's key.
- a partial specialisation: the digest of its template head and argument
  pattern.
- a using-declaration: the key of the scope its qualifier names.
- a using-shadow: the key of the entity the name stands for.
- a deduction guide: the template deduced, then as a function.

A non-template function's return type is not in its key: changing it
changes the unit, not which entity it is.

A discriminator is the tagged digest `H("slip.disc.v1", stream)` of a
stream of its own, written with the encodings of §4: its entities are
`Ref`s and its types type digests, so it maps under relocation (§3.4)
with everything else.

### 3.3 Entities without a name

- An unnamed class or enumeration at namespace or class scope is named by
  the first name its declaration introduces: the typedef name it is given
  for linkage, else its first declarator, else its first enumerator, else
  (an anonymous union) its first member. Failing all of those it is named
  by its ordinal among the unnamed types of its scope. A name-independent
  declarator or member (`struct { int a; } _;`, `union { int _; };`,
  P2169) names nothing, since a scope may hold several: it is skipped, and
  the rules after it apply.
- A declaration that introduces no name at all (§2.1) is **keyed by
  content**: `name` is `content` and `disc` is the unit's own digest. Two
  identical `static_assert`s in one scope are one unit. Adding or removing
  such a declaration adds or removes a key; it never renumbers another.

### 3.4 Mapped keys

Every `KeyId` in a digest is a **mapped** id: computed under a relocation
table `R` given to the walk. `R` is a list of rules

```
Rule := parent: Parent, kind: KeyKind, name: Name, disc: opt<digest>    an entity as the walk finds it
        to_parent: Parent, to_name: Name                                what it is to be called instead
```

A rule's parents are `Parent`s, not bare ids: an entity of the global
namespace has the parent `root`, and relocating a top-level namespace is
the first use of a rule. `parent` is in mapped terms (`verify.md` §2.1).

and the mapped id of an entity is computed bottom-up: map its parent and
everything in its discriminator first; if a rule matches the resulting
(parent, kind, name, disc), replace parent and name by the rule's; hash.
A key whose name is another entity's takes that entity's mapped name: a
specialisation and a partial specialisation are named by their template
(§3.2), so a rule that renames a template renames them with it, as their
parent, the template's key, already is (WP-103).
With `R` empty this is the identity, which is what the map phase uses.
Verification digests the post-image with the rules that undo the declared
relocations, so that a relocated entity, every member of it, every type
that mentions it and every signature that contains it get the keys they had
before, and the two digests are compared for plain equality. Where the
rules come from, and why this is sound, is `verify.md`. A walk under
rules also counts how each rule was used, so that a run can check that no
observer still has a renamed entity under its old name (`verify.md` §9).

Two different entities of one translation unit with the same mapped `KeyId`
are a **key collision**: every unit with that key is indeterminate (§10).
That holds of every entity the walk gives a key, not of units only: an
entity with no unit (a specialisation that is only named, a function that
only a block-scope `extern` declaration in a templated function declares,
§2.2) is still written into references by its key, so two entities the
rules give one key are a collision even if one of them, or neither, has a
unit; if no unit has the key, the order unit is marked, so that the
collision is a finding. Entities whose keys are equal under no rule are
one entity, not a collision (WP-103).
Declarations that the language lets a scope repeat and that Kiln does not
join into one entity (a using-declaration and its using-shadows, a
namespace alias, and every declaration keyed by content) are the same
entity when their keys and their digests are equal, and a collision when
only their keys are.
The exception is namespaces: two namespaces that a rule gives one key (the
new namespace mapped onto the old one, which still exists for what stays in
it) are **merged**. Each is written as a unit with that key; if what the
two namespaces are is the same, those are one unit (§11), and if not,
verification finds two digests under one key.

## 4. Streams

A unit's digest is `H("slip.unit.v1", stream)`: the tagged digest of a byte
stream the walk produces. The stream is a sequence of **nodes** in the
encodings of `spec/model.md` §2.

```
Node := tag: uv, fields…
```

`digest-kinds.md` gives every node its tag (Slip's own numbering, never a
Kiln discriminant), its fields in order, and their encodings. A node's
encoding is self-delimiting given the tables: every list is preceded by its
count and every optional field by its presence byte, so two different trees
cannot produce the same bytes.

### 4.1 References

Wherever a node refers to an entity:

```
Ref := none = 0
     | unit = 1,  id: KeyId                       an entity with a key (mapped, §3.4)
     | local = 2, ordinal: uv                     a local variable, parameter, binding or label of this unit (§4.3)
     | template_param = 3, depth: uv, index: uv
     | param = 4, level: uv, index: uv            a function parameter named inside a signature
     | outer = 5, owner: KeyId, index: uv         a local variable, parameter or binding of another unit (§4.3)
```

A reference names the entity, never the declaration it was found through:
a using-declaration, an alias or an injected class name resolve to their
target.

### 4.2 Types and values

A type is written as its digest, `H("slip.type.v1", type stream)`, where
the type stream is the structure of the **canonical** type (Kiln's
`canonical`, plus the adjustments `digest-kinds.md` lists where Kiln's
canonical form keeps a spelling), with entities as `Ref`s and component
types as their digests. A constant value is written by the rules of §7. Both
are memoised per parse; the memo is not observable.

A dependent type or key can hold an expression (the operand of a
`decltype`, an array bound, a `noexcept` operand, a requires-clause). It
is written as an expression of §6 inside the stream that holds it.
Entities the expression itself declares (the parameters of a
requires-expression or of a lambda) are numbered from 0 within that
stream; a reference to a local entity declared outside it uses the
ordinal of the unit being written (§4.3), and a type digest that holds
such a reference is memoised for that unit only. In a `decltype` operand
a parenthesis that is the whole operand is written, since it changes the
type.

### 4.3 Local entities

A local entity has an **owner**: the innermost entity with a key whose
own stream holds its declaration. For what a function body declares
(in any block of it) the owner is the function; for what a lambda's
body declares, the call operator (for a generic lambda the call
operator template, and in an instantiated body the specialisation);
for a closure type outside a function body, the declaration whose text
holds the lambda-expression (the variable or field for an initialiser,
the function for a default argument, the template for a default
template argument). A member of a local class has an ordinary key and
is the owner of what its own body or initialiser declares.

- Parameters, local variables, structured bindings and labels are
  **local objects**. Inside the unit whose stream holds their
  declaration they are numbered in the order the walk first meets
  them, by declaration or by reference, in one sequence for the unit,
  and referred to by `Ref::local` with that **ordinal**. Their names
  are not digested. An entity referred to before its declaration is
  walked (a `goto` to a later label) gets its ordinal at the
  reference; its declaration writes that ordinal and then what it is.
- Local classes, enumerations and closure types are **local types**.
  A local type has a key: its parent is its owner and its name is
  `local(n)`, `n` counting the local types of that owner in the order
  the walk of the owner's stream meets their **declarations** (the
  declaration statement of a class or enumeration; the
  lambda-expression of a closure). A reference never numbers a local
  type: a function's own return type can name one before the body is
  walked. The members of a local type have ordinary keys under it.
- A local type is digested in place, at its declaration, in the stream
  of the unit that holds it, and is not a unit: its number, its own
  stream, then the streams of the members a class's member lists name
  (§5), in the order of those lists. A closure whose lambda-expression
  the walk meets again (Kiln copies a default argument it rewrites)
  writes its number alone.
- What would be a derived unit were the type not local is a derived
  unit all the same: a specialisation of a generic lambda's call
  operator or conversion function, an implicit member defined on use.
  Such a unit is keyed under the local type and is no part of the
  stream that holds the type, so a use never changes a declared unit
  (§1 rule 5).
- A local alias or typedef is not digested: the types it names appear
  canonically wherever it is used.

The walk order is fixed by `digest-kinds.md` (each node lists its children
in order), so numbering is a function of the program and not of the parse.

**Numbering an owner.** The number of a local type is needed wherever
its key is: at its declaration, at a reference that the walk meets
first, and in another unit (the return type of a function that returns
a closure; a call of a closure held by a namespace-scope variable; the
key of a derived unit under it). It is found by numbering the owner:
a walk of the owner's own stream that stops at each local type's
declaration without entering its members, records the local types and
the local objects the owner declares, in order, reads no key, no type
and no reference, and writes nothing. So the numbers cannot depend on
the order units are written in, and each owner is numbered once. A
local type that the numbering of its owner does not find (a
lambda-expression that only a type holds, an owner that is a namespace
or a class) has no key: whatever needs it is indeterminate (§10).

**Local objects of another unit.** The body of a derived unit under a
local type names local objects that its own stream does not declare:
an instantiated body of a generic lambda refers to the variables of
the enclosing function its closure captures, and to those it only
names in an unevaluated operand or as constants. Such a reference is
`Ref::outer`: the key of the object's owner and the object's index
among the local objects that owner declares (the variables an
init-capture declares, in capture order; then the parameters, in
order; then the others in the order the numbering meets their
declarations). Inside the unit that holds the declaration the same
object is `Ref::local`. Neither form is allowed in a key.

**Lambda-expressions inside a key or a type.** A closure type whose
lambda-expression is walked inside a key or a type digest (the operand
of a `decltype` in a signature, a template argument) would need the
key that is being written. The key or type that holds it is
indeterminate.

- A value computed once and used at several places (Kiln's opaque values)
  is numbered in the same way as a local object, in a sequence of its
  own: the first use writes the number and the expression, later uses
  the number.

### 4.4 A stream does not depend on when it is written

**A unit's stream is a function of the translation unit and the unit
alone.** It does not depend on which units were written before it, on
whether a type digest, a key or a local numbering it uses was already
computed and memoised for another unit, or on whether it is written by
the ordinary walk or written again afterwards. Consequences the code
must keep:

- every numbering a stream uses (local entities §4.3, opaque values,
  back-references to a node already written in the same stream) starts
  from nothing at the head of the unit's stream, and is saved and
  restored around a stream written inside another (a type digest, a
  key);
- a memo holds only values that are functions of their key (a type's
  digest, an entity's key), never a position in some unit's numbering;
- nothing a stream writes is read from state that writing another
  stream changed.

Two interfaces stand on this property and check it: `slip_digest::records`
computes a unit's key again after the walk and compares it with the one
the walk used (`spec/verify.md` §2.3), and `slip_digest::trace` writes a
unit's stream a second time and accepts it only if it hashes to the
unit's digest (`spec/verify.md` §6.1). A unit for which the second
writing differs is a defect of the walk; it is counted
(`verify.traces.unstable`, `spec/verify.md` §7.2) and loses its
place, never its finding. The tests of the two interfaces hold the
property over every pair test: every record's key is the walk's
(`tests/records.rs`), and every unit has a trace whose stream hashes to
its digest (`tests/trace.rs`).

## 5. Declarations

For each unit kind, `digest-kinds.md` lists the fields of the stream. The
rules common to all:

- **Order that is behaviour is kept**: base classes, non-static data
  members, virtual functions, enumerators, parameters, template parameters,
  the captures of a closure. **Order that is not is removed** by sorting by
  key: the other members of a class, the friends of a class, the members of
  a namespace (which are not listed at all: a namespace's unit holds only
  what the namespace itself is).
- A class's unit lists its members by key and holds the class-level facts
  (bases, layout-affecting attributes, `DefinitionBits`); each member is a
  unit of its own.
- A function's unit holds its declaration facts (type, specifiers,
  attributes, default arguments, contracts) and, if it is defined, its body.
  A function defined in one parse and only declared in the other has
  different digests.
- A template's unit holds its head and its pattern, digested as written,
  with dependent constructs by structure (§8).
- Access, linkage, storage class, `inline`, `constexpr`/`consteval`,
  `virtual`, `final`, deleted and defaulted, the module a declaration
  belongs to, and every attribute Kiln stores are included.

## 6. Statements and expressions

Every statement and expression kind has a row in `digest-kinds.md`. The
rules common to all:

- An expression node writes its kind's tag, its value category and object
  kind, its type (§4.2), then its kind's fields.
- **Implicit nodes are nodes.** Implicit conversions (with their cast kind
  and base path), materialised temporaries, bound temporaries, cleanups,
  implicit `this`, default arguments and default member initialisers at
  their point of use, and constant-expression wrappers (with their value)
  are all written. Only a redundant parenthesis is skipped: one around an
  operand that is not instantiation-dependent, outside a canonical operand
  (`digest-kinds.md` §7 "Parentheses"); in a dependent operand a
  parenthesis can change the meaning (`decltype((x))`, `&(T::m)`).
- **The selected entity is written wherever one was selected**: the callee
  of every call, the constructor of every construction, the conversion
  function of every user-defined conversion, the allocation and
  deallocation functions of `new` and `delete`, the `begin`/`end`/`get`
  functions a range-for or structured binding uses, the functions a
  coroutine or a rewritten comparison uses, the destructor a temporary or a
  `delete` runs.
- A call's "found by argument-dependent lookup" bit and "had several
  candidates" bit are not written when the call is resolved: the callee is
  the fact. The same holds for a construction and for a resolved name. A
  qualified member call's suppression of virtual dispatch is written.
- Of a resolved name, the qualifier, the declaration it was found through
  and its template arguments as written are not written (§1 rule 2,
  §4.1). The template arguments are written when the entity named is a
  template, where they are part of what the name means.
- A use of a default argument writes which parameter's it is, by
  position, and not the expression: that is in the unit of the function
  (or, in an instantiation, in the default argument's own unit, §2.2).
- Stored results are written: the value of a type trait, of `noexcept(e)`,
  of a concept check or a requires-expression, of `sizeof...`, of
  `offsetof`.
- Values the front end does not store (`sizeof`, `alignof`, the condition
  of a `static_assert`) are covered by their operands: they are functions
  of types and expressions whose digests are compared, in a translation
  unit all of whose units are compared.

## 7. Constant values

A constant is written by structure, never by Kiln's interned id:

- integers and booleans by type and value; floating-point values by format
  and exact bits; null pointers by kind;
- a pointer by its base (an entity as a `Ref`; a string literal by its
  kind, type and bytes; a label by its ordinal; a lifetime-extended
  temporary by the `Ref` of the declaration that extends it and the
  temporary's index among that declaration's, since a temporary can hold
  a pointer to itself and its value is written where it is materialised;
  `typeid` by the type; another constant of Kiln's store by its content,
  `H("slip.value.v1", stream)` of the stream that writing that constant
  here would give, computed apart from the unit as a key is), its offset
  and designated type;
- a pointer to member by the member's `Ref` and its path;
- an aggregate by its type and its target-layout image with padding bytes
  zeroed, the ranges of the image that hold no value (padding and
  uninitialised subobjects: not a function of the type alone), each
  relocation written as the pointer it holds, and each active union
  member as a `Ref`.

String literals are compared by content: whether two literals are the same
object is unspecified by the language and is not digested.

## 8. Templates and instantiations

- A template's pattern is digested with dependent names by structure: a
  dependent name's identifier and nested-name structure; for an unresolved
  call, the set of declarations found at the definition (as sorted `Ref`s)
  and whether the name is subject to argument-dependent lookup at
  instantiation.
- Template parameters are `Ref::template_param`; their names are not
  digested.
- Each specialisation that exists in the translation unit is a unit: an
  explicit one is declared, an implicit one is derived (§2.2). Its digest is
  that of the instantiated declaration, exactly as for a non-template.
  Dependent lookups resolve at instantiation, so this is where a change in
  what an unqualified call inside a template finds becomes visible.
- A specialisation records the template or partial specialisation it was
  instantiated from, as a `Ref`.
- The guarantee for templates rests on the instantiations. A pattern's
  digest holds what Kiln's AST holds for the pattern, and some of what
  decides an instantiation is not there (`digest-kinds.md`, "Hazards
  accepted for now"): a transformation whose only effect is on such a
  fact is found in every specialisation the translation unit
  instantiates, and in no other.

## 9. Values that come from names and positions

`__func__`, `__PRETTY_FUNCTION__` and the fields of `std::source_location`
are written **symbolically** (which builtin, not its value), as are literals
that come from the expansion of `__LINE__`, `__FILE__`, `__COUNTER__`,
`__DATE__`, `__TIME__` and their relatives.

The expression's **own type** and the **constant it stands for** are
symbolic with it. The array type of `__func__` and its kin (`const
char[N]`, `N` the length of a name) is written as that builtin's type,
without the bound. The object that an immediate invocation builds from
such values and nothing else (`std::source_location::current()`, in a
default argument as anywhere) is written as the builtins it was built
from, not as its bytes.

Whatever **consumes** such a value is written by value as always, so a
real change is seen: an array bound from `__LINE__`, `sizeof(__func__)`,
the length of a name deduced as a template argument, a variable declared
with the array's type or a reference to it, a constant computed from the
characters.

Why the type goes with the value: a declared relocation says that names
change, and a value that is a function of a name changes by exactly that,
as a mangled name does. `assert` expands to `__PRETTY_FUNCTION__`; were
its length a fact, no function that asserts could be relocated and
verify. A program that observes such a string (a log that is parsed, a
registry keyed by it) is told by the report, not by the verdict
(`verify.md` "Open questions").

## 10. Indeterminate units

A unit is **indeterminate** when its digest cannot be trusted to mean what
this spec says:

- it contains an invalid declaration, an error-recovery expression or a
  type that contains an error;
- it contains a kind or a value that `digest-kinds.md` marks `unsupported`;
- its key collides with another entity's (§3.4);
- it holds demand-driven state that `digest-kinds.md` could not assign to a
  derived unit.

**No unit vanishes.** An entity that §2.1 makes a unit and whose key
cannot be formed (an unexpected parent or name, a cycle, a kind whose key
is not written yet) is still a unit: it is written under a fallback key
(its own kind; parent the nearest enclosing entity that has a key, else
`root`; name `content`; `disc` the unit's digest) and is indeterminate.
A declaration of a kind that has no key kind at all makes the unit of
the nearest enclosing entity indeterminate, the order unit at the top
level. A key built from an indeterminate key (its parent's, its
template's, one in its discriminator) is indeterminate for the same
cause.

**A collision does not travel.** Collisions are found after the walk,
when the keys of the whole translation unit are known: the mark is put
on the units that share the key, and only on them. A unit that refers to
one of them wrote the shared key in its stream during the walk and stays
determinate. This is sound because a collision is never silent: each
colliding key is an `indeterminate` finding (`verify.md` §4 rule 1, or
`derived_indeterminate` for derived units), so an observer with a
collision is never `verified`. The per-unit "unchanged" of a referrer is
then one line of a report that already fails; it is not a claim Slip
makes on its own.

Its digest is still computed and recorded, with the `indeterminate` flag.
Verification treats an indeterminate unit as different from every unit.
The number of indeterminate units is a counter (§12) that the corpus
baselines gate: on a corpus that parses without errors it must be zero
before M1 closes, or each remaining cause is listed in "Open questions".

Digests are produced only for an observer whose shard is `ok`.

## 11. The `digests` section

Section 19 of the shard (`spec/model.md` §8).

```
Digests := scheme: uv              1; changes whenever any stream, key or table of this spec changes (rule below)
           tu:     digest          H("slip.tu.v1", list<GroupSum>), groups in the order below
           groups: list<Group>     sorted by (class, file)

Group   := class:  GroupClass
           file:   bytes           the subject path of a file; empty for class `tu`
           count:  uv              units in the group
           indeterminate: uv       units in the group with the indeterminate flag
           digest: digest          H("slip.group.v1", list<Unit>) over the group's units
           units:  opt<list<Unit>> sorted by (key, digest); present when detail was asked for

GroupSum   := class, file, count, indeterminate, digest
GroupClass := declared = 0 | derived = 1 | tu = 2

Unit    := key: digest, digest: digest, flags: uv      flags bit 0: indeterminate; other bits must be 0
```

From the first release, `scheme` changes with every change to a stream,
a key or a table of this spec, without exception. Before that release no
shard outlives the build that wrote it (shards are stored by plan, and
the cache keys of `spec/exec.md` that will replace the plan name the
tool's version), so no shard of one scheme meets a reader of another, and
`scheme` stays 1 until then.

- A declared unit belongs to the group of the file that holds the entity's
  first declaration in the translation unit. A derived unit belongs to the
  derived group of the file that holds the first declaration of what it
  derives from (the template, the class, the function). The single `tu`
  group holds the order unit and every namespace: a namespace is reopened
  wherever its members are, and which file opens it first is not a fact
  about it. Builtins are in the derived group with the empty path.
- The file of a declaration is the file of its location, or of the
  outermost macro expansion that produced it. A **subject path** is the real
  path of that file, with the root of a post-image mapped back to the root
  of the subject, so that the same file has the same group before and
  after.
- The walk is given the subject path of a buffer by its caller (`Input`
  `subject_path`) and does not compute one. What a caller answers for a
  buffer that is not a file read from disk is the caller's rule and is
  stated where the caller is: the map task gives a buffer the front end
  made (`<built-in>`, the command line's macros) its name in the source
  manager and refuses any other buffer that is not one of the shard's
  inputs (`spec/model.md` §8.5 "Subject paths"); the snippet runner of
  the tests gives the snippet's path.
- A declared unit whose first declaration has **no location** (the walk
  finds no file for it) is written with the empty path. The walk does not
  invent a file and does not drop the unit; a shard that holds such a
  group breaks `spec/model.md` §8.2 and is refused where it is written
  (`spec/model.md` §8.5). An entity that Kiln declares without a
  location and that §2.1 makes a declared unit is therefore a defect of
  the tables to be decided (derived, as builtins are, or `tu`), not a
  case this section covers: none is known.
- Two units of one group with the same key and digest are one unit.
- **Detail** is a parameter of the walk: `none` (no `units` lists), `all`,
  a list of groups by `(class, file)`, or `differing` from a list of
  `GroupSum`s (every group whose sum is not in that list, and every group
  that has an indeterminate unit; this is what a verify job asks for,
  `verify.md` §4.1, §5). The group digests and `tu` are the same whatever
  the detail.
- `indeterminate` is in the sum, although the group's digest already
  covers every flag, so that a reader of sums alone knows which groups
  hold a unit that verification must report (`verify.md` §4.1).

Equal `tu` digests mean equal unit sets. Unequal ones are narrowed to the
groups that differ and the groups with an indeterminate unit, whose unit
lists (asked for with detail, by parsing
again if the shard does not have them) are compared unit by unit. Groups
are an index, not a rule: verification's definition is always the
comparison of unit sets (`verify.md`).

## 12. Counters

| Counter | Meaning |
|---|---|
| `digest.units` | units, declared and derived |
| `digest.units.derived` | derived units |
| `digest.groups` | groups |
| `digest.indeterminate` | indeterminate units |
| `digest.collisions` | keys shared by different entities |
| `digest.bytes` | bytes of unit, type and key streams hashed |
| `digest.indeterminate.<cause>` | indeterminate keys whose first cause is of this kind: `error`, `unsupported`, `unaccounted`, `collision`, `undeclared_local`, and `pending` while the walk has areas not yet written. Present only when not 0 |
| `digest.indeterminate.pending.<area>` | of the `pending` ones, by area. Goes with `Cause::Pending` |
| `digest.cause.<cause>.<word>` | for the causes that carry a text (`unsupported`, `unaccounted`): indeterminate keys whose first cause has this text. The word's form and the function that makes it (`slip_digest::map::cause_word`) are `spec/model.md` §8.5 |

These three count keys, each once, under the first cause the walk met
for it; `digest.indeterminate` counts units, so the
`digest.indeterminate.<cause>` counters can add up to less than it.
`digest.cause.unaccounted.walk.depth` is the number of keys cut off by
the bounds of §12.1.

### 12.1 The walk's bounds and its stack

The walk recurses over types, keys and nested entities, and over
statements and expressions. Both recursions are bounded, and a stream
that would pass a bound is indeterminate with the cause
`unaccounted` `walk/depth` instead of going deeper:

| Bound | Counts | Value |
|---|---|---|
| `DEPTH_LIMIT` | nested streams other than statements and expressions (a type inside a type, a key's parent, a template argument list, a declaration stream) | 200 |
| `NODE_LIMIT` | levels of statement and expression recursion. A chain that the walk writes in a loop (operators, calls, member accesses, casts; `else if`, runs of labels, nested loop bodies) is one level whatever its length | 10 000 |

- The walk needs `slip_digest::STACK_SIZE` bytes of stack (64 MiB) and
  must run on a thread that has them. A caller checks before it calls
  (`spec/model.md` §8.5 "The stack"); the walk does not check for itself
  and does not grow a stack.
- `STACK_SIZE` is at least four times what both bounds can use, by
  measurement: a test walks to each bound over the deepest path of
  frames each area has (`frames_fit_the_stack` and the tests an area
  adds to it) and asserts the factor. The measurement that counts is the
  one of the **release profile** (`spec/harness.md` §9.3); the test
  profile's is a second check, with larger frames.
- An area that adds a recursion through `enter_node` or the depth count
  adds its path to that measurement in the same change. A recursion that
  enters neither count is a defect.
- The bounds and `STACK_SIZE` are part of the scheme (§11): a unit that
  is indeterminate for `walk/depth` under one value would have a digest
  under another.

## 13. Completeness obligations

The claim "every field that code generation reads is digested" is kept
true by mechanism, not by review alone.

1. **Tables.** `digest-kinds.md` has one row for every variant of every
   Kiln enumeration the AST uses, and one row for every stored field of
   every AST record, payload, side record and cold map, each with a class
   (entity, type, value, child, name, flag, written, location, bookkeeping),
   a decision (`in`, `out`, `symbolic`, `demand`, `unsupported`). A field
   that packs several facts into one word has one row per fact and none
   of its own. A decoded form Kiln's interface returns in place of a
   stored record (a type view, a constant) and a type whose values are
   named constants have tables like any other.
2. **No wildcard.** Every dispatch of the walk over a Kiln enumeration lists
   every variant. A new variant in Kiln does not compile.
3. **Pinned schemas.** For every Kiln type the tables cover, `slip_digest`
   asserts at compile time that the type's `Snap::SCHEMA_HASH` is the value
   recorded when its rows were last reviewed. A Kiln change that adds,
   removes or retypes a field or variant does not compile, and names the
   type. The fix is to review the rows, then update the pin; the commit says
   what was reviewed.
4. **Tables against Kiln.** A test reads `digest-kinds.md` and Kiln's own
   schema descriptors (`kiln_snap::Schema`) and fails if a variant or field
   has no row, or a row names something that does not exist.
5. **Tables against tests.** Every row decided `in` names at least one
   *must-differ* test (§14); every row decided `out` with class *written*
   and every row decided `symbolic` names at least one *must-equal* test;
   every `demand` row names a *demand* test and every `unsupported` row
   whose construct Kiln can build an *indeterminate* test. A test fails
   for a row that names none or names a test that does not exist.
6. **What the pins cannot see.** A word whose meaning changes inside Kiln
   without its type changing (a per-kind operand, a bit of a flag word) is
   invisible to 3 and 4. Mutation tests (§14) and, when Kiln provides it, a
   public table of operand roles (`docs/design/05-kiln-interface.md` KR5)
   cover it; until then every such word is listed in "Open questions".

## 14. What the tests must show

- **Must-equal pairs** (`crates/slip_digest/tests/equal/`): two sources that
  differ only in something §1 rule 2 excludes; their digests must be equal.
- **Must-differ pairs** (`tests/differ/`): two sources that differ in one
  included fact; the digests must differ, and the differing unit must be the
  one the test names. One pair at least per `in` row.
- **Determinism**: every test source is parsed twice in one process and in
  two; all digests equal. On the corpora this is invariant (a) of the
  engineering plan.
- **Demand independence** (`tests/demand/`): a source, and the same source
  with an added declaration that only *uses* existing entities (forcing
  implicit members and instantiations); every unit present in both must
  be equal. One pair at least per `demand` row.
- **Indeterminate** (`tests/indeterminate/`): a source holding a construct
  decided `unsupported`; the unit that holds it must have the flag.
- **Mutation**: the catalogue of `spec/pitfalls.md`, each row a correct edit
  that must compare equal under its relocations and a wrong edit that must
  be caught.
- **Format**: the `digests` section round-trips, and every malformed
  encoding is rejected (`crates/slip_model/tests/`).
- **Completeness**: obligations 4 and 5 of §13.

### 14.1 What a pair test's `expect.txt` says

A pair test's directory (`spec/digest-kinds.md` §1.5) may hold
`expect.txt`. Its lines, each trimmed; a blank line and a line beginning
`#` are none (`# pending` marks a test the runner skips and lists):

| Line | Holds when |
|---|---|
| `changed <name>` | a key of the name has declared or `tu` units on both sides, and they differ |
| `added <name>` | side B has declared or `tu` units under a key of the name and side A has none |
| `removed <name>` | the other way round |
| `indeterminate <name>` | a unit of the name is flagged for a cause other than `Pending` (§10) |
| `derived-added <name>` | side B has a derived unit (§2.2) under a key of the name and side A has none |
| `derived-removed <name>` | the other way round |
| `derived-changed <name>` | the derived units under a key of the name differ in any way, presence included; under `exact derived`, both sides have one and they differ |
| `exact derived` | a directive, not a line: the derived keys that differ between the sides are exactly the keys the `derived-*` lines name, each in the way its line says |

1. Any other line is an error of the test, as is a line written twice
   and a second `exact derived`. The directive is exactly the text
   `exact derived`.
2. A name is the name of the key's first unit, from side A when A has
   the key. Lines are compared by name: a line names every key of its
   name, and two keys of one name that differ the same way are one line.
3. **`differ`** reads every line but `indeterminate`, and needs one
   besides the directive. Over declared and `tu` units, the keys that
   differ are exactly the `changed`, `added` and `removed` lines, whatever
   the `derived-*` lines are. A `derived-added` or `derived-removed` line
   that does not hold fails the test, with or without the directive.
   Without the directive a derived difference no line names is free.
4. **`demand`** reads `added` and `derived-added` lines and the
   directive. Every unit both sides have is equal; the declared keys only
   B has are exactly the `added` lines and A has none B lacks; every
   `derived-added` line holds, and under the directive the derived
   differences are exactly those lines, so that an unnamed instantiation
   only B has, or any derived unit only A has, fails.
5. **`equal`** reads no line and no directive: every unit both sides have
   is equal already, and a unit only one side has, declared or derived,
   fails the test (the derived groups are part of the `Digests`
   compared). **`indeterminate`** reads `indeterminate` lines only.

## Tests

- `crates/slip_digest/tests/equal.rs`, `differ.rs` — §14 pairs, one
  directory of sources per test under `tests/equal/` and `tests/differ/`.
- `crates/slip_digest/tests/determinism.rs`, `demand.rs`,
  `indeterminate.rs` — §14.
- `crates/slip_digest/tests/tables.rs` — §13 obligations 4 and 5.
- `crates/slip_digest/tests/runner.rs` — §14.1, the runners' reading of
  `expect.txt`, over the fixtures of `tests/runner/`.
- `crates/slip_digest/tests/records.rs`, `trace.rs` — §4.4.
- `frames_fit_the_stack` in `crates/slip_digest/src/expr.rs` and the
  tests each area adds to it — §12.1.
- `crates/slip_model/tests/formats.rs` — §11, the section's encoding.
- `spec/pitfalls.md` and its tests — the mutation catalogue.

## Open questions

- Kiln has no "complete the translation unit" step, and advises against
  forcing one (resolving a `noexcept` can instantiate; defining an implicit
  member can diagnose). §2.3 therefore keeps demand-driven state apart
  instead. Kiln's own advice goes further (digest only what is written and
  leave instantiations out); this spec keeps instantiations as derived
  units because dependent lookups resolve only there
  ([03](../docs/design/03-equivalence.md) §7 row 6). If derived units prove
  unstable between equal programs, the fallback is to compare them only in
  a stricter mode.
- Whether two parses of the same input give the same instantiation order
  matters only to performance here (the digest sorts), but is assumed by
  the determinism test's failure messages.
- Recognising a literal that came from `__LINE__` and its relatives (§9)
  needs a public location query in Kiln (KF3). Until then such literals are
  written by value, and a transform that moves lines sees a difference in
  units that use them.
- `sizeof` and `alignof` are covered by their operands (§6). A direct layout
  digest per class (size, alignment, field offsets, vtable slots) would be
  an independent check of the same facts and is cheap if Kiln exposes its
  layout query.
- Detail by list of files, and storing unit lists for owned files by
  default so that a mismatch needs no second parse.
- A type-alias template specialisation is transparent like any alias. Is
  there a case where the alias chosen changes a deduction or a mangled name
  that code generation reads?
- §1 rule 8: if digests of different observers are ever compared (to
  verify a shared header once for many observers; the agreement check of
  `verify.md` §8), the options and the target have to enter the digest
  first, as one more unit of the `tu` group.
- A cold map with no entry for a node writes the empty form of its rows
  (`digest-kinds.md` §2.1), several bytes hashed for every declaration
  and expression, so that the stream does not depend on which maps Kiln
  filled. If `digest.bytes` shows this to matter, the rows can be limited
  to the kinds that can have an entry; that changes `scheme`.
- The order unit lists no instantiated variable (§2.1). Kiln emits them
  at a fixed place, and a program that relies on unordered
  initialisation can observe it.
- An entity's attributes are an ordered list over its declarations, so
  moving a pure flag (`cold`, `noinline`) from one declaration to
  another, or repeating it, is reported as a difference. Removing that
  needs a table saying, per attribute kind, whether it is a flag,
  whether the first one decides or whether each one counts. In the same
  way `[[gnu::x]]` and `__attribute__((x))` differ (`AttrNode` `syntax`),
  though Kiln reads the spelling only to check `noreturn` and
  `nodiscard`.
- Attributes that only affect diagnostics (`[[nodiscard]]`,
  `[[deprecated]]`) are included. A policy to exclude them would let a
  transform change them without declaring it.
