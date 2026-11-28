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
   then written by the rows `digest-kinds.md` names beside it. What an
   operand inside a written type binds (`decltype(make(x))`, an
   expression template argument) is not spelling either, though the
   type or value that holds it is all the unit writes: it is written in
   the unit's operand unit (§2.4, WP-156).
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
whether it was explicitly instantiated, declared or defined. The members
of a class so instantiated, by an explicit instantiation declaration
(`extern template`) or definition, are declared units too, all of them,
whatever Kiln instantiated before (`digest-kinds.md` §5.2, §11.10). A
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
- an implicit instantiation: a class, function or variable template
  specialisation whose kind is `ImplicitInstantiation`, a member of one
  (a member's kind is its effective kind, `digest-kinds.md` §11.10: the
  greater of its own and its class's, so a member of an explicit
  instantiation is declared whatever Kiln instantiated first), an
  instantiated default argument or default member initialiser;
- an entity every declaration of which an instantiation made outside its
  class (below), such as a friend declared only by a class template;
- a builtin function or type Kiln declares on first use;
- an entity that would be declared but **no declaration of which has a
  location** (Kiln's implicit global allocation and deallocation
  functions while no source redeclares them, `__int128_t`, an implicit
  `std::align_val_t`), whatever flags its declarations carry: a unit
  only if referenced, as a builtin (`spec/landed/model/WP-050.md`);
- the object of a class-type constant used as a template argument, keyed
  by its content;
- a **resolved property** of a unit, declared or derived (kind
  `resolved`, parent the unit it belongs to, its *owner*, name the
  property): a fact Kiln computes for a declaration only when something
  needs it. Present only if computed. It is a derived unit whatever its
  owner is (`digest-kinds.md` §11.1).
  The **definition** of an entity Kiln may define on use is one of these
  (the *definition class*, `digest-kinds.md` §11.9: an implicit
  instantiation, a specialisation named by an explicit instantiation
  declaration, every member of an instantiated class, an implicitly
  declared or first-declaration-defaulted function, a friend declared
  only by instantiations): such an entity is declared when it is named
  and defined only when it is used, so its unit holds the declaration
  and the resolved unit `definition` holds what the definition added
  (the body, the initialiser, a deduced type, the pattern a class was
  instantiated from, a class's members). Otherwise a use added in one
  function would change the digest of every instantiation it is the
  first to define, against rule 5 of §1. The entity's own unit is
  derived or declared as the entity is (the own unit of a member of an
  explicit instantiation is declared); its `definition` is derived in
  every case, so a difference in it is reported when both parses have
  it and differ (`verify.md` §4), and a `definition` on one side only is
  not a difference by itself.

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

**Every instantiation has a unit** (WP-158). Every function body and
every default argument or default member initialiser that Kiln
instantiated in this parse is in the stream of a derived unit, whatever
made the front end instantiate it: a call, an unevaluated operand
(`decltype`, `sizeof`, `noexcept`, a requires-expression), a conversion
(a closure to a function pointer), a deduced return type, an address
taken. Its key, group and cause are those above: the body is the
`definition` of its specialisation, or written in place in the unit of
the local type's owner; the default is a `default_argument` unit
(§4.3). The rule is checked against what Kiln instantiated, not against
the units the walk chose: the counter `digest.instantiated.unheld`
(§12) counts

- the bodies of the functions in Kiln's list of instantiated function
  definitions (`DeclArena::fn_def_insts`), and of every function
  declaration with a body that an instantiation made (its own
  specialisation or member-specialisation kind, else the kind of the
  context it is declared in: a local class's member, a lambda's call
  operator in an instantiated body);
- the default arguments of the parameters of those function
  declarations, and the default member initialisers of fields an
  instantiation made;
- the initialiser of every variable an instantiation made that is not
  local to a function: a variable template specialisation, a static
  data member of a class template specialisation (also a member of a
  local class in an instantiated body). A variable declared in a
  function body is not one: its initialiser is a statement of that
  body and is held with it;

that no kept unit's own stream wrote, leaving out a declaration in a dependent
context (a pattern, written as one) and one inside a local type that a
stream met and could not number (§4.3 "What these rules do not yet
show": the unit that holds it is indeterminate, unequal to everything).
It is 0 over every fixture (§13 item 7) and is reported on the corpus.

"Wrote" is exact: a body, a default or an initialiser counts as held
only when it is written where the unit's own stream is being written
(the unit's level, not in a profile, not detached, not a numbering
walk: the condition under which the emitted mark is recorded in place,
§11). A walk whose stream is dropped holds nothing: the second walk of
a default argument's immediate invocations, and the streams of the
candidates the canonical candidate of a friend is chosen from (§2.2
above). The counter reads what Kiln instantiated, so it cannot see a
definition Kiln never made: see the limit of §2.3 "a friend definition
not instantiated".

An implicit member that Kiln has declared but nothing references is not a
unit: whether Kiln declared it depends on which lookups happened. What such
a member would be is determined by its class, whose unit holds the facts
(`DefinitionBits`: triviality, deletedness, constexpr-ness of each special
member).

### 2.3 Demand-driven state

Kiln's account of what in the AST exists only because the translation unit
used it (Kiln `62afe3289`), and where each goes. At the pinned
`60fb5ab9d` (WP-163; `4a83f6e3c` from WP-066) it
still holds: what Kiln added since (the definition-context operator sets,
the names of an overloaded member access, the shadows of a
using-declaration, the owner of a closure, the checked mark of an
attribute) is recorded where the source is parsed, whatever the unit
uses, and the written-reference and declaration-extent tables exist only
when a client turns them on, as does the preprocessing record, which is
the preprocessor's and not the AST's. Every digested parse turns the
written-reference table on (WP-156 fix round 3; §2.4 site 9): it is
what the source wrote, the same whatever the unit uses.

| State | In the digest as |
|---|---|
| Implicit special members: declared by lookups, defined on odr-use | derived unit if referenced or defined; else nothing |
| Implicit instantiations of every kind; the specialisation tables and their order | derived units; order not digested |
| Bodies of a closure's conversion functions; definitions of defaulted comparisons; the implicit `==` of a defaulted `<=>` | `resolved` unit `definition` of their derived units |
| The definition of an implicit instantiation or implicit member: body, initialiser, deduced type, the pattern chosen for a class | `resolved` unit `definition` (§2.2) |
| An instantiated default argument or default member initialiser; whether one is still pending | derived unit; the pending mark is not digested |
| The value of an immediate invocation inside a default argument, computed at its first use | `resolved` unit |
| A function's deferred exception specification, once resolved | `resolved` unit; which functions can be deferred is a rule of `digest-kinds.md` §11.2 |
| A deduced return type of a template member | `resolved` unit `definition` of the instantiation |
| A variable's evaluated constant value (`eval`), default-argument immediate values. At Kiln `4a83f6e3c` `eval` is written when Kiln checks the declaration's initialiser: with the declaration, or, for one made by instantiation, when a use instantiates it (`digest-kinds.md` §11.3) | `resolved` unit |
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
unit's digest differs, except where the demand is recorded in no unit (a
substitution whose result is not kept, a candidate no longer considered);
a one-sided derived unit marked emitted unreferenced (§11) is therefore a
finding of its own (`verify.md` §4 step 3, WP-155).

**A limit: names bound in a substitution whose expression is not kept**
(WP-153; found by WP-139, `spec/landed/edits/WP-139.md` finding 3). A
substitution whose expression is not kept is seen by its outcome, never
by what its names bound:

```cpp
struct Base { static int s(); };
namespace n { struct S : Base { static int s(); }; }
template <class T, int = sizeof(T::s())> int call() { return 0; }
int u() { return call<n::S>(); }
```

Rename `n::S::s`. `T::s` at `call<n::S>` now finds `Base::s`. Every
digest is equal: the default argument's value is in the
specialisation's key, and the substituted expression is nowhere. The
shapes, and the outcome that is compared today:

| Shape | Outcome that is compared |
|---|---|
| a default template argument at a use that takes it | the argument, in the specialisation's key |
| an alias template's pattern at `A<X>` | the type the alias names |
| a requires-clause, a type-constraint or a concept-id at a satisfaction check | the stored result; the candidate chosen |
| a partial specialisation matched (`void_t<decltype(swap(…))>`), also one that matched and was not chosen | the pattern the specialisation was made from |
| a signature substituted during deduction (`-> decltype(T::s())`, a parameter type), also of a candidate that lost | the specialisation's type |
| a `noexcept` or `explicit` operand substituted at instantiation | the stored value |
| the type of a non-type template parameter, substituted with the arguments before it | the converted argument |
| a requires-expression, or an operand that only a type or a value holds (`int a[sizeof(T::s())]`), in an instantiated declaration | the value; the type |

In each of these an operator, a constructor or a conversion function
that the substitution selected is as unseen as a name is.

No call evaluated at run time is written in such an expression: it is
an unevaluated operand, or it is evaluated during translation. So no
call of the substituted expression changes at run time while the
outcome is equal. Two things are not proven.

The first is the statement Slip makes, "every name resolves as it
did": in a namespace migration, the argument-dependent lookup inside a
trait can find another `swap` and the trait still say yes.

The second is behaviour that follows from what was instantiated. A
name that binds a function with a deduced return type instantiates its
body. That can instantiate a definition that is emitted whether or not
it is referenced: a variable of static or thread storage duration whose
initialiser or destructor runs, or a function with the attribute
`constructor`, `destructor` or `used`. Such a demand is written in no
unit, so the argument of the paragraph above does not hold for it: the
derived units it made are on one side only and no unit both sides have
differs. Since WP-155 such a definition's unit carries the mark
emitted unreferenced (§11), and `spec/verify.md` §4 step 3 reports the
one-sided marked unit as `derived_only_pre` or `derived_only_post`. It
holds whether the substitution succeeded or failed after the name
bound.

The same behaviour changes with no name bound to another entity, when
a candidate is no longer considered:

```cpp
extern "C" int puts(const char *);
inline int note() { puts("registered"); return 0; }
template <class T> struct Tr { static int r; };
template <class T> int Tr<T>::r = note();
namespace lib {
struct X {};
template <class T> auto probe() { return (void)&Tr<T>::r, 1; }
template <class T, int = sizeof(probe<T>())> void f(T, char);
}
void f(lib::X, long) {}
void use() { f(lib::X(), 0); }
int main() { use(); }
```

Write the call `::f(lib::X(), 0)`: the callee is the same function,
argument-dependent lookup no longer supplies `lib::f`, its default
template argument is not substituted, `Tr<lib::X>::r` is not
instantiated, and the program no longer prints. No record of what
names bound can see this. WP-155 closes it: a derived unit that is
marked emitted unreferenced, and that one image only has, is a finding
(here `derived_only_pre Tr<lib::X>::r::<definition>`; pitfall row E24).
`spec/landed/digest/WP-153.md` §1 has the programs and what closes
each; `spec/landed/verify/WP-155.md` has the mark and the rule.

**What the operand units close (WP-156).** A name in an operand that
only a type or a value holds, inside a written type (`decltype`,
`typeof`, an expression template argument, a `noexcept` operand, the
written argument list of an alias template specialisation), is written
in the operand unit of the unit that holds the type (§2.4): outside any
template (`decltype(n::S::s()) v;`, which §1 rule 2 alone would make
spelling) and in a declaration an instantiation made (a specialisation's
substituted return type, parameter types and `noexcept`, a member of an
instantiated class). Of the shapes above, these are closed where such
an operand is written: the substituted signature of a specialisation
that exists, a `noexcept` operand substituted at instantiation (in the
operand unit of the resolved `exception_spec` unit), and an operand in
a written type of an instantiated declaration. What stays a limit, by
name, each pinned by an equal pair whose comment names it
(`tests/equal/_/limit-operands-*`):

| Limit | Why | Asked of Kiln |
|---|---|---|
| a type name alone in an array bound (`int buf[sizeof(X)]`) | Kiln keeps a constant bound as a value only, for every declarator; the site (3) is reserved. A name in a bound's expression (`sizeof(make(x))`) is a site-9 entry and is written (§2.4, WP-156 fix round 3), so the limit narrows to a type name alone | the bound as written (KR9 §5.11) |
| a pack index after substitution (`T...[sizeof(make(x)) - 4]`) | absent after substitution; site 5 is reserved | (asked, low) |
| `explicit(e)` of a function template specialisation | the stored value only; the pattern's own stream walks the written operand | |
| a template candidate that lost to a non-template | no declaration in the parse: Kiln did not instantiate it (the compatibility level of `docs/wp/WP-155.md`) | whether it deduces there |

An adjusted parameter of an instantiated function (`decltype(T::s())
q[2]` is `int *`), named a limit by `docs/wp/WP-153.md` (B7.2), is
closed at the pin: `ParamData::original_ty` keeps the type before
adjustment after substitution too, and the walk reads it
(`tests/differ/_/operands-adjusted-parameter`). So is the written type
of a compound literal of array type (`compound_literal_written`,
`digest-kinds.md`; `tests/differ/_/operands-expr-holders`).

An operand that writes no reference (`noexcept(true)`, `array<char, 4>`)
is not written; neither is a deduced type. An instantiation-dependent
operand is not written as one (its instantiation's is the
specialisation's, `digest-kinds.md` §4.4), but the holders inside it
whose own operands are not instantiation-dependent are, each an entry
(`W2<sizeof(make(x))>` in `decltype(t, W2<sizeof(make(x))>())` of a
pattern's signature or local, or in the expression argument
`W2<sizeof(T) + sizeof(Wr<decltype(make(x))>)>`; WP-156 fix round 2,
X4 to X6). A lambda or statement expression inside such an operand
fails closed (`operands/dependent-body`): its body is not walked for
holders.

Nothing is counted for this limit. The AST holds no fact that says a
satisfaction check or an alias template-id was substituted, and what
it does hold (that a specialisation took a default argument or was
made from a partial specialisation) is true of every specialisation of
a standard container (`spec/landed/digest/WP-153.md` §12). The design
that removes the limit is `spec/landed/digest/WP-153.md`; what it needs
of Kiln is `docs/requests/KR9-substitution-bindings.md`.

**A limit: a friend definition not instantiated** (found by the review
of WP-158, its finding F1). A friend function defined in a class
template, and declared again by another declaration that an
instantiation also makes, is called; Kiln does not instantiate its
definition when the redeclaration instantiated *later* is the one the
call names:

```cpp
struct D;
template <class T> struct B { friend int fr(X u, D*) { return static_cast<T&>(u).mf(1L); } };
template <class T> struct A { friend int fr(X u, D*); };
struct D : B<X>, A<X> {};
int w() { return fr(X(), (D*)nullptr); }
```

Rename `X::mf(long)`: the call in the body keeps the old name and now
binds `X::mf(int)`. No digest differs, the counter
`digest.instantiated.unheld` is 0 (Kiln made no body to count), and the
verdict is `verified`; clang 19 runs the pre-image with status 0 and
the edited tree with status 1. The orders:

| Order | Outcome |
|---|---|
| two classes, the definition's (`B`) instantiated first, the redeclaration's (`A`) second | **wrong `verified`**: no definition, the pair is equal |
| one class, the definition first, the redeclaration after it | Kiln makes no definition either; the digest fails closed: the two friend declarations collide (`digest.collisions 1`), the friend's unit is indeterminate on both sides |
| two classes, the redeclaration's class first (`D : A<X>, B<X>`) | works: the definition is instantiated, `fr(X, D *)::<definition>` differs |
| one class, the redeclaration first | fails closed, as the other one-class order |

The cause, as far as read at Kiln `4a83f6e3c`:
`instantiate_function_definition` (`kiln_sema/src/templ/class.rs:1626`)
starts from `member_redecl_instance` (`:2318`), which returns the
declaration itself whenever it was instantiated from a pattern; the
search for the redeclaration whose pattern has a body (`:2329`) is
reached only for a declaration that was not, so the later friend's
pattern, which has no body, is the one instantiated, and nothing is.
The request to Kiln is in `docs/wp/WP-158.md` ("Fix round 1", "For
Kiln"). Nothing guards it in Slip (Kiln is parked, and the guard the
review sketched would read the same missing definition). The rules
pairs `rules/wp158/friend-two-classes-rev` (equal today: the limit) and
`friend-twice-one-class-rev` (the collision) and the pitfall row R26
pin it; each changes when Kiln instantiates the definition.

### 2.4 Operand units

The digest writes a type by its canonical form and a constant by its
value (§4.2, §7). A name inside an expression that only such a type or
value holds is then written nowhere:

```cpp
namespace lib { struct X {}; int make(X); }   // a migration leaves make behind
template <class T> int make(T);
lib::X x;
decltype(make(x)) y;                           // lib::make, or ::make<lib::X>: `int` both times
template <class T, unsigned long N> struct array {};
array<char, sizeof(make(x))> a;
```

Every unit that has such an expression has an **operand unit**
(WP-156; the design is `spec/landed/digest/WP-153.md` §3.7 as ruled in
its §13.2): key kind `operands`, parent the unit's key (`Parent::Unit`),
name `operands`, no discriminator. It is a derived unit, in the derived
group of its parent's file (none for a parent in the translation unit's
group), whatever its parent is: a declared or derived entity, a
specialisation, a content-keyed unit, a resolved unit (`definition`,
`exception_spec`), a `default_argument` unit. An operand unit has no
operand unit. Its cause is its parent's (§10), and its own.

**Where the operands are read: the holders of a written type.** A
declaration's written type (the return type, each parameter's type
before adjustment, a variable's and a variable template
specialisation's, a field's, a typedef's and an alias's type, an
enumeration's fixed underlying type, a base specifier, a friend's
type); a using-declaration's qualifier; a template head's non-type
parameter types and default type arguments (a template template
parameter's head included); the argument list as written of an
explicit or partial specialisation and of a class's explicit
instantiation; in an expression, the template arguments and the
qualifier of a reference to a declaration, of a member expression, of
a concept-id and of an unresolved or dependent name, and the written
type of a cast, `sizeof`, `alignof`, `typeid`, a type trait, `new`, a
compound literal, `__builtin_offsetof`, `__builtin_va_arg`,
`__builtin_convertvector`, a generic selection, a pseudo-destructor's
destroyed type, a requires-expression's parameters and type
requirements; inside a written type, a nested-name-specifier's types,
and a dependent name's qualifier and arguments. One traversal
destructures every variant of a written type with every field bound
(`operands::slots`, Layer S: each field is a slot that is read, a
denotation, a folded value or plain), and the matches over
declarations and expressions (`operands.rs` `signature`, `holders`)
list every kind by name, so a kind or a field Kiln adds fails to
compile until it is placed; a census (`operands::census::SLOTS`)
gives every stored written type of Kiln's role tables a row. Inside a written type the
**operands** are, by site:

| Site | Operand |
|---|---|
| 1 | `decltype(e)` |
| 2 | `typeof(e)` (`__typeof__`) |
| 3 | reserved: an array bound (a value only at the pin; the limit of §2.3) |
| 4 | an expression template argument |
| 5 | reserved: a pack index (absent after substitution at the pin; §2.3) |
| 6 | `noexcept(e)` of a function type |
| 7 | reserved: `explicit(e)`, which the unit's own stream walks |
| 8 | the written argument list of an alias template specialisation |
| 9 | a written name no stream walked (below): the reference to its entity |
| 0 | what Kiln does not keep, indeterminate for its cause |

The walk of a written type reads only what is written: it stops at what
a name denotes (a typedef's or alias's underlying type, a class), at a
substituted parameter's replacement (type or non-type), and never reads
the type of an expression or a type a placeholder deduced (a variable
declared with `auto` or `decltype(auto)`, and a function with a deduced
return type, have no written type but the placeholder's
type-constraint, `Same<decltype(make(x))> auto`: its initialiser or
body is in its own stream; the type-constraint is read where Kiln keeps
it, a template parameter's or an undeduced placeholder's; where it
does not, its names are site-9 entries and trigger T4's, below). An instantiation-dependent alias
specialisation's list is not written, but an operand in it that is not
instantiation-dependent is, as is one in a dependent name's qualifier
or arguments: no pattern's spelling holds it. A holder met inside an operand gives
its operands as entries of their own, right after the entry that holds
them (`decltype(sizeof(decltype(make(x))))` is two entries). Where a
function has parameter declarations, its parameters' operands are read
from them and not again from its type.

**What is written.** An operand of sites 1, 2, 4 and 6 is written only if
its walk writes a reference to a declared entity or a selection (a
callee, a constructor, a conversion, an operator function, a local
object), or is indeterminate: `noexcept(true)`, `array<char, 4>` and
`decltype(0)` add nothing. A site-8 list is always written: the name
that moved shows only in its canonical arguments (`void_t<typename
T::t>`). An instantiation-dependent operand is not written: a pattern's
own spellings hold it (`digest-kinds.md` §4.4). A unit with no written
operand has no operand unit.

**The stream.** Per declaration of the entity, in declaration order (the
operand unit of a part, and of a unit of no entity's declarations, has
one group): if the declaration has written operands, their `count` and
then each; a declaration with none writes nothing. The declaration the
parent's stream is read from also holds, after its signature's
operands, those the parent's stream met in what it writes in place (its
body, its initialiser, its locals, the members of a local class, a
lambda's parameters), in the walk's order. An entry is `uv` its site,
then the ordinary walk of the expression (§6), or for site 8 the
canonical form of each argument. After the entries, the operand unit's
own-spelling list (`digest-kinds.md` §4.4 "Spellings"). So a transform
that adds or removes a declaration with a written operand changes the
operand unit.

**The parent's state.** An operand is written in its parent's state:
the parent's parameters, locals and local types have the numbers the
parent's numbering gives them (§4.3: the owner's numbering walks its
operand entries after its own stream and before its parts), `this` is
the parent's. A parameter of a function type that no function
declaration holds (`void (*fp)(int a) noexcept(noexcept(f(a)))`) is
written by its depth and index (`Ref::param`). The names digest (§9) of
the entity is fed by the written entries only.

**Site 9: the written names no stream walked** (WP-156 fix round 3,
the ledger; `spec/landed/digest/WP-156.md` §2.7). Every digested parse
records Kiln's written-reference table (Kiln `spec/ast.md` §18): an
entry for each name written in the source, in a macro or not, with
its entity, its role and the declaration that holds it. An entry takes
part when it has an entity, is neither dependent nor unresolved, and
its entity is no namespace or namespace alias. Its unit is the unit
that holds its holder (for a declarator group's shared entry, each
declaration's); one the translation unit holds, or a declaration no
unit holds (a parameter of a function type), belongs to `<order>`.

- **Met**: the entry's name expression (role `Expr`, `Member`,
  `Operator`, `UdSuffix`) was written by a kept stream of its unit (its
  own, or an operand entry that is written): a `DeclRefExpr` or
  `MemberExpr` at the entry's name location with its entity, an
  operator call's callee, a `new` or `delete` expression's operator
  function at its qualified name. Nothing more is written.
- **Deferred**: its name is under a root a part of the unit walks (the
  `noexcept` operand of `exception_spec`, a `default_argument` unit's
  expression); the part decides it at its own end, met by its stream
  or written in its operand unit.
- **Unmet**: written in the unit's operand unit, after its groups, as
  one more group in table order: `uv 9`, then the reference to the
  entity, in the unit's state. No role, spelling or location is
  written.

A name that joins no expression (a type, template or concept name)
cannot be met; it is written, with every entry of its range, where
the unit has one of these **triggers** (each read from the AST, with
no location):

| | Trigger | Entries written |
|---|---|---|
| T1 | the unit is a namespace's or `<order>` | every entry |
| T1b | a class one of whose members its own explicit instantiation names | every entry the class itself holds |
| T2 | a declaration whose qualified declarator-id (`Redecl` entry) has a qualifier component that is not a namespace, a class that is no specialisation or an enumeration (a template-id, an alias or typedef, a `decltype`) | the declaration's entries through the last of its qualifier's |
| T3 | the unit read a value Kiln folded (a `_BitInt` width, a vector's count, an address space) | every entry |
| T4 | a declaration whose type a placeholder deduced, or a unit with a `new`, that holds a concept name | the declaration's entries through its last concept name (for `new`, every entry) |
| T5 | a friend function template specialisation | every entry it holds, and every entry of the befriending class's unit (a parameter written in the friend's declarator is held by a `ParmVar` of the class, linked to no function: KT1 2.9 (f)) |

In a trigger's range a name of the unit is written by its entity, so
a transform that changes which names are written there (a typedef
replaced by its type) changes the operand unit; a rename or a
relocation does not.

**Conservation.** Every entry that takes part is decided by its
unit's end (or its part's). After the walk, an entry no unit decided
(its unit was never written) is counted
(`digest.operands.names.unclaimed`) and makes `<order>`'s operand unit
indeterminate (`operands/unclaimed-name`); an entry deferred to a part
never written is counted apart (`digest.operands.names.undemanded`)
and is no cause. `unclaimed` is 0 on both sides of every fixture of
the digest's tests (§13 item 9).

**Named limits that fail closed:** `kiln/written-refs-not-recorded`
(the table is off or not whole: `<order>`'s operand unit);
`operands/unclaimed-name`; `kiln/friend-form-not-carried` (a friend
read from a module image); `kiln/bit-int-width` (a pattern's
`_BitInt(e)`, whose width Kiln keeps as a profile); `operands/splice`;
`operands/dependent-body`; `operands/limit`; `operands/unwalked-local`
(a site-9 entry naming a local the unit's numbering never met: a
local class declared in a lambda inside an array bound).

**Named limits that stay `equal`**, each with a pinned pair
(`tests/equal/_/limit-operands-*`): a type name alone in an operand
Kiln does not keep, in a slot no trigger names (the class is closed
by Kiln, KT1 2.8); a type name alone in an array bound (§2.3; a name
in a bound's expression is unmet and written); a member template name
after `.template` in an expression no stream walks.

**`noexcept` of a specialisation** (a function whose exception
specification can be deferred, `digest-kinds.md` §11.2): its operand
belongs to the operand unit of its resolved unit `exception_spec` only,
never to the specialisation's own, so a use that resolves the
specification changes no unit both images have.

**A member template of a class made by instantiation.** Its partly
substituted signature is in its own key, so a name that moved in it
would give a unit on each side only. The class's lists write the keys
of its member templates that are derived units, sorted
(`digest-kinds.md` §5.5 `members-other`): for a class whose definition
is its resolved unit `definition` (§2.2), that unit.

**Comparing.** An operand unit is compared as any derived unit: on both
sides, unequal is a `derived_changed` finding of the operand unit (the
report shows each side's operands). On one side only, with a parent
both images have, it is no finding (`spec/verify.md` §4 step 3): the
parent's stream is unchanged by construction, and the operand was
removed with its names or no longer refers to anything.

The counters are in §12; the key in `digest-kinds.md` §3.1 and §3.2;
the sites as a class in `digest-kinds.md` §11.11.

## 3. Keys

A **key** identifies an entity independently of the parse, of source
positions and of the translation unit. It is structural: an entity's key
contains the key of the entity it is a member of.

A key holds names only. That an entity's parent chain passes through an
unnamed namespace, or through a namespace opening with a `visibility`
attribute, is in the key as a name and nothing else: a rule rewrites the
chain (§3.4), and what the chain gives the entity besides names (the
linkage of its name, internal under an unnamed namespace; the visibility
a namespace opening gives its symbol) is written in the entity's own unit,
as the post-image has it, by the row `enclosing` (`digest-kinds.md` §1.7,
WP-113). A rule therefore moves an entity's name and never its linkage or
its visibility.

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
Three kinds of key take another entity's name as the rules map it
(WP-103), so that one rule renames what the source cannot rename apart:

- a specialisation and a partial specialisation are named by their
  template (§3.2): a rule that renames a template renames them with it,
  as their parent, the template's key, already is. No rule matches a
  specialisation itself (`verify.md` §2.1);
- a member instantiated from a pattern member (a member of an implicit or
  explicit instantiation of a class template, a member template's
  instantiation, an explicit specialisation of a member), the link Kiln
  records as `instantiated_from`, is named by the pattern member, through
  the whole chain of patterns (a member of a nested class template's
  instantiation takes its pattern's name, which takes its own pattern's).
  The link is asked of every declaration of the member's redeclaration
  chain, not of its representative only: a friend defined in a class
  template and declared again at namespace scope follows its pattern
  whichever declaration comes first. A friend declaration in a pattern
  that does not define the function (a function defined elsewhere and
  befriended) is not followed: the function keeps its own name and its
  own rule. Two patterns that define one function cannot both be
  instantiated in a valid program (a redefinition), so a chain leads to
  at most one pattern with a definition; were two to give different
  mapped names, the key is indeterminate (`key/patterns-disagree`).
  A member of an explicit specialisation of the class is declared in that
  specialisation, has no such link, and keeps its own name: it needs its
  own rule. No source renames an instantiation's member apart from its
  pattern's, so a rule on an instantiated member is accepted only where
  it says what the pattern says (WP-119): a rule that matches the
  member's **plain** key (its own name, parent and discriminator mapped)
  and produces exactly the key following gives (the patterns' mapped
  name, with the plain key's parent, kind and discriminator) gives the
  member that key, and the member is counted in that rule's `via`
  (`verify.md` §9.1); a rule on the followed key is then not applied
  after it (a table maps a key once). Any other rule on the plain key
  (another name, or the pattern's name under another parent), and a rule
  that matches the followed key when none matches the plain key, makes
  the key indeterminate (`key/rule-on-instantiated-member`). Such a rule
  is how a function defined as a friend in a class template and declared
  at namespace scope is renamed: a unit that does not instantiate the
  class keys the function by its own declaration, which needs the rule,
  and a unit that does accepts the same rule.
  A member follows its patterns only where its own name is the
  pattern's by construction, so that following gives the member its own
  key under every table that renames nothing of the pattern (WP-131): an
  identifier, an operator, `constructor`, `destructor`, a literal
  operator, `deduction_guide`, and `unnamed_by` of one of those, are
  written in the pattern and not substituted. A member whose name is a
  **conversion** name does not take its pattern's name: the name is the
  type converted to, which is the instantiation's (`W<bool>::operator
  bool()`, instantiated from `W::operator T()`), so the pattern's name is
  another name. No rule renames a conversion function apart from its
  type, and the type digest in the member's own name is already in mapped
  terms: a rule on the type it converts to reaches it there. The same
  holds of `unnamed_by` of a conversion name (a class whose declarator is
  name-independent, named by its first member, a conversion function,
  P2169), of a `local` number (it counts the instantiation's own local
  types: a discarded `if constexpr` branch of the instantiation declares
  none, and one expansion statement instantiates its local class once
  per element, each a distinct entity) and of an `unnamed_ordinal` (it counts the scope's own unnamed
  types). Such a member is keyed as an entity that follows no pattern:
  a rule that matches its plain key applies to it as to any key, and the
  checks above are not made. A rule whose name is a conversion name comes
  from one declared change, a `relocate` of a conversion function with
  `=` (`slip declare` gives a new name only to an identifier): it moves
  that conversion function, and matches its key, as for any entity. A
  declared relocation of a conversion function with `=` inside its own
  class is an identity line, which `slip declare` refuses (`verify.md`
  §2.1, "One way to each key"; the fixture
  `rules/inert/conversion-rule-on-instantiated-member` shows the
  refusal);
- a decomposition is named by its first binding (digest-kinds §5.2),
  as the rules map the binding.

Every other name made from another entity's holds the plain name, as
written in the post-image: `unnamed_by` (an unnamed class or enumeration
named by a typedef or a declarator, `verify.md` §2.1), an unnamed field
or variable named by its anonymous class, and a using-shadow, which has
the name it is written with (its discriminator, the key of the entity it
stands for, is mapped as any key is).
With `R` empty this is the identity, which is what the map phase uses.

**A rule that matches nothing changes nothing** (WP-131). For every
translation unit, the walk under a table none of whose rules matches a
key of that translation unit (a rule's `(parent, kind, name, disc)` is no
entity's, its parent and discriminator mapped) gives the units, keys,
digests, flags, causes, names and counters of the walk under the empty
table, whichever files are read-only; and, when no rule produces a key
of that translation unit either (`verify.md` §9.1: an entity whose key
is the key a rule produces counts `direct` for it, matched or not), no
rule has a use (`via` and `direct` 0). The exception is `digest.bytes` (§12),
which measures work: a walk under a non-empty table keys a namespace in
the view of a member of the other ownership and computes a pattern's key
again for its mapped name, and counts those bytes, whether or not a rule
matches; it is never smaller. Every route the walk takes only under a
non-empty table (following a pattern, a mapped name, a namespace keyed in
a member's view, the collisions among mapped keys, the ownership of an
entity) must therefore give the answer of the empty table when no rule
applies. A table is no reason for a key to differ; only a rule that
matches is. `crates/slip_digest/tests/inert.rs` holds it over every
source of the crate's pair and rule tests.
Verification digests the post-image with the rules that undo the declared
relocations, so that a relocated entity, every member of it, every type
that mentions it and every signature that contains it get the keys they had
before, and the two digests are compared for plain equality. Where the
rules come from, and why this is sound, is `verify.md`. A walk under
rules also counts how each rule was used, so that a run can check that no
observer still has a renamed entity under its old name (`verify.md` §9).

**Ownership.** With rules the walk is also given which files are
read-only (`verify.md` §2, `Declared.files`; `Options::read_only`, a
predicate over the image's files that the caller makes: every file
read-only when the caller has none, so that every key is the key of the
empty table). A rule matches only an
**owned** entity, and the namespaces that enclose a namespace member are
mapped only when the member is owned; a dependency entity (one with a
declaration in a read-only file, by its expansion location or its name
token's spelling location, a file read-only per inclusion when its
include chain passes through a read-only file; a
specialisation of such a template; an implicit member of such a class)
keeps its own name and its namespaces theirs, with only what its
discriminators mention mapped. The definition and the reason are
`verify.md` §2.1 "Rules rename owned entities only". A file may also be
**rebuilt** (`verify.md` §2.1, WP-118): for everything in this section
it is read as an owned file, except that an entity with C language
linkage declared in one is a dependency entity. "Map its parent"
above is therefore: a parent that is not a namespace has its own key; a
namespace parent is mapped from the point of view of the entity being
keyed: the same namespace can have two keys in one walk, one for its
owned members and one for its dependency members, and its own unit has
the key of its own ownership.

**A namespace's unit in each view** (WP-140). A namespace is owned iff
none of its openings lies in a read-only file (`verify.md` §2.1); its
own unit is keyed in that view. It has a unit in the **other** view too
when that view exists. **A view of a namespace exists iff a member of
that ownership exists in it**, where a member is (a) an entity whose
semantic parent is the namespace, declared by the source or made by the
compiler (`std::__va_list`, the implicit `std::align_val_t`); an entity
with C language linkage, which is keyed under `c_linkage`, is a member
of every namespace one of its declarations is in, through linkage
specifications, as lookup finds it there (WP-141 second review F1), (b) a
written content-keyed declaration (using-directive, namespace alias,
using-declaration, `static_assert`), or (c) a nested namespace, named,
inline or unnamed, one of whose views of that ownership exists. A
read-only opening holds in the dependency view even when it is empty
(Addendum A4: what a dependency opens, lookup finds); an empty owned
opening is nothing (what a transform leaves behind); a declaration the
compiler makes that is no entity (the using-directive an unnamed namespace implies) is nothing.
The predicate is decided from the declarations of the image, never from
what the walk keyed (`Walk::ns_holds`, one function for this unit and
for `E(k)` below; WP-141). The **owned view** applies the rules along the chain; the
**dependency view** applies none. So an empty owned opening, one that holds only empty nested namespaces, or one
that holds only an explicit specialisation (keyed under its template, not
under the namespace) holds nothing (WP-140 review F3, the definition of
`docs/wp/WP-141.md` Addendum A4; fixtures `tests/rules/views/untouched-*`).
The unit in the other view is written only when its key differs from the
namespace's own key, so that a table that maps nothing adds none (and the
dedup below is not relied on). It has the namespace's **one digest**, the
digest of its own unit (every opening: the `inline` bit and the
attributes of the whole chain), and the own unit's cause. The digest is
not per view: a fact that one view's openings hold (an `inline` written
only by the dependency, a `[[deprecated]]` added by an owned opening)
changes what the other view's members mean
(`spec/landed/digest/WP-140.md`, why option A, a digest per view, was
rejected; fixtures `tests/rules/views/inline-reopened-plain`,
`deprecated-owned-opening`, `deprecated-dependency-opening`). Its cost:
an attribute that applies only to the declarations of its own opening
(`visibility`) changes both views' units, and fails closed
(`views/visibility-owned-opening`). So a move into a namespace that a
dependency also opens compares the owned view's unit with the pre-image's
namespace (`verify.md` §2.1). A view unit is **named** by the name its
key denotes and the namespace it is a part of: `legacy (the owned part of
acme)`, `acme (the dependency's part of acme)` (review F4), in findings,
record errors and `slip units`, and `slip declare` takes that name.
**Units are only added**: no key, digest or cause of another unit moves
(no member's key, and not the own unit's), and no pre-image (digested
under the empty table) has a unit in a view. The view key's cause and a
view key that cannot be formed are guards: a namespace key's cause and
errors are made of its names' and its parents', the same namespaces' in
either view, so the view key's are the own key's, which the own unit
already has (`units::view_units`). A **reference** to a namespace is
written as the next paragraph says (WP-141).

**A reference to a namespace** (WP-141, build A). A namespace is
referred to by a view key. With one view key there is nothing to
choose, and the reference is `Ref::unit` of the own key, as anywhere.

*Named to find members*: the scope of a using-declaration in its key,
the qualifier of a name left unresolved when the qualifier is a
namespace, in a stream or in a profile, and the qualifier of a qualified
concept-id. Take the declarations the lookup found (the
using-declaration's shadows, each as the entity it stands for; the
unresolved name's set; the concept). Each gives the key of the part of
the named namespace that holds it, by its **route**, in this order:

1. a member of the named namespace or of its inline namespace set: its
   own view (the view of its ownership);
2. else, reached through using-directives written in the named
   namespace or its inline set, followed transitively as
   [namespace.qual] follows them: the view of each such directive (the
   ownership of the file it is written in), both keys if both;
3. else the route is not known (it came through a using-declaration,
   whose shadow Kiln does not keep in a found set): every view key, and
   the counter `digest.ref.namespace_route_unknown` counts it. The
   shadow is never guessed by searching the namespace.

The keys are sorted and duplicates removed. One key is written as
`Ref::unit`. Two or more, or nothing found (then every view key, and
`digest.ref.namespace_no_referent` counts it), are written as
`Ref::views`: the count and the keys. The route is asked only when the
named namespace has two view keys.

*Named whole*: the namespace a using-directive at namespace scope
nominates, and the target of a namespace alias. In a read-only file
(by the path of the file the declaration is written in) it is written
with its key in the dependency view (its one view key if it has no
dependency part), never `Ref::part`. In an owned file it has a
**reading** for each of its view keys `k`. A namespace **holds
something under `k`** if a view of it whose key is `k` exists, by the
definition of the paragraph above (the same predicate). `E(k)` is the set of namespaces
of the image that hold something under `k`. The reference is **whole
for `k`** if every namespace of `E(k)` is named: by this alias; or, for
a using-directive, by the using-directives of the same enclosing
namespace **entity** in the same file, taken together (never by those of
another namespace that has the same mapped key). The reading for `k` is
`Ref::unit(k)` if the reference is whole for `k`, else `Ref::part(k,
view)`, `view` the ownership of the named namespace's own unit. The
unit is written with the reading of the namespace's own key. A
directive or an alias at block scope is written as before. *(The other
readings, the alternates of `verify.md` §4 step 1a, are WP-143's and
not built: a unit has no alternate, and §11 has no `tu_alt`.)*

The guard: R1 asks the route only when the named namespace has two view
keys; R2 writes a reference plain iff the named namespace has one view
key `k` and `E(k)` is that namespace alone (Addendum 3). Under the empty
table or a table that maps no namespace of the chain, no namespace has
two view keys and no key is held by two namespace entities, so every
reference is `Ref::unit` of the own key, written exactly as before
WP-141: no digest moves. One exception fails closed: `E(k)` carries a
cause for the whole image when any namespace's view key cannot be formed
(a depth or a cycle, which no source reaches today), and then every R2
reference carries it where the empty table gives none (second review F4). The table of namespaces (`E(k)`, the inline sets, the
directives written in each) is made in one pass over the image before
the walk reads it, so nothing depends on the order units are written in.
`Ref::views` and `Ref::part` are never written for a pre-image.

Each owned using-directive and alias whose reading for a view key is
not whole is reported, informational only (`Output::namespace_parts`):
the unit, the key, whether it is the written reading, and the
namespaces of `E(k)` it misses with the file of each one's first
opening. The display name of a using-directive whose written reading is
`Ref::part` says which part it names (`verify.md` §2.1; the words are
provisional).

Every site that writes a key (a unit's
own, a reference from another unit's stream, a parent, a
specialisation's template name) takes it through the one mapping, so an
entity and every reference to it agree (WP-110). With `R` empty none of
this is consulted.

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
A part of a unit (a resolved or `default_argument` unit, `digest-kinds.md`
§11.1, §11.4) is keyed under its owner's key and follows its owner's key
cause: every part of an owner whose key collides is indeterminate with
it (`Collision`), whether or not another part shares its key, and parts
of two colliding owners that share a key are the owners' collision,
counted once (§12), whichever of them uses made the parts.
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
     | views = 6, count: uv, ids: KeyId*          a namespace named to find members under several view keys (§3.4), ascending
     | part = 7, id: KeyId, view: bool            a namespace named whole, not whole for its own key (§3.4)
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
  (§1 rule 5). A specialisation is one whatever made Kiln instantiate it
  (§2.2 "Every instantiation has a unit"): a call, a conversion of the
  closure to a function pointer, an unevaluated operand (WP-158).
- An instantiated default argument of a local function that is no unit
  (a local class's member, the call operator of a lambda that is not
  generic, in an instantiated body) is a `default_argument` unit
  (`digest-kinds.md` §11.4), keyed `parameter(i)` under the function's
  key (the members of a local type have ordinary keys), derived, origin
  file the function's (WP-158). The function's own stream, written in
  place, writes the parameter's default as absent, as for every
  instantiated declaration; without the unit the expression would be in
  no stream.
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
and no reference, and writes nothing. The walk goes on into the streams
of the owner's units that hold what its own stream leaves out, each
recording only what was not met before: the resolved unit `definition`
of an owner of the definition class (`digest-kinds.md` §11.9: the body
of an instantiated function, the initialiser of an instantiated
variable), whose parameters the own stream met first; then the
`default_argument` unit of a field an instantiation made (§11.4). The
owner of the variable of each expanded expansion statement is numbered
too: Kiln keeps the statement's pattern parts in an instantiated body,
so a specialisation's `definition` names its pattern's variable, by
`Ref::outer`. So is the owner of the variable a lifetime-extended
temporary extends, when that owner is a template's pattern or a member
of one: Kiln shares a non-dependent temporary of a pattern's body with
its instantiations (a range-`for` over a prvalue: the temporary's
`LetData` `extending` is the pattern's `__range` variable), so the
specialisation's `definition` names that variable by `Ref::outer` too
(WP-123). So the numbers cannot depend on
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

**Names a program reads.** A local entity has no key that a rule can
map, and no spelled-entity record can report it, so a rename of a
local type is invisible to the digest unless the stream writes the
name where the program reads it. The symbolic form of a value derived
from a name therefore includes the written names of the local types it
would print:

- A function name (`__func__`, `__FUNCTION__`, `__PRETTY_FUNCTION__`,
  and `__builtin_FUNCTION()` or `__builtin_source_location()`, a
  `SourceLocExpr` of the function-name kinds) lexically inside a member
  of a named local class, at any depth (through nested local classes
  and lambdas), writes the names of the named local types around it, up
  to the owner, outermost first. A unit under a local type (a generic
  lambda's specialisation) writes those around its entity; a stream
  that writes a local type in place, those it is inside.
- A `typeid` whose operand mentions a named local type (the type
  operand, or the static type of an expression operand, through
  pointers, references, arrays, member pointers, function types and
  template arguments) writes, after the operand, the names of those
  types with the named local types around each, in the order met.
- An unnamed local type or a closure has no written name: nothing is
  written for it. A rename that nothing reads stays equal.
- A reflection of a local entity can read its name; every reflection
  is indeterminate today, and the package that writes reflections owes
  the same rule.
- A template instantiated with a local type prints it too (`f<X>` in a
  specialisation's `__PRETTY_FUNCTION__`); its instantiated body, the
  resolved unit `definition` since WP-045, is held to the same rule,
  which no pair test shows yet.

**An owner keyed by its content.** A `consteval` block is the call of
a lambda (P3289R1) that Kiln gives the block's scope as owner; its
closure is owned by the block, whose key is its own content digest
(§3.3). Inside that digest the key of the block is not yet known: the
local types under it have the parent `SELF_KEY`, which can only mean
the unit being digested, and nothing computed with it is memoised for
another unit.

**What these rules do not yet show.**

- "In an instantiated body the specialisation" holds since WP-045: the
  instantiated body is the resolved unit `definition`, which numbering
  the specialisation walks (the implicit constructor of a local class of
  a function template's specialisation, a closure an instantiated body
  passes to another template, a lambda in a variable template's
  initialiser, a lambda returned from a generic lambda's body:
  `tests/differ/_/inst-*`). A lambda in the default argument of an
  instantiated function is in that function's `default_argument` unit,
  which numbering does not walk: such a closure has no number.
- Kiln's owner of a lambda in the default argument of a lambda's
  parameter is the function whose body holds the outer lambda, not the
  call operator whose stream walks the default: the numbering of that
  function enters the call operator's parameters' default arguments
  (and nothing else of its members) so that it finds the closure.
- A local class's other members (`members-other`, digest-kinds §5.6)
  are written sorted by key, as a namespace-scope class's are: their
  order in the class is not semantic, and their keys under the local
  type are as stable as the type's number.

- A value computed once and used at several places (Kiln's opaque values)
  is numbered in the same way as a local object, in a sequence of its
  own: the first use writes the number and the expression, later uses
  the number.

**Back-references.** Kiln's AST is a graph in places: one statement or
expression node is the child of two parents, or is reached through the
semantic form and again through the form as written. In one stream the
statement and expression nodes are counted in the order they are first
written, in one sequence for both, from 0 at the head of the stream. A
node that the walk reaches again in the same stream and **in the same
state** is written as `backref` (`digest-kinds.md` §1.7): its tag and
`uv` the count of its first writing; its rows and children are not
written again. The state is every fact of the walk other than the node
that a row reads: whether an instantiation-dependent expression encloses
the node, and whether a key is being computed
(`spec/landed/digest-kinds/WP-049.md` "The state" lists the rows that
read each); a node reached in another state is written in full and
counted again. A first writing carries no number, so the stream of a
unit in which no node is reached twice is what it is without the rule.

- Opaque values come first: an `OpaqueValueExpr`, and a node numbered as
  an opaque value's source when the walk reaches it, are written by the
  rule above, are not counted, and are never written as `backref`.
- Nodes walked while a canonical operand is written (profile mode,
  `digest-kinds.md` §4.4) are not counted and never written as
  `backref`. A requires-expression inside a key is also written in
  profile mode (it numbers its own parameters), so its nodes are not
  counted either.
- A back-reference names a node of its own stream only. A type digest, a
  key, a discriminator, a content digest, a unit and each element of a
  `*sorted` list (encoded alone before the list is sorted) is a stream
  of its own, enclosing or enclosed: none names a node another wrote.

Why it cannot hide a difference: two streams are equal only if every
first writing is equal and every back-reference names the same count,
that is, the same node shared at the same places in the same state, so
the node means at its second place what its first writing says. Sharing
is a fact of the AST that Kiln builds deterministically from the
translation unit: a change that turns a shared node into two equal nodes,
or the reverse, changes the stream, a difference where the code
generated is the same (allowed, §1 rule 6), which the shapes the tests
hold do not show when only names change.

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
  key); a back-reference, besides, never names a node of another
  stream (§4.3 "Back-references");
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
  belongs to, and every attribute Kiln stores are included. Linkage is
  the linkage of the name, not only the storage class written: for an
  entity at namespace scope it, whether an unnamed namespace encloses it,
  and the visibility its enclosing namespace openings give each of its
  declarations are the row `enclosing` (`digest-kinds.md` §1.7); an
  entity in a class or a function takes them from that class or function,
  whose unit holds the row.

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
  instantiation. Its explicit template arguments are written too, and an
  expression among them that no template was chosen to convert it for
  (`get<n::k>(t)`) is the pattern's own expression, walked in full as
  every other (§6; `digest-kinds.md` §4.6 `ArgView` `Expr`, WP-123), so
  what it names is compared under the rules.
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
are written **symbolically**: as the builtin and the entity they are of,
not as their value. So, by the rules of the second list, are literals that
come from the expansion of `__LINE__`, `__FILE__`, `__COUNTER__`,
`__DATE__`, `__TIME__` and their relatives (not yet: Kiln does not say
that a literal came from one, `digest-kinds.md` "Literals from builtin
macros"; they are written by value).

**What a name-derived string's digest holds.** Where such a string is only
passed on, its content is not part of the unit's digest, whatever changes
it: the declared rename, a requalification, a respelling of the enclosing
function's signature. Where the program reads it at compile time, the
content is written by value (below). `__PRETTY_FUNCTION__` and
`std::source_location::function_name()` print more than names (the
enclosing function's parameter and return types as written, an alias, an
elaborated keyword, a qualifier as written, template parameters' names;
Kiln's `kiln_ast/src/dump/pretty.rs` `pretty_function`): that is content,
and equal in the unit digest by this rule. A second, informational digest
of the bytes of every name-derived string a unit holds symbolically is
compared by the verifier and reported ("The names digest", below).
`__func__`, `__FUNCTION__` and
`__builtin_FUNCTION()` hold the unqualified name only; a conversion
function's names its canonical type.

**A symbolic value has the identity of the object it stands for**: two are
written alike only if they are the same string, or the same
`std::source_location::__impl` object, of the same entity. A string of a
`PredefinedExpr` is written as its `PredefinedKind` and the function whose
name it holds, as any reference to that entity is written (so a rule maps
it); a function name of the source-location builtins as plain or pretty
and its context; an `__impl` object as the function of the first use that
built it, that use's ordinal among the function's uses that build one (every
use counted, also one whose object an earlier use of the same file,
function, line and column built), and the bytes of the file
name it points to, written as a `__FILE__` literal's bytes are, so the two
cannot disagree (its line and column are dropped; its file is not). The
function a `PredefinedExpr`'s string belongs to is looked up in whole
definitions: the body, a function-try-block's handlers, a constructor's
mem-initializers, and through the `#include` of a body that comes from
an included file.

The uses of a source-location string are taken **by content**: every use
of a string of these bytes, so the `"get"` of `a::get()` and of
`b::get()` is one string (Kiln made one object per content until Kiln
WP-3000, `50ee80489`; it makes one per content, kind and declaration
since, in the pin since WP-163, and the rule reads the content as
before). Such a string is written as the context of its uses only where
**every use** of a string of its bytes in the translation unit is of one
kind and in **one entity** (a lambda or
a local class of a function is another entity) whose name of that kind
the bytes are; otherwise by its bytes. **What equal means for a name
string**: two are equal when their bytes are equal, or when their bytes
differ exactly as a rule says the name of one certain entity changed; the
symbolic form is the permission to follow a rename, written only where
the string is certainly that entity's. So a function of the same name
gaining or losing a use of the string moves every holder of it between
the entity's form and the bytes: a cost
(`spec/landed/digest/WP-115.md`, item 1). An `__impl` object built **outside
a function** (a namespace-scope initializer, a default member
initializer used by an aggregate initialization there, a default
argument evaluated there, a variable template's specialisation, a
lambda's init capture) is numbered, by its first use, among the uses of
the **declaration that holds it** (the variable or field whose
initializer holds the use, or the function whose parameter's default
argument does), in its own order, every use counted, written with that
declaration; only a use no such declaration holds (an enumerator, a
bound) is numbered among every such use of the translation unit. A pretty name's identity also holds the names of the
named local types and block-scope aliases its printed signature
mentions, which no rule maps. A string is written so
only where its bytes are what Kiln prints for that entity of that kind;
anywhere else, and for a file name (which no declared rename changes), its
bytes are written. A pointer into such a string keeps its offset by value.
The local scope names of the entity (WP-040's `write_scope_names`) are part
of a name string's identity, in both modes. So when a computation at
compile time consumes a name or a line and returns one of two strings or
objects, which one it returned is in the digest (`digest-kinds.md`
`PtrBase`).

The expression's **own type** is symbolic with it where the value is
passed on. The array type of `__func__` and its kin (`const char[N]`, `N`
the length of a name) is written as that builtin's type, without the
bound, where the array is converted to a pointer, which passes the value
on and nothing of its length; anywhere else the array itself is read and
its type is written by value (`digest-kinds.md` §4.2 rule 8).

Whatever **consumes** such a value is written by value as always, so a
real change is seen: an array bound from `__LINE__`, `sizeof(__func__)`
and every other operator that reads the array's type (`alignof`,
`decltype`, `typeid`, a type trait, a range-`for` over it), the length of
a name deduced as a template argument, a variable declared with the
array's type or a reference to it, a constant computed from the
characters. A constant computed from such a value is written by value
where it is written: as a template argument, an array bound, an
enumerator, a variable's value, a `constexpr` function's result used as
any of these. Where a constant is computed but not stored, what it reads
is written by value: in a `static_assert` condition and in the operand of
a constant expression whose value is not stored. So it is in a key and in
a type, which name one entity of the parse whoever asks. A block-scope
variable has no `value` unit and the function's uses read its value
without writing it (`digest-kinds.md` §11.3): its value is written with
its declaration in the function's mode (a pointer to a name passes it on),
and a use read by value (in a `static_assert` condition, a key, a type)
writes the variable's value by value.

Why the type goes with the value: a declared relocation says that names
change, and a value that is a function of a name changes by exactly that,
as a mangled name does. `assert` expands to `__PRETTY_FUNCTION__`; were
its length a fact, no function that asserts could be relocated and
verify. A program that observes such a string (a log that is parsed, a
registry keyed by it) is told by the report, not by the verdict
(`verify.md` §4.4, §7.1).

**The names digest** (WP-112). Beside its digest, a unit has a **names
digest**: `H("slip.names.v1", entries)` of the bytes of the name-derived
strings it writes symbolically, as Kiln evaluated them, in the order the
walk writes them. A unit that writes none has no names digest (not the
digest of nothing). The entries, each after what the symbolic form holds,
so that two strings swapped are seen:

| Entry | Fed where |
|---|---|
| `uv 1`, the `PredefinedKind` (0 `__func__`, 1 `__FUNCTION__`, 2 `__PRETTY_FUNCTION__`), `bytes` | a `PredefinedExpr` written symbolically, or a string a pointer names that is one |
| `uv 2`, `uv 0` plain or `uv 1` pretty, `bytes` | a source-location function name a pointer names; a `__builtin_FUNCTION()` (plain), also at a use of a default that passes it on (below) |
| `uv 3`, `bool` has one, `bytes` | the function name an `__impl` object points to (what `function_name()` returns), from a pointer to the object or a `__builtin_source_location()`, also at a use of a default that passes it on (below) |
| `uv 4`, `digest` | the names digest of a constant a pointer names, where the walk writes the constant by its content digest (the constant's own entries, memoised with it) |

An entry is fed only where the value is written symbolically: never where
it is written by value (`by_value` raised: a key, a type, a
`static_assert` condition, a template argument, a constant computed and
read), since the unit's digest already holds those bytes; never while an
owner is being numbered (§11's numbering streams are not the unit's) nor
for a candidate stream of `canonical_decl` (the chosen one is written
again). An operator that reads only the array's type (`sizeof(__func__)`,
`alignof`, `decltype`) writes the length by value and leaves the string
symbolic, so the string is fed too: the unit differs and notes. A
`std::source_location`'s `function_name()` is fed; its `line()`
and `column()` are not (any edit above moves them: `verify.md` §8), and its
`file_name()` is not, because it is written by value (above). A string
reached twice is fed each time it is written: two uses of one string
object, or two uses of one default, feed two entries; a back-reference
(§4.3) writes no string and feeds nothing. A unit resolved into another (an inline body, an
instantiation's `<definition>`, §6) feeds the entity's unit: the strings
of an instantiation's body are its key's (`h<int>(int, int)`), each
instantiation its own; a pattern's dependent string has no bytes (Kiln
evaluates none) and feeds nothing. `typeid(T).name()` is not a constant
in Kiln (the call is written as a call): nothing to feed. `__FILE__` and
its kin are literals written by value.

**Defaults at a use** (WP-112, R2). A default argument or default member
initializer is written once, with its declaration (§7 "Shared nodes"), but
the source-location builtins in it are evaluated at each use
(`CXXDefaultArgExpr`, `CXXDefaultInitExpr`), as the function the use is
in. The principle: a use contributes the value of the immediate
invocations evaluated at the use, written as every constant is written.
Kiln does not evaluate them per use (it holds one value of each, `default_imm`,
evaluated in the declaration's context, contrary to CWG 2631), so the
digest does not infer from that value whether the use's inputs are passed
on. At each use, for each `__builtin_FUNCTION()` and
`__builtin_source_location()` its default reaches, through nested defaults
(a default argument inside a default argument or a default member
initializer, the rebuilt default where Kiln rebuilt one for the use), in
walk order, after the use's rows:

- (a) one that is the operand of no immediate invocation is passed on: the
  use's value (Kiln's `source_loc` of the use: the plain function name, or
  the `__impl` object's function name) is fed, as at a `SourceLocExpr`,
  and the local scope names are written once after them (as there).
  Nothing by value.
- (b) one that is the argument of `std::source_location::current`, and of
  no other immediate invocation, is passed on, as (a). `current` is
  identified by the entity: the static member function `current` of the
  class `source_location` of namespace `std`, inline namespaces skipped,
  every argument absent (its default) or itself a source-location builtin.
  Not `std::experimental`'s, nor a derived or wrapping class's, nor a user's
  `mine::source_location::current`.
- (c) one that is an operand of any other immediate invocation is
  consumed: the use's value is written by value in the unit that holds the
  use (`bool` pretty, `opt`, `bytes`: the plain function name, or the
  `__impl` object's function name where one is built; never the file, the
  line or the column), cause-free (the unit stays determinate), and nothing
  is fed for it. Where the use reaches a builtin of (a), (b) or (c), its
  rows are followed by `count`, the number of (c), and then theirs. (c)
  over-approximates: an immediate function that passes the name on (a
  user's `id(p)`, `mine::source_location::current`) makes the use's unit
  differ under a rename that changes the name. Those false findings go
  away when Kiln evaluates the invocations per use (`spec/landed/verify/WP-112.md`
  "For Kiln").
- (d) a use whose default reaches none of these, or where Kiln recorded no
  values at the use, writes nothing more. Nor does a use that is itself
  inside a `ConstantExpr` (`current()` written in a body, `pick()` an
  argument of an immediate invocation there): that invocation is evaluated
  where it is written, its value is written as every constant is (a name
  it passes on fed with it), and the use's inputs are its operands. A
  nested use met where the default itself is written, with its
  declaration, is judged by these rules with the values Kiln recorded for
  it there (the declaration's context), as a `SourceLocExpr` written there
  is.
- A body written in place inside an expression (a closure type's members
  at its lambda-expression, the members of a local class declared in such
  a body or anywhere else: whatever `local_type` writes) is an evaluation
  context of its own, also inside a `ConstantExpr`: a use in it is judged
  by (a) to (d) as in any body, not as an operand of the invocation
  around the lambda-expression (`pass([] { log(); })`, a consteval
  constructor taking `+[] { log(); }`). The reason: the body runs where it
  is called, not where the constant is evaluated, and the constant's value
  is the closure, not a name. Where the constant's value also holds a name
  the body passes on, feeding it is at worst a note too many, never a
  wrong equal.
- A use the walk writes by value (`by_value` raised, above: inside a
  constant the walk does not see the result of, such as an immediate
  invocation in a default argument, which Kiln evaluates per use and has
  no value for at the declaration) writes every builtin it reaches as (c),
  fed for none, as a `SourceLocExpr` there is written by value (WP-115's
  known cost C1).
- What a default reaches does not depend on the use: it is computed once
  per translation unit for each default expression (the shared one, and
  each rebuilt one) and memoised; a nested default is composed from its
  own answer. A default met again while it is being walked (a default
  that reaches itself, which is ill-formed: "recursive evaluation of
  default argument") contributes nothing the second time, and an answer
  that met one is not memoised, so every answer is the same wherever the
  walk began. The walk is bounded by the defaults' sizes. A default whose
  own expression has more than 65,536 nodes, or a nesting of more than
  65,536 defaults, makes the unit holding the use indeterminate,
  `Unsupported("default/reach-limit")` (§10): the walk never gives up
  silently.

A `__func__` or `__PRETTY_FUNCTION__` in a default is none of these: Kiln
records no value of it at a use (its value is its declaration context's,
`""` outside a function), and it is written, and fed, where the default is.

Per key (`model.md` §8.7): a key with one unit that has a names digest has
it; a key with several has `H("slip.names.key.v1", count, digests)` of
them in its units' order. Its file and name are its first such unit's.

The names digest is never part of a unit's digest, a group's or `tu`:
no unit digest changes by it. Its mutations and the cases that pin each
decision are in `spec/landed/verify/WP-112.md`.

## 10. Indeterminate units

A unit is **indeterminate** when its digest cannot be trusted to mean what
this spec says:

- it contains an invalid declaration, an error-recovery expression or a
  type that contains an error;
- it contains a kind or a value that `digest-kinds.md` marks `unsupported`;
- it holds a type whose sugar meets a typedef whose declarations do not
  all carry the same alignment, or it is that typedef
  (`Unsupported("typedef/alignment-by-declaration")`, `digest-kinds.md`
  §4.2 rule 9: Kiln's `alignof` and its cached layout read different
  declarations);
- it holds a constructor definition whose mem-initializers the AST does
  not hold: one Kiln did not check because its class or the definition
  itself is in a template (`DEPENDENT_CTX`) or it is invalid, with a
  ctor-initializer or without, since the AST does not say which
  (`Unsupported("kiln/pattern-mem-initializers")`,
  `spec/landed/digest/WP-124.md`); an instantiation's constructor holds
  its own list and is not one. A local class is in the unit of what
  holds it, so the unit is the constructor's own only for a class
  member; for a local class it is the holder's: a function or member
  function (its body, or a lambda in it), a friend's body, a data
  member (a lambda in its default member initialiser), a member function
  (a lambda in a default argument), a class template (a lambda in a
  default template argument), a concept, a variable template, and a
  function that is not a template whose expansion statement holds one;
- its key collides with another entity's (§3.4);
- it holds demand-driven state that `digest-kinds.md` could not assign to a
  derived unit;
- its entity's declarations are given different visibilities by the
  namespace openings around them (`digest-kinds.md` §1.7 items 4-5), so a
  symbol's visibility depends on where each use is
  (`visibility_by_use`, WP-113);
- it holds a use of a default too large for the walk to follow
  (`Unsupported("default/reach-limit")`, §9 "Defaults at a use").

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
Digests := scheme: uv              3; changes whenever any stream, key or table of this spec changes (rule below)
           tu:     digest          H("slip.tu.v1", list<GroupSum>), groups in the order below
           groups: list<Group>     sorted by (class, file)

Group   := class:  GroupClass
           file:   bytes           the subject path of a file; empty for class `tu`
           count:  uv              units in the group
           indeterminate: uv       units in the group with the indeterminate flag
           digest: digest          H("slip.group.v1", list<Unit>) over the group's units,
                                   each unit's flags without bit 1
           units:  opt<list<Unit>> sorted by (key, digest); present when detail was asked for

GroupSum   := class, file, count, indeterminate, digest
GroupClass := declared = 0 | derived = 1 | tu = 2

Unit    := key: digest, digest: digest, flags: uv      flags bit 0: indeterminate; bit 1: emitted
                                                         unreferenced (WP-155, below); other bits must be 0
```

From the first release, `scheme` changes with every change to a stream,
a key or a table of this spec, without exception. Before that release no
shard outlives the build that wrote it (shards are stored by plan, and
the cache keys of `spec/exec.md` that will replace the plan name the
tool's version), so no shard of one scheme meets a reader of another, and
`scheme` changes before then only when a change makes units differ that
compared equal under some rule table (a wrong equal closed): a recorded
baseline of the earlier scheme must then be recorded again. Such changes
that land before any baseline of the current scheme is recorded share its
number: the number exists to void recorded baselines, and there is none
to void. It is 2 since WP-113 (the row `enclosing`, `spec/digest-kinds.md`
§1.7); WP-107's rule 9 (an aligned typedef is written as sugar,
`spec/digest-kinds.md` §4.2) landed in it the same day, before a baseline
of scheme 2 was recorded. WP-140 (a namespace's unit in each view,
§3.4) changes no stream, key or table row and no existing unit: it adds,
to a post-image digested under a table that maps a namespace's chain, a
unit for each namespace that holds something in its other view. Which
comparisons that can change, and how: the pre-image's namespace unit
that had no partner (`removed legacy`) gets one, so that difference
becomes equal when the digests agree, or `changed` when they do not (an
attribute or `inline` of the namespace's openings); a view unit with no
partner is `added`, which happens only in an observer that already
differs (an owned entity keyed under it has no partner either: a
pre-image entity under `legacy` implies a unit `legacy`); a view unit
whose key is another unit's with a different digest is a collision,
indeterminate (fails closed). No comparison goes from a difference of a
member to equal. No baseline of scheme 2 is recorded, so it shares the
number.

It is 3 since WP-155: a unit carries flag bit 1, **emitted unreferenced**,
when it holds the definition of an entity the translation unit emits
whether or not anything refers to it (the rule, and the Kiln fields it
reads, are `spec/landed/verify/WP-155.md` §1): the resolved unit
`definition` of an entity of the definition class (`digest-kinds.md`
§11.9), else the entity's own unit. The flag is part of no unit's digest
(no stream, key or table row changes: every unit digest is what it was
under scheme 2), nor of the group's: `group_digest` hashes a unit's
flags without bit 1, so every group digest and `tu` is what it was under
scheme 2 too. The scheme moves because a shard of scheme 2 holds no
mark: compared with one of scheme 3, a one-sided marked unit would be
read as unmarked, a wrong `verified` (`verify.md` §4 step 3). The move
voids every recorded baseline of scheme 2 (its digests stay valid, its
marks are missing): the shard goldens of `crates/slip_model` are blessed
again with the package; `bench/baselines` and the corpus invariants are
the supervisor's to record again. WP-155's fix round changed which units
are marked (below), not the format; no baseline of scheme 3 was recorded,
so it shares the number. WP-141 landed beside it saying "`scheme` stays
2" (`spec/landed/digest/WP-141.md`): written before this move, it reads
as "no move of its own"; its changes are under 3 for the same reason.
WP-158 (§2.2 "Every instantiation has a unit") changes no existing
unit's stream, key or table row; it adds units (a local specialisation
reached without a call, an instantiated default argument of a local
function) that the stream of no unit held before. That closes wrong
equals (an instantiation that binds otherwise and that no unit held
compared equal), the case this rule moves the number for; but no
baseline of scheme 3 is recorded (the shard goldens of `crates/slip_model`
are unchanged by it, `bench/baselines` was last recorded under WP-050,
and the corpus invariants are recorded by the supervisor at the merge,
who confirms none was), so it shares the number 3.
WP-163 (the Kiln pin `60fb5ab9d`) adds rows, all `out` or `unsupported`
(no stream reads them), and changes streams only where Kiln's facts
changed or where Slip reads them again in the old way
(`spec/landed/digest-kinds/WP-163.md`). Two changes close wrong equals:
an instantiation's parameter attributes now include the ones it
inherits from a later declaration (Kiln WP-3407), and a class's hidden
using-shadow is made in either member order (Kiln WP-3145). No baseline
of scheme 3 is recorded since WP-158, so it shares the number 3. The
supervisor confirms that at the merge.
WP-156 (§2.4, operand units) adds units (an operand unit, a key of a
new kind under its parent) and changes existing streams in one place:
an instantiated class's lists name its member templates that are
derived units (`digest-kinds.md` §5.5 `members-other`), so the
definition streams of such classes change (B3). It closes wrong
equals: a name in an operand that only a type or a value holds bound
otherwise and every unit compared equal (pitfalls N47, N48). No
baseline of scheme 3 is recorded since WP-163, so it shares the
number 3. The supervisor confirms that at the merge.

**The mark** (`crates/slip_digest/src/emitted.rs`). A unit is marked by
either of two ways; the mark is their union.

**In place.** A unit is marked when its own stream writes in place, in
the full walk, the definition of a function with the attribute
`constructor`, `destructor` or `used`: a member of a local type written
where the type is declared (a local class's member function, at any
depth, also inside a lambda's body; a closure type's call operator), the
function itself in a dependent context or not. That holds whatever the
unit's kind: an entity's own unit, a resolved unit `definition`, a
`default_argument` unit (the initialiser of a field an instantiation
made), a content-keyed unit; except a unit whose entity is a template, a
template's pattern, a partial specialisation or in a dependent context,
which is never marked. The walk records the function in the state of the
unit being written (§4.3), which a key, a content digest, a candidate
stream (§2.2) and any other stream written with a fresh unit state put
aside and restore with the rest; nothing is recorded while an owner is
numbered (§4.3), inside a key, a type digest or a profile, or in a stream
whose local state is put aside. So the initialiser of a variable, of a
static data member or of a field, and the body of a function, mark the
unit that holds them, wherever the lambda in them is.

**By entity.** The entities read are the variables (`DeclKind::Var`) and
functions that are not local (a block-scope `extern` declaration is the
namespace's) and not a template's pattern, a partial specialisation or in
a dependent context. The unit marked is the entity's resolved unit
`definition` when there is one, else its own unit when the entity is
defined. One is marked when any of these holds:

1. It, or its defining declaration, has the attribute `used`.
2. Its body or initialiser declares, at any depth, a function with the
   attribute `constructor`, `destructor` or `used`: a member function of
   a local class (also nested, also inside a lambda's body), or a
   lambda's call operator (Kiln accepts `[]() __attribute__((used)) {}`
   and puts the attribute on the call operator). The declaration's
   enclosing declarations are climbed through `ctx`; a closure type also
   counts the variable its name is prefixed by
   (`LambdaMangling::context`). A static local with `used` does not mark
   its function: clang emits it only where the function is emitted.
3. It is a function with `constructor` or `destructor`.
4. It is a variable whose type, through arrays and references, is a class
   whose definition is missing or whose destructor is not trivial
   (`DefinitionBits::TRIVIAL_DTOR`), or a chain of more than 256 such
   steps.
5. It is a variable that is not **constant-initialised**. It is when one
   of these holds: its evaluated state (`VarData::eval`, read in
   definition mode over every declaration) is `Value`; no declaration has
   an initialiser and its type is not a class, or is a class, or an array
   of one, whose default constructor is trivial (`TRIVIAL_DEFAULT_CTOR`
   set and `NONTRIVIAL_DEFAULT_CTOR` clear); its initialiser is the call
   of a trivial constructor with no argument
   (`Ast::is_trivial_default_construction`: Kiln's form of `K k;`); or
   its type is arithmetic, enumeration, pointer or reference and its
   initialiser is **plainly constant**.

Thread-local variables follow the same rules as variables of static
storage duration.

A **plainly constant** initialiser is one of this grammar, read on Kiln's
expression nodes; anything else is not (a call, a read of a variable's
value, a binary or conditional operator, a temporary, a cast written in
the source, a member access, a compound literal), nor is a tree of more
than 64 nodes:

```text
plain   := literal | enumerator | sizeof | address
         | ( plain )                        ParenExpr
         | { } | { plain }                  InitListExpr, at most one element
         | + plain | - plain                UnaryOperator Plus, Minus
         | conversion of plain              ImplicitCastExpr: IntegralCast,
                                            IntegralToFloating, FloatingToIntegral,
                                            FloatingCast, IntegralToBoolean,
                                            FloatingToBoolean, BooleanToSignedIntegral,
                                            NullToPointer, NoOp
literal := IntegerLiteral | FloatingLiteral | CharacterLiteral
         | CXXBoolLiteralExpr | CXXNullPtrLiteralExpr | GNUNullExpr | StringLiteral
enumerator := DeclRefExpr naming an EnumConstant
sizeof  := UnaryExprOrTypeTraitExpr: sizeof, alignof, __alignof
address := & designator                     UnaryOperator AddrOf
         | decay of designator              ImplicitCastExpr ArrayToPointerDecay,
                                            FunctionToPointerDecay
         | designator                       a DeclRefExpr bound as it is: see below
designator := ( designator ) | StringLiteral
         | DeclRefExpr naming a function
         | DeclRefExpr naming a variable (DeclKind::Var) that is not
           thread-local, not of reference type, and of static storage
           duration (not local, or a static local)
```

A bare designator (a `DeclRefExpr` that names no enumerator) is `plain`
only where it is bound as it is: the declared type is a reference, and the
`DeclRefExpr` is the root of the initialiser or is reached from the root
through parentheses, lists of one element and `NoOp` conversions alone.
Anywhere else (under a sign, under another conversion, or for a declared
type that is not a reference) a `DeclRefExpr` is an enumerator or the
initialiser is not plain. The grammar does not rest on Kiln placing an
lvalue-to-rvalue conversion on a read. `designator` reads the declaration
kind `DeclKind::Var` exactly: a binding, a parameter or a declaration of
any other kind is refused.

**What neither way placed.** The counter `digest.emitted.unplaced` (§12)
is the number of local functions that are not in a dependent context,
have a definition and have `constructor`, `destructor` or `used`, which
neither way placed: the first did not meet them in a unit's own stream,
and no declaration around them (rule 2's climb) is one the second way
marked an entity for. It is zero over every fixture; a shape that makes
it non-zero is a mark too few the walk can see, and is listed in
`spec/landed/verify/WP-155.md`.

A mark too many is a finding too many; a mark too few a wrong `verified`.
The marks too many that remain are listed in `spec/landed/verify/WP-155.md`
§1 (a class initialised by a constexpr constructor is the known one).

- A declared unit belongs to the group of the file that holds the entity's
  first declaration in the translation unit that has a location (below). A derived unit belongs to the
  derived group of the file that holds the first declaration of what it
  derives from (the template, the class, the function). The single `tu`
  group holds the order unit and every namespace: a namespace is reopened
  wherever its members are, and which file opens it first is not a fact
  about it. A namespace's unit in a view that is not its own (§3.4,
  WP-140) is there too, beside its own unit. Builtins are in the derived
  group with the empty path.
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
- A declaration Kiln makes with **no location** (an implicit global
  `operator new`, `__int128_t`, an implicit `std::align_val_t`) has no
  file. The **origin file** of a unit is the file of its origin
  declaration if that has a location, else of the first declaration of
  its chain, in chain order (earliest first), that has one: a
  redeclaration in source (`<new>`'s `operator new`) puts the entity in
  that file's group. The declaration without a location stays in the
  chain and is read like any other; only the file is taken elsewhere.
  An entity no declaration of which has a location is derived (§2.2), so
  no declared unit has the empty path. Should the walk find none all the
  same, the unit is written in the derived group with the empty path and
  is indeterminate, `Unaccounted("unit/file")`; a shard holding a
  declared group with the empty path still breaks `spec/model.md` §8.2
  and is refused where it is written (`spec/model.md` §8.5).
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
| `digest.units.derived.emitted` | derived units marked emitted unreferenced (§11, WP-155) |
| `digest.emitted.unplaced` | local functions with `constructor`, `destructor` or `used` that neither way of the mark placed (§11, "What neither way placed"); always present, 0 over every fixture |
| `digest.instantiated.unheld` | function bodies, default arguments and default member initialisers Kiln instantiated that no unit's stream holds (§2.2 "Every instantiation has a unit", WP-158); always present, 0 over every fixture |
| `digest.groups` | groups |
| `digest.indeterminate` | indeterminate units |
| `digest.collisions` | keys shared by different entities; a key shared only by parts is counted unless every two of its parts are parts of two different owners whose shared key collides (§3.4) |
| `digest.bytes` | bytes of unit, type and key streams hashed |
| `digest.indeterminate.<cause>` | indeterminate keys whose first cause is of this kind: `error`, `unsupported`, `unaccounted`, `collision`, `undeclared_local`, `visibility_by_use`, and `pending` while the walk has areas not yet written. Present only when not 0 |
| `digest.indeterminate.pending.<area>` | of the `pending` ones, by area. Goes with `Cause::Pending` |
| `digest.operands.units` | operand units (§2.4, WP-156). Present only when not 0, as each `digest.operands` counter |
| `digest.operands.site.<n>` | written operands of site `n` (1, 2, 4, 6, 8; §2.4), nested ones included; site 9, the written names no stream walked (§2.4 site 9) |
| `digest.operands.unreferenced` | operands of sites 1, 2, 4 and 6 not written because their walk wrote no reference (§2.4 "What is written") |
| `digest.operands.dependent` | instantiation-dependent operands, not written |
| `digest.operands.deduced` | variables declared with a placeholder, whose type is not read (§2.4) |
| `digest.operands.unkept` | entries that are indeterminate for a named cause (site 0: §2.4 "Named limits that fail closed") |
| `digest.operands.names.unclaimed` | written-reference entries that take part and that no unit decided (§2.4 "Conservation"); always written, 0 on every fixture of the digest's tests; not 0 makes `<order>`'s operand unit indeterminate (`operands/unclaimed-name`) |
| `digest.operands.names.undemanded` | entries deferred to a part that was never written (§2.4 "Conservation"), no cause |
| `digest.operands.names.unmet`, `digest.operands.names.trigger.<t>` | site-9 entries by why they are written: a name a join takes that no stream met (L3), or a name a trigger covers (`t1`, `t1b`, `t2`, `t3`, `t4`, `t5`; `<order>`'s are `t1`) |
| `digest.operands.names.role.<role>`, `digest.operands.names.holder.<kind>` | site-9 entries by the entry's role and by its holder's declaration kind (`none`: no holder), each as a word (`ud_suffix`, `cxx_record`) |
| `digest.operands.names.unmet.<role>.<kind>` | the unmet site-9 entries by role and holder kind: the shapes of the names no stream walks, for the corpus run |
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
7. **Every instantiation has a unit.** The counter
   `digest.instantiated.unheld` (§2.2, §12) is 0 for both sides of every
   fixture the digest's tests parse (`tests/common`, beside
   `digest.emitted.unplaced`): what Kiln instantiated is read from Kiln,
   so a route by which an instantiation gets no unit fails the suite on
   the first fixture that takes it. A unit test
   (`demand::tests::unheld_reads_what_kiln_instantiated`) shows that the
   count reads Kiln's facts: with no unit written, it counts every
   instantiated body and default argument of the four shapes of WP-158.

8. **Operand sites** (WP-156). The operand sites are a class of their
   own (`digest-kinds.md` §11.11), listed by name: what a written type
   holds as an expression. Each holder of §2.4 and each site is a
   pair test of `crates/slip_digest/tests/differ/_/operands-*` (the name
   moves, the operand unit differs) and a mutation of the package's
   handback (the holder skipped, the test fails); a holder that a new
   Kiln type adds is a row of §11.11 before the walk reads it.
9. **Every written name is accounted for** (WP-156 fix round 3). The
   counter `digest.operands.names.unclaimed` (§2.4 "Conservation",
   §12) is 0 for both sides of every fixture the digest's tests parse
   (`tests/common`): an entry of Kiln's written-reference table that
   takes part is met by a stream, deferred to a part, or written at
   site 9, so a holder the walk does not read costs a duplicate-free
   entry, not a wrong answer. A unit test
   (`operands::ledger::tests::sweep_counts_what_no_unit_decided`) shows
   that the sweep counts what no unit decided. Layer S's census
   (`operands::census::SLOTS`) gives every stored written type of
   Kiln's role tables and schema descriptors a row, and its test holds
   the two together.

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
- **Inert rules** (§3.4, `tests/inert.rs`): every source of every pair
  test and of every rule case, walked under tables of rules that match
  nothing of it, gives the walk under the empty table, compared in full.
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
| `names-changed <name>` | both sides give a key of the name a names digest (§9 "The names digest"), and they differ |

1. Any other line is an error of the test, as is a line written twice
   and a second `exact derived`. The directive is exactly the text
   `exact derived`.
2. A name is the name of the key's first unit, from side A when A has
   the key. Lines are compared by name: a line names every key of its
   name, and two keys of one name that differ the same way are one line.
3. **`differ`** reads every line, and needs one besides the directive
   and the `indeterminate` lines. An `indeterminate` line there is
   `indeterminate <name>` or `indeterminate <name> (<cause>)`, the cause
   as §10 writes it: the units flagged for a cause other than `Pending`
   are, on each side, exactly those the lines name, and their keys are
   left out of what follows (WP-124: a pattern indeterminate while its
   instantiation shows the change). Over the other declared and `tu`
   units, the keys that
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
5. **`equal`** reads no line but `names-changed` (rule 6) and no
   directive: every unit both sides have
   is equal already, and a unit only one side has, declared or derived,
   fails the test (the derived groups are part of the `Digests`
   compared). **`indeterminate`** reads `indeterminate` lines only.
6. **Names** (WP-112). `differ`, `equal` and the rule suite
   (`tests/rules.rs`) read `names-changed` lines: the keys both sides
   give a names digest whose names digests differ are exactly those
   lines, so a names change no line names fails, and so does a line that
   does not hold. In `equal` a key with a names digest on one side only
   fails the test too, since its units are equal. A pair whose units are
   equal and whose strings changed is therefore an `equal` pair with
   `names-changed` lines.

## Tests

- `crates/slip_digest/tests/equal.rs`, `differ.rs` — §14 pairs, one
  directory of sources per test under `tests/equal/` and `tests/differ/`.
- `crates/slip_digest/tests/determinism.rs`, `demand.rs`,
  `indeterminate.rs` — §14.
- `crates/slip_digest/tests/tables.rs` — §13 obligations 4 and 5.
- `crates/slip_digest/tests/runner.rs` — §14.1, the runners' reading of
  `expect.txt`, over the fixtures of `tests/runner/`.
- `crates/slip_digest/tests/records.rs`, `trace.rs` — §4.4.
- `crates/slip_digest/tests/names.rs` and the `names-changed` lines of
  the pairs — §9 "The names digest" (WP-112).
- `frames_fit_the_stack` in `crates/slip_digest/src/expr.rs` and the
  tests each area adds to it — §12.1.
- `crates/slip_model/tests/formats.rs` — §11, the section's encoding;
  `crates/slip_model/tests/sections.rs` — the `names` section
  (`model.md` §8.7).
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
