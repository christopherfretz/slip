# verify — comparing digests under declared changes

Status: draft (M1). Implemented by `slip_verify` (the comparison: pure, no
front end, no I/O) and by the verify job of the worker (`slip_digest` over
the post-image). Digests, units, keys and groups are defined in
[digest.md](digest.md); this file defines what is compared with what, what
"verified" means, and what a mismatch reports. Design rationale:
[03-equivalence.md](../docs/design/03-equivalence.md).

`slip_verify` and `slip_digest` are Slip's trusted base. Nothing that
produces edits is trusted: a transform, the reducer, the spelling of a name
and a repair can each be wrong, and verification is what catches the
result. So verification takes nothing from them except the **declared
changes** of §2, which are small, are data, and are printed in the report.

## 1. The claim

For one observer, with pre-image `A` (the subject as planned) and post-image
`B` (the subject with the edits applied):

> `B` is the same program as `A` except that the entities the rules name
> have the names the rules give them, and except for the units the
> exemptions list.

"The same program" is the equality of unit sets of `digest.md`: every
entity present in one is present in the other, with the same key after
mapping and the same digest. It is checked per observer. A run is verified
when every observer is (§7). What per-observer verification cannot show is
§8.

## 2. Declared changes

```
Declared := rules:   list<Rule>      digest.md §3.4; sorted by (parent, kind, name, disc), no two with the same
            changed: list<KeyId>     units whose digest may differ
            created: list<KeyId>     units that may exist only in the post-image
            removed: list<KeyId>     units that may exist only in the pre-image
            kept:    list<KeyId>     version 2: owned entities that are, on purpose, members of a namespace a rule maps onto (§9.2)
            files:   opt<Ownership>  version 2: which files are read-only (`spec/edits.md` §2.1); required when `rules` is not empty
```

`kept` and `files` are version 2 of the container (WP-110); a version 1
`DECL` is read as one with `kept` empty and `files` absent, and a
version 1 `DECL` with a rule is refused. `kept` is sorted and has no
duplicate, like the other lists. `files` is part of the rule table's
identity: `rules_id` is then `H("slip.rules.v2", rules, files)`.

Each list is sorted and has no duplicate (rules by the bytes of their
encoded `(parent, kind, name, disc)`, keys by their bytes); a key may be in more than one of
the exemption lists and in `kept`. Every `KeyId` is in the key space
of the pre-image: for an entity a rule renames, the key it had before. The
container form is kind `DECL`, version 2 (a reader also reads version 1),
one required section `declared` (id 1) holding this record; its JSON
rendering follows `spec/model.md` §10, with members `rules`, `changed`,
`created`, `removed`, and in version 2 `kept` and `files` (`null`, or
`{"owned": [...], "readonly": [...]}`, each path in the form of
`spec/model.md` §10 for bytes). `files` is encoded as `opt` of the
record `owned: list<bytes>, readonly: list<bytes>`. A writer writes
version 2; `dump` and `undump` keep a file's version, so a version 1
file's rendering has version 1 and the four members.
`rules_id = H("slip.rules.v1", rules)` names the rule table when `files`
is absent, and `H("slip.rules.v2", rules, files)` (rules as
`list<Rule>`, then `owned` and `readonly` as `list<bytes>`) when it is
present.

**The reader's checks**, in this order, each with its message; a binary
reader reports `Malformed` at the offset named, a JSON reader at the path
(`crates/slip_model/tests/declared.rs` has one test per row):

1. The rules are strictly ascending and admissible (WP-022: an invalid
   name, `two rules with one (parent, kind, name, disc)`,
   `rules are not strictly ascending`; at the later rule), and no two
   produce one key (WP-111 review F1: `two rules produce one (parent,
   kind, name, disc)`, at the later rule of the pair whose produced key
   is least, after the order of every rule is checked).
2. `changed`, `created`, `removed`, in that order:
   `<list> is not strictly ascending`, at the later key.
3. `kept is not strictly ascending`, at the later key.
4. `files`, as a transform request's ownership (`spec/edits.md` §2.1,
   `spec/landed/edits/WP-060.md` §3), with its messages and in its order:
   `owned is empty` (at `owned`); `not a canonical absolute path`,
   `owned is not strictly ascending` (at the entry); the same two for
   `readonly`; `a readonly entry is also in owned` (at the `readonly`
   entry).
5. `a version 1 container has a rule`, at the first rule.
6. `rules without files`, and `files without a rule`, at `files` (its
   tag byte). `files`
   is present exactly when the rules are not empty: with no rule nothing
   is mapped and the files are not read (§2.1), so a second encoding of
   the empty table, with another `rules_id`, is refused rather than
   given a meaning.

A value made in memory may hold rules and no files; `validate` refuses
it (rows 3 to 6), and so does the writer's debug assertion.

With all four lists empty the claim is plain equality. That is the check of
corpus invariants (a) and (b).

### 2.1 Rules

A relocation "the entity with key `K = (parent, kind, name, disc)` is now
`(parent', name')`" is declared as the rule that **undoes** it:

```
Rule { parent: map(parent'), kind, name: name', disc,  to_parent: parent, to_name: name }
```

where `map(parent')` is the new parent as the walk maps it under the rules
that undo the relocations made before this one (`parent'` itself if no
rule reaches it; `root`, `c_linkage` and `builtin` map to themselves). The
walk applies each rule to a key whose parent is already mapped, so a rule
for a member of a relocated parent names the parent as the parent's own
rule maps it; `slip declare` makes it so by mapping a `relocate` line's new
parent under the lines above it (§2.3): a line whose new parent another
line relocates comes after that line
(`crates/slip_digest/tests/rules/parents/mapped-parent`, and
`mapped-parent-reversed`, whose lines in the other order give differences). Both parents of a rule are `Parent`s
(`digest.md` §3.4). The walk of the
post-image applies the rules bottom-up (`digest.md` §3.4), so the relocated
entity, its members, and every type and signature that mentions it are
written with the keys they had in the pre-image, and the comparison below
needs no map.

- A rule is applied to an entity at most once; rules do not chain. Renaming
  `f` to `g` and `g` to `f` in one run is two rules.
- **One way to each key** (WP-111 review F1). No two rules of a table
  produce one key, `(to_parent, kind, to_name, disc)`, and no rule
  produces the key it matches (`parent == to_parent`, `name == to_name`:
  it changes nothing). Two producers of `K` say that two entities of the
  post-image are both the pre-image's `K` (one entity renamed twice, a
  namespace split in two by two namespace rules); with them an entity
  that reaches `K` through one rule is `via` of that rule and is counted
  in no count of the other, so two observers that disagree about it are
  both silent (§9.2). An identity rule beside a rule that produces its
  key is the same case. So `RuleTable::new` refuses a table with two
  producers of one key (`RuleError::SameProduced`, naming both), and so
  does the reader of every container that holds a table (row 1 above),
  in the trusted base, since a table also comes from a transform's
  `expect_renamed` and not only from `slip declare`; `Rule::refusal`
  refuses an identity rule (`RULE_IDENTITY`). A table has at most one
  rule that produces a key (`RuleTable::produces`). A namespace split in
  two is declared per member or per nested namespace (§2.3).
- **Namespaces merge.** A rule of kind `namespace` may give its entity the
  key that another namespace of the post-image has: `acme` mapped to
  `legacy` while `legacy` still exists for the entities that stay in it.
  That is one rule beside a namespace that no rule maps, never two rules
  of one target, which "One way to each key" refuses.
  The two are then one unit (`digest.md` §3.4); if their digests differ
  there are two units with one key, and the comparison reports it (§4).
  For every other kind, two entities with one mapped key are a collision
  and both are indeterminate.
- Rules match in mapped terms. After `acme` is mapped to `legacy`, a rule
  with parent `legacy` matches members of either namespace. A rule that
  renames a relocated member to a name that a staying member of the old
  namespace has therefore collides, and is reported, although the edit may
  be correct ("Open questions").
- **An unnamed class is named by the plain name.** An unnamed class or
  enumeration named for linkage by a typedef, or by its first declarator
  (`digest.md` §3.3), has `unnamed_by` the name as written in the
  post-image, not as a rule maps it: a rule that renames the typedef does
  not rename the class, which needs a rule of its own, from `unnamed_by`
  the new name to `unnamed_by` the old one
  (`crates/slip_digest/tests/rules/unnamed/typedef-renamed-no-class-rule`,
  `typedef-renamed-with-class-rule`; native rename makes that rule,
  `edits.md` §9.2).
- **No rule matches a specialisation.** A rule of kind `specialisation`
  or `partial_specialisation` is refused (`RULE_ON_SPECIALISATION`): a
  specialisation's key follows its template's (`digest.md` §3.4), and the
  rule would key it apart from its template, saying that a use of one
  template's specialisation is a use of another's
  (`tests/rules/wrong/named-only-spec-reparented-defined-templates`,
  `named-only-spec-reparented-relocate-form`: before, both equal). A
  member of a class template's implicit instantiation is the same: its
  key follows the pattern member's, and a rule that matches the member
  itself makes its key indeterminate (`tests/rules/instantiated/`).
  And a part of a unit: a rule of kind `resolved` or `default_argument`
  is refused (`RULE_ON_PART`): its key's parent is its owner's mapped
  key, and its name a property or a parameter index, which no edit
  renames apart from the owner.
- **No rule moves an entity into or out of the unnamed namespace by
  renaming a namespace.** A rule of kind `namespace` whose `name` is the
  unnamed namespace's and whose `to_name` is not, or the reverse, is
  refused (`RULE_UNNAMED_NAMESPACE`): the namespace's members change
  linkage (below)
  (`tests/rules/wrong/unnamed-namespace-renamed-to-named`,
  `named-namespace-renamed-to-unnamed`,
  `named-namespace-merged-onto-unnamed`: before WP-103, all equal, with
  internal symbols made external or the reverse). Since WP-113 the
  comparison finds every such relocation of a namespace with members
  (each member's row `enclosing` differs); the refusal is kept because it
  says why when the rule is declared, not after a run. The one correct
  transform it refuses is the rename of a namespace with no member that
  has a row (`namespace { }` to `namespace foo { }`), which changes no
  symbol (`spec/landed/digest/WP-113.md`, item 6).

These, and the identity rule of "One way to each key" above, are
refused in one place, `Rule::refusal`
(`crates/slip_model/src/keys.rs`), and every table records the first rule
it refuses when it is made (`RuleTable::refused`), whichever way: by
`RuleTable::new` or by the reader of the declared container. The
container's encoding still admits such a rule (it is a well-formed
`Rule`); what refuses it is the use: a resolver (`slip declare`, §2.3)
must not make a table that holds one, and the walk keys nothing under
such a table, so every unit of the post-image is indeterminate
(`key/refused-rule`) and nothing verifies.

**Rules rename owned entities only.** A run has files it may edit and
files it may not (`spec/edits.md` §2.1; `Declared.files`). A rule that
says "`acme` is what `legacy` was" is a claim about the code the run
owns; the dependency's code was not edited, its binaries were not
rebuilt, and whatever it declares has the name it had. An entity of the
post-image is a **dependency entity** if

- one of its declarations lies in a read-only file. A declaration lies
  in the file of its **expansion location**: the file it is written in,
  and for a declaration that a macro expansion produces, wholly or in
  part, the file that holds the outermost invocation. It also lies in a
  read-only file if its **name token** (the token that spells its name;
  an implicit declaration and one with no name have none) is spelled in
  a read-only file: a dependency macro's body that spells the name. A
  name token spelled in no file (made by `##`, in scratch space) is
  decided by the expansion location alone. A declaration with
  no location in a file (a builtin, an implicitly declared function)
  lies in a read-only file; so does one whose expansion location is in
  a buffer that is not a file (`<built-in>`, the command line, a
  buffer whose name begins with `<`, scratch space), in a file the
  image cannot map to a subject path (§5.1), or in a file read after
  the walk was given its files. This holds under every ownership, a
  walk with every file owned included: such a declaration is never
  owned (`tests/rules/owned/builtin-declarations-read-only`). An entity
  with more declarations than
  the walk follows (`digest.md` §2, the redeclaration limit) is a
  dependency entity: an unread declaration might be in a read-only
  file;
- where a file is read-only, it is so **per inclusion**: a file entered
  through an include chain that passes through a read-only file (it, or
  a file that includes it, or one further up, is read-only) is
  read-only for that inclusion in that translation unit, whatever the
  run's files say of it. The dependency was compiled against what it
  includes, so its binary bakes in the types, typedefs, exception types
  and RTTI of an owned header it includes: moving them breaks it
  (`tests/rules/owned/dependency-built-against-owned-*`). Kiln has one
  buffer per file and one location range per inclusion, so each
  inclusion is decided on its own; an includer that is no file (the
  command line) does not make a file read-only. An owned file that
  includes a read-only one stays owned;
- or it is a specialisation of a template, implicit, explicit or
  partial, or a deduction guide made from a template's constructors, and
  the template is a dependency entity. A specialisation is keyed by its
  template and its arguments, and the one a source writes has the key of
  the one an instantiation makes (§4 step 4 depends on it);
- or it is an implicitly declared member and its class is one.

Every other entity is **owned**. Ownership is decided once per entity,
from all its declarations in the translation unit: not by its first
declaration, nor by its definition (an owned definition of a function a
dependency header declares is the dependency's). The walk maps a key bottom-up as above,
with two changes:

1. **A rule matches an entity only if the entity is owned.**
2. **The namespaces that enclose a member of a namespace are mapped from
   the member's point of view.** For an owned member, the rules of kind
   `namespace` apply along the chain of namespaces that enclose it,
   whoever else opens them. For a dependency member, no rule applies to
   any namespace of that chain. A parent that is not a namespace (a
   class, a function, a specialisation) has one key, its own, and its
   members have it as their parent. A namespace's own unit is a member
   of the namespace that encloses it like any other entity: it is owned
   if none of its openings lies in a read-only file.

What a discriminator mentions (template arguments, parameter types) is
mapped as that entity is, by its own ownership: with a dependency
template `vec` and an owned `Mine`, `legacy::vec<acme::Mine>` is written
`legacy::vec<legacy::Mine>`, and its member `size` under that key.

So `acme` mapped to `legacy` says where the **owned** members of `acme`
came from, and nothing about the rest:

- A dependency's declaration that the edit moved into `acme` (a
  dependency header included inside a moved block; a dependency macro
  that opens the namespace, redefined by owned code) keeps `(acme, …)`;
  the pre-image's `(legacy, …)` has no counterpart; both are findings.
  Without this rule the two images are isomorphic and the edit verifies,
  although the dependency's binary still defines the entity in `legacy`
  (pitfall row N34).
- A dependency that itself opens `acme`, before and after, has the same
  keys in both images, and the owned members of `acme` are still mapped:
  their keys agree. The namespace's own unit is the dependency's (one
  opening is read-only), keyed `acme`; and since owned code opens it too,
  it also has a unit **in the owned view**, keyed `legacy` under the rule,
  with the namespace's one digest (`digest.md` §3.4 "A namespace's unit
  in each view", WP-140). That unit is the pre-image's `legacy`'s
  partner, and the rule is used by it (`via`, §9.2). So a correct move
  into a namespace that a dependency opens verifies
  (`tests/rules/stale/dependency-opens-target-moved`, `tests/rules/views/`,
  pitfall row N46), in every observer alike: one that does not include
  the dependency has `acme` owned and mapped, the same unit
  (`views/target-opened-observer-without-dependency`, and two observers
  that reach the dependency's opening through different headers,
  `views/observer-direct`, `observer-through-header` and
  `crates/slip_cli/tests/verify_job.rs`
  `wp140_relocation_into_a_namespace_a_dependency_opens_over_two_observers`).
  The digest is the namespace's, over every opening, in both views: an
  owned opening that adds `[[deprecated]]`, or a dependency's `inline`
  that the pre-image's namespace did not have, is a difference
  (`views/deprecated-owned-opening`, `deprecated-dependency-opening`,
  `inline-reopened-plain`); so is an attribute that applies to its own
  opening only (`visibility`), which fails closed
  (`views/visibility-owned-opening`). A reference to the namespace
  writes its own key, the dependency's: "Open questions", "References to
  a namespace that two views key".
- An owned declaration made through a dependency's macro
  (`LEGACY_DECLARE(Mine)` invoked in an owned file) is owned: its
  expansion location is owned, and its name `Mine` is spelled in the
  owned file, as an argument. The name token's spelling decides, not the
  spelling of the whole declaration, or every owned declaration behind a
  namespace macro of the dependency would be the dependency's. A
  dependency macro whose body spells the name (`#define LEGACY_API_DECLS
  void legacy_init(); int legacy_count();`) declares dependency
  entities wherever it is invoked: the dependency fixes those names, and
  its binary may define them (`dependency-macro-fixed-functions`,
  `dependency-macro-declares-fixed-name`). So does a name spelled in a
  dependency's body and passed on through an owned macro's argument
  (`dependency-macro-name-through-owned-macro`): the token is still the
  dependency's text.
- An owned explicit specialisation of a dependency template, an owned
  definition of a function the dependency declares, a function a
  dependency class befriends: dependency entities. They cannot move, and
  moving one is two entities, a collision or a pair of findings.

Ownership is read from the declarations the translation unit has. An
entity that the dependency declares only in a header this observer does
not include is owned here; an observer that includes both sees the
entity declared in two namespaces and reports it; if no observer
includes both, nothing shows it (§8).

Three holes remain where ownership reads as owned what the dependency's
binary fixes. Each verifies wrongly today; each is pinned by a pending
case beside the case it is the pair of, and the planned answer for all
three is the run-level check of WP-116: an entity a rule renames is
defined in an owned file by some observer.

- **Two observers.** As above: the owned declaration and the
  dependency's are never in one translation unit
  (`observer-includes-owned-only`, equal; its pair
  `observer-includes-both`, a finding).
- **A pasted name.** A name made by `##` from tokens of a dependency's
  body (`#define DECL_INIT() void legacy_##init();`) is spelled in
  scratch space and decided by its expansion location: owned
  (`dependency-macro-name-pasted`, pending; its pair
  `dependency-macro-fixed-functions`). At this pin Kiln does not say
  which operands of `##` came from an argument, and a pasted name made
  from an owned argument (`TEST(Suite, Name)`) must stay renamable.
- **A guard-skipped inclusion.** A file entered first directly, then
  included by a read-only file whose `#include` the include guard or
  `#pragma once` skips, is owned for its only inclusion: the skipped one
  makes no location range, and nothing the walk reads at this pin
  records it (`dependency-built-against-owned-eh-direct-first`,
  `dependency-built-against-owned-type-direct-first`, pending; their
  pairs, the `-through-dependency-first` cases). Kiln's WP-2111
  (every inclusion directive recorded, skipped ones included) would let
  the walk close it per translation unit.

**The cost of the include-chain rule.** It errs in the safe direction
in one common shape: a read-only file that the build generates and that
includes owned code. A CMake precompiled-header file (`cmake_pch.hxx`,
in the build tree, itself entered by `-include`) and a unity-build main
file (`unity_0_cxx.cxx`, which `#include`s owned `.cpp` files) are
read-only, since they lie outside every subject root, so every owned
entity behind them is a dependency entity for that observer: no rule
renames it, and a correct migration is reported as a difference in every
such observer (`tests/rules/owned/generated-includer-of-owned`, a
read-only `pch.h` including an owned `src.h`;
`main-file-read-only`, a read-only main file). That stays: knowing
less never accepts more. The answer is a third class of file, **not
edited, and rebuilt from source**, whose names no binary outside the run
holds; it is an open question below and a package of its own
(`docs/wp/WP-118.md`).

`slip declare` (§2.3) refuses a `relocate` line whose entity is a
dependency entity and not a namespace (`DependencyEntity`).

With an empty rule table nothing is mapped and ownership is not
consulted: a digest of the map phase does not depend on `files`. A walk under
rules that is not given the run's files treats every file as read-only:
every key is then the key of the empty table (knowing less never
accepts more).

The run's files are in subject paths, and ownership is of **real
paths** (`spec/edits.md` §2.1). The job decides each buffer by the
subject path of the buffer's real path, its symbolic links resolved, and
never by the path as the front end spells it: a read-only file reached
through a symbolic link under an owned root is read-only. That subject
path is a real path of the subject (or a created path under its real
directories) only because guard G6 (§5.3) holds: a post-image whose
links differ from the subject's along the routes the parse used is
`unverifiable`. `ReadOnlyFiles`
decides on the path it is given and normalises nothing
(`owned.rs`, `read_only_files_needs_the_real_path`).

Two kinds of key are not an entity's and are not decided by ownership:
the key of a `resolved` or `default_argument` unit has its entity's
mapped key as its parent, and a rule of either kind is refused (above);
the order unit's key is a constant. What the rules do to such a unit
follows from its entity's key.

**Why applying rules to the post-image is sound.** Let `φ` be the function
from the entities of `B` to keys that the walk computes. If no unit is
indeterminate, `φ` is injective except on merged namespaces. Equal unit
sets then say that `φ` is a bijection between the entities of `B` and those
of `A` (namespaces aside) under which every unit's content is the same,
every reference inside a unit included, because references are written as
mapped keys. That is an isomorphism of the two resolved programs, as far
as the units hold them (not the linkage and visibility an entity takes
from its namespaces, below; nor what §8 lists); the rules only choose
which isomorphism is tried. Within that, a wrong or missing rule cannot
make two different programs compare equal: it changes which keys the
post-image gets, and any key that then fails to line up is a reported
difference.

`φ` is over every entity the walk gives a key, not over units only: an
entity with no unit is still written into the references to it by its
mapped key, so if it shares that key with another entity, a reference to
one reads as a reference to the other. The collision of `digest.md` §3.4
is therefore counted over keys, whether or not a unit has them
(`crates/slip_digest/tests/rules/wrong/named-only-specialisations`,
`merged-local-extern`: before WP-103 both compared equal). A dependent
name in a template's definition is written as a name, which no rule maps:
the definition is compared by its names, and what such a name resolves to
is compared in the instantiations, derived units both images have (§4);
a template nothing instantiates is outside what is shown (§8;
`tests/rules/attempts/dependent-name-unused-template` compares equal
though the programs differ).

Two namespaces sharing a key is the one place `φ` is not injective. A
namespace's unit holds only what the namespace itself is (`digest.md` §5),
and its members are units of their own with keys that do collide if two
of them coincide. Generated code depends on which namespace an entity is
in through names, which is what a relocation declares, and **through two
properties the member takes from its enclosing namespaces**, which it
does not declare:

- **linkage**: a member of the unnamed namespace, or of a namespace
  within it, has internal linkage ([basic.link]/4); moved out, it has
  the linkage its declaration gives it (an inline function or a class's
  member functions become weak external symbols, a variable a global
  one), and moved in, internal;
- **visibility**: `__attribute__((visibility(…)))` on a namespace
  opening sets the default visibility of the symbols declared in that
  opening and in the namespaces written inside it.

**The rule: what an entity takes from its namespaces is in its own
unit.** Every entity whose semantic context is a namespace (and every
block-scope `extern` declaration, keyed in its namespace) has in its unit
the row `enclosing` (`digest-kinds.md` §1.7, WP-113): the linkage of its
name, whether an unnamed namespace encloses it, and the visibility its
enclosing openings give its declarations, all computed on the parse the
unit is written from. The visibility is found per declaration, from the
opening it is written in or, for one written qualified outside its
namespace, the latest opening of the namespace written before it; where
the declarations' visibilities differ, a use's symbol takes that of the
declaration most recent at the use, which no unit holds, and the unit is
indeterminate (`visibility_by_use`, `digest.md` §10). Which declarations
count is clang's: a specialisation of a class or variable template and
an instantiation take the template's, wherever they are written; an
explicit specialisation of a function template its own; a class or an
enumeration its definition (`digest-kinds.md` §1.7 item 5). A key holds names only (`digest.md` §3), and the
rules rewrite keys only, so the row is written as the post-image has it
and is not mapped: a relocation that changes either property for any
entity makes that entity's unit differ, whatever rule table is given. An
entity in a class or a function has no row: what it takes from the
namespaces it takes through that class or function, whose unit has the
row, and with it differs (a member function's symbol is internal because
its class is; a local class or a closure is the function's;
`spec/landed/digest/WP-113.md`, "The chain", also for friends,
specialisations and template arguments). A namespace has no row: it has
no symbol, and a merged namespace's unit is shared.

This closes the forms that WP-103 left open (`tests/rules/needs-wp113/`,
`tests/rules/enclosing/`): an entity of any kind relocated out of the
unnamed namespace, into it, or between namespace openings of different
visibility; a named namespace relocated, under its own name or another,
to or from a parent within the unnamed namespace; and a namespace
relocated between parents of different visibility. A relocation between
two named namespaces of the same visibility, or from an unnamed namespace
to another unnamed namespace, compares equal, as it should.

The argument is about one translation unit. The ownership rule above is
what makes the declared isomorphism one that the rest of the program
(code that was not edited and is not an observer) can be party to. It
fixes the key of every dependency entity whose parents are namespaces,
or dependency entities themselves, so the isomorphism is the identity on
those. It does not fix the key of a dependency member of an owned class
(a read-only file included inside the class body): the class has one
key, its own, and it is mapped, so the member's key is mapped with it
(`tests/rules/owned/dependency-members-in-owned-class`, equal). That is
harmless unless the dependency's binary defines the member under the
class's old name, which needs the dependency compiled against the owned
class: the include-chain rule above makes such a class read-only where
an observer sees the dependency include it, and otherwise it is the
guard-skipped or two-observer hole.

A pre-existing entity that a rule would capture is caught by the same
comparison: if the pre-image already had an entity with the key a rule
matches, the post-image's entity of that key is mapped away, and the
pre-image's unit has no counterpart (`removed`).

### 2.2 Exemptions

- `changed`: a unit with this key may have any digest in either image. It
  must still exist in both.
- `created`: a unit with this key may be absent from the pre-image.
- `removed`: a unit with this key may be absent from the post-image.

Absent means that the image has no unit of the key at all, declared or
derived. An entity that one image declares and the other derives (an
explicit specialisation removed, whose uses now instantiate the primary
template; or one added where the template was instantiated before) is
in both images: it is neither created nor removed, it is `changed`
(§4 step 4).

Nothing else is weakened. A created entity that changes how an existing
name resolves, or a removed one that something still referred to, changes
the digest of the unit holding that reference, which is not exempt. An
exemption that an observer did not need is not an error for that observer
(the entity may not be visible in it); one that **no** observer needed is
reported for the run (§7).

Exemptions are by key. A unit keyed by content (`digest.md` §3.3) cannot be
`changed`: changing it removes one key and adds another.

### 2.3 Declared changes by name

A `Declared` holds keys, and nobody writes a key by hand. `slip declare`
(`spec/exec.md` §8.8) makes a `Declared` from a text file by resolving
names in the two images. It is outside the trusted base: what it produces
is the data of §2, which the report prints, and a wrong rule cannot make
two different programs compare equal (§2.1).

**The text.** UTF-8; a byte order mark at its start is skipped. A line that is empty, or blank, or whose first
character after blanks is `#`, is ignored. Every other line is one of

```
relocate <entity> => <parent> :: <name>
changed  <entity>
created  <entity>
removed  <entity>
kept     <entity>
```

- The first word is the keyword; blanks (spaces and tabs) separate it from
  the rest, and the rest is trimmed of blanks at both ends.
- `<entity>` is `[<kind>] <text>` or `<text>`. `<text>` is taken literally:
  it is a unit's name as §6 `name` renders it (`ns::f(int, char *)`). The
  optional `[<kind>]` is the name of a key kind (`digest-kinds.md` §3.1:
  `[function]`, `[using_declaration]`), for a name that units of two kinds
  share.
- For `relocate`, the rest is split at its first ` => `. The right side is
  split at its last `::`; what follows, trimmed, is `<name>`: an identifier
  (`[A-Za-z_][A-Za-z0-9_]*`), or `=` for "the name it has". What precedes,
  trimmed, is `<parent>`: an `<entity>`, or empty for the global namespace.
  So `:: acme`, `acme :: g` and `acme::g` are all read.

**Records.** A **unit record** of an image is, for a unit of it, its
`KeyId`, its `Key` (the record the id is the digest of, `digest.md` §3),
its name and whether its key is a fallback key (`digest.md` §10). The
records of an image are those of the observers the command selects, each
digested once per table below. A name **resolves** in a set of records to
the one `KeyId` that every unit of that name (and kind, if given) has.

| Line | Resolved in | With rules |
|---|---|---|
| `relocate`, its `<entity>` | the pre-images | none |
| `relocate`, a non-empty `<parent>` | the post-images | none |
| `changed`, `removed` | the pre-images | none |
| `created`, `kept` | the post-images | the rules this file gives |

**The rule of a `relocate` line.** Let `K = (parent, kind, name, disc)` be
the `Key` of `<entity>`, and `P'` the new parent: `root` for an empty
`<parent>`, else `unit(id)` of the entity it names. The line gives

```
Rule { parent: m(P'), kind, name: N', disc,  to_parent: parent, to_name: name }
```

with `N'` the identifier given (`name` itself for `=`), and `m` the mapping
of §2.1 computed from the post-image's records: `m(root)`, `m(c_linkage)`
and `m(builtin)` are themselves; `m(unit(id))` takes the `Key` `k` with that
id, replaces its parent by `m(k.parent)`, applies the rules of the
`relocate` lines **above this one** as `RuleTable::map` does, and is
`unit` of the id of the result. A discriminator on the way is not mapped:
a new parent whose own discriminator mentions a relocated entity gets a
rule that does not match, which verification then reports as differences.
So a line whose new parent another line relocates comes after that line,
and `slip declare` refuses the other order (`Order`, below).

**Order.** Each level of the chain `m(P')` looks up in the rules above
(the `Key` of the level, its parent already mapped) is a **probe** of the
line. A `relocate` line one of whose rules matches a probe of a line
above it, `(parent, kind, name, disc)` equal, is the error `Order`,
reported at that later line and naming the earlier: the earlier line's
parent was resolved without the later rule. With this check every file
`slip declare` accepts gives one rule table, and so one `rules_id`,
whatever the order of its lines: a rule that matches no probe of a line
above it changes no level of that line's chain, so the line's `m(P')` is
the one the whole table gives; the other lines' rules do not depend on
order; and `Duplicate` is a property of the set of rules. (Tested by
permuting the lines of each case into every order: each is either
refused with `Order` or gives the one table, `crates/slip_cli/tests/
declare.rs`.)

**A function a class template befriends without defining it.** A friend
declaration of a function (or function template) in a class template's
pattern that does not define it is its own entity, keyed in that class,
and is not followed to the function's rule (`spec/landed/digest/
WP-103.md`, fix round 3, finding 2). For a `relocate` line whose entity
is such a function and whose `<name>` changes the name, `slip declare`
also writes, for each such friend declaration of the pre-images (same
kind and name, in a class whose nearest enclosing namespace is the
function's parent, named `<class>::friend ` followed by the function's
name as its record gives it, qualified (`n::A::friend n::f(int)`, the
display name of a friend function below; before WP-111's review the
unqualified name was matched, and a function of a namespace got no such
rule)),

```
Rule { parent: m(F.parent), kind, name: N', disc: F.disc,  to_parent: F.parent, to_name: F.name }
```

with `F` the friend declaration's `Key`. The friend stays in its class,
so a line with `=` writes none. A line that gives one of these rules
again (`relocate A::friend f(int) => A :: g`) is accepted, in either
order; one that gives another rule with its match is `Duplicate`.

**An entity with C language linkage.** Its key's parent is `c_linkage`
whatever namespace declares it (`digest.md` §3), so moving it between
namespaces changes no key: it needs no line; a line given is accepted and
writes none. Renaming it does: for an
`<entity>` whose `Key` has the parent `c_linkage`, the line's `<parent>`
is resolved and checked like any other (it says where the declaration
is, and a name that resolves to nothing is an error), and the rule is

```
Rule { parent: c_linkage, kind, name: N', disc,  to_parent: c_linkage, to_name: name }
```

So `relocate legacy::c_fn(int) => legacy :: c_fn2` and `=> :: c_fn2`
give one rule. The row that needs it is the rename of a C function to a
builtin's name (`docs/wp/WP-109.md`, sketch 11): the front end then
treats the calls as the builtin's, which the callers' units show. What
C sources and the linker see of such a rename is §8's. Such a line's
parent looks nothing up in the rules above (its rule does not depend on
it), so it has no probe (**Order**). A line that moves the function
without renaming it (`relocate legacy::c_fn(int) => acme :: c_fn`, or
`=`) is the one that needs none: its parent is still resolved and
checked, and it writes no rule (the rule would be an identity rule,
which §2.1 refuses, and beside a rename of the function it would hide
the rename, WP-111 review F1). `slip declare` says so on standard error,
one note per such line (`spec/exec.md` §8.8); `Resolved.unwritten`
lists their line numbers.

Both answers to a line that changes no key are meant, and they differ
for a reason. For an entity with C language linkage the line is a true
statement about the source (the declaration moved) that needs no rule,
because its key does not hold the namespace: a note, no rule. For any
other entity a `relocate` whose target is its own source declares
nothing (`relocate n::C => n :: =`): it is the error `Refused` (an
identity rule).

**A namespace split in two.** A namespace rule says that the owned
members of the namespace moved together. A namespace whose members went
partly to `acme` and partly to `acme2` is not declared by two namespace
rules (`relocate legacy => :: acme` and `relocate legacy => :: acme2`
produce one key, which no table holds, §2.1: `Duplicate`); it is
declared per member (`relocate legacy::a => acme :: =`) or per nested
namespace. The per-member declaration verifies, and a member moved to the
other half than its line says is a finding (`crates/slip_cli/tests/
declare.rs`, `loop_namespace_split_per_member`;
`crates/slip_digest/tests/rules/stale/split-per-member`,
`split-per-member-wrong-half`).

**Errors**, each naming the line number. One error is reported: the
first `Syntax` error in file order if the text has one; else the first
error in file order among the `relocate`, `changed` and `removed` lines;
else the first among the `created` and `kept` lines, which are resolved
last because they need the rules the file gives:

| Error | When |
|---|---|
| `Syntax` | an unknown keyword; a missing `<entity>`; a `relocate` without ` => `, without `::`, or with a `<name>` that is neither an identifier nor `=`; an unknown `[<kind>]` |
| `NoUnit` | the name resolves to no unit; the message lists up to eight unit names of the set that contain the text's last component |
| `Ambiguous` | units of that name have more than one `KeyId`; the message lists their kinds |
| `Fallback` | the unit has a fallback key: there is no `Key` to make a rule from, and no stable key to exempt |
| `NotRenamable` | `relocate` with an identifier for an entity whose `Name` is not an identifier |
| `Duplicate` | two `relocate` lines give rules with one `(parent, kind, name, disc)` (one entity relocated twice to one place, or two entities of one kind and discriminator relocated to one place; two overloads to one name have two discriminators and are not); two `relocate` lines give rules that produce one key (one entity relocated to two places, a namespace split by two namespace rules: §2.1, "One way to each key"), the message naming the earlier line; two lines of one exemption list give one key. Reported at the later line |
| `DependencyEntity` | `relocate` whose entity is not a namespace and is a dependency entity (§2.1, rule 1) in some selected observer's pre-image under the run's files: no rule renames it. A namespace is not refused: its rule applies in the owned view (§2.1, rule 2) |
| `Order` | `relocate` one of whose rules matches a probe of a line above it (**Order** above); the message names that line |
| `Refused` | `relocate` whose rule §2.1 refuses (`Rule::refusal`): of kind `specialisation` or `partial_specialisation` (rename the template), or `resolved` or `default_argument` (a part follows its owner), or an identity rule (`relocate n::C => n :: =`: the entity relocated to where it is; but for C language linkage, above, where such a line writes no rule). A namespace rule across the unnamed namespace, the third form §2.1 refuses, is never made by a line: the unnamed namespace's name is not an identifier (`NotRenamable`), and no identifier names it. The walk keys nothing under a table that holds such a rule, however it was made; this error is where its reason is given |

A file with no line gives `Declared::none()`.

**The run's files.** A rule renames owned entities only (§2.1), so the
container `slip declare` writes carries the run's files, from its
`--owned` and `--readonly` (`spec/exec.md` §8.8): real paths, which are
subject paths. A `relocate` line needs them: a text with one and no
`--owned` is a usage error, with no default (knowing less never accepts
more, and "the roots" by default would make a dependency's entity
renamable by accident). The `created` lines are resolved in records
walked with these files, and so are the `kept` lines. Whether an entity
is a dependency entity is decided on the pre-images, each file by the
subject path of its real path, as the verify job decides it on the
post-image; an entity that is one in any selected observer is one. An
`--owned` entry that covers no file a selected pre-image read is a
warning (`spec/exec.md` §8.8).

**Interface.** The records come from the walk (`slip_digest::records`,
which is trusted base only in that it must not disturb the walk); the
text and its resolution are `slip_cli::declare`, pure functions over
records, so that they are tested without a parse.

```rust
// slip_digest::records
pub struct Record {
    pub id: Digest,              // the unit's KeyId, as `digest` gives it under `rules`
    pub key: Option<Key>,        // the Key whose id that is; None for a fallback key
    pub kind: KeyKind,
    pub class: GroupClass,
    pub name: String,            // §6.1
    pub indeterminate: bool,
}
/// One record per unit that `digest(input, …)` has under `rules`, sorted by
/// `(id, name)`. `Err`, naming the unit, if a unit that has no fallback key
/// gets a `Key` whose id is not the unit's `KeyId`: no record is guessed.
/// `read_only` is the run's read-only files, as `Options::read_only`
/// (`digest.md` §11); not read when `rules` is empty.
pub fn records(input: &Input<'_>, rules: &RuleTable, read_only: ReadOnly<'_>) -> Result<Vec<Record>, String>;
/// By unit KeyId under the empty table: the units of dependency entities
/// under `read_only`, and of friend declarations in a templated class that
/// do not define their function. `merge` is the union of each set.
pub struct Facts { pub dependency: BTreeSet<Digest>, pub undefined_friends: BTreeSet<Digest> }
pub fn facts(input: &Input<'_>, read_only: ReadOnly<'_>) -> Facts;

// slip_cli::declare
pub struct Entity { pub kind: Option<KeyKind>, pub text: String }
pub enum Line { Relocate { entity: Entity, parent: Option<Entity>, name: Option<String> },   // None: the global namespace; `=`
                Changed(Entity), Created(Entity), Removed(Entity), Kept(Entity) }
pub enum ErrorKind { Syntax, NoUnit, Ambiguous, Fallback, NotRenamable, Duplicate, Refused,
                     DependencyEntity, Order }
pub struct LineError { pub line: u64, pub kind: ErrorKind, pub detail: String }
pub fn read(text: &str) -> Result<Vec<(u64, Line)>, LineError>;            // line numbers from 1
pub struct Needs { pub post: bool, pub post_mapped: bool }                 // which post-image parses the table asks for
pub fn needs(lines: &[(u64, Line)]) -> Needs;
pub struct Resolved { pub rules: RuleTable, pub changed: Vec<Digest>, pub removed: Vec<Digest> }
pub fn resolve(lines: &[(u64, Line)], pre: &[Record], post: &[Record]) -> Result<Resolved, LineError>;   // no facts
/// `facts`: the pre-images' (`slip_digest::records::facts`, merged), for
/// `DependencyEntity` and the rules of undefined friends.
pub fn resolve_with(lines: &[(u64, Line)], pre: &[Record], post: &[Record], facts: &Facts) -> Result<Resolved, LineError>;
/// `post_mapped`: the post-images' records under `resolved.rules`.
pub fn finish(lines: &[(u64, Line)], resolved: Resolved, post_mapped: &[Record]) -> Result<Declared, LineError>;
```

## 3. What is compared

**Shards.** The pre shard is the observer's shard of phase `map`; the post
shard is its shard of phase `verify`, round `r` (the `r`-th application of
edits). Which round the pre shard is of is a parameter of the job (§5.2
step 0). Both must be `ok` and carry a `digests` section of the same
`scheme`, the one this build of `slip_verify` implements; and the post
shard's digests must have been computed with the rule table `rules_id`
(§5 records it). In each of the two `digests` sections the groups must be
strictly ascending by `(class, file)` and `tu` must be `tu_digest(groups)`
(`digest.md` §11): §4.1 step 1 trusts `tu` and does not recompute it. The
scheme is checked for the pre then the post image, then the other two
conditions per image, pre first.

**Units as a multiset.** The units of an image are the triples `(key,
digest, flags)` of all its groups; a triple that occurs in `n` groups has
multiplicity `n`. (Only a unit keyed by content can occur in two groups of
one class: the same `static_assert` written in two files.) Declared units
and the units of the `tu` group are compared together as the **declared
multiset** `D`; derived units as the **derived multiset** `V`.

## 4. Comparison

Let `Dp`, `Dq` be the declared multisets of the pre and post shards.

1. **Indeterminate.** Every key that has a unit with the indeterminate
   flag in `Dp` or in `Dq` is one finding `indeterminate`, also when the
   key has units on one side only. The key takes no further part, no
   exemption applies to it, and it is never in `used` (§6).
2. **By key.** For each other key `k`, let `P(k)` and `Q(k)` be the
   multisets of digests of that key in `Dp` and `Dq`, and let
   `R(k) = P(k) − Q(k)` and `A(k) = Q(k) − P(k)` be their differences as
   multisets: what only the pre-image has of the key, and what only the
   post-image has.
   - both empty: no finding.
   - both non-empty: finding `changed`, unless `k` is in `changed`.
   - only `R(k)` non-empty: finding `removed`, unless `k` is in `removed`.
   - only `A(k)` non-empty: finding `added`, unless `k` is in `created`.

   Each exemption list prevents only its own kind: `changed` only
   `changed`, `removed` only `removed`, `created` only `added`. A
   prevented finding puts `k` in `used`; an exemption whose key had no
   finding of its kind is not used.

   A key normally has one unit on each side, and then this is the obvious
   rule: equal, different, gone, new. Several units of one key arise only
   from a content-keyed unit written in several files (equal digests, a
   multiplicity) and from merged namespaces that are not the same (unequal
   digests). Of two copies of one `static_assert`, deleting one is
   `removed`, not `changed`: the rule is stated on the differences so that
   what is equal on both sides never decides the kind of a finding (§4.1
   depends on that).
3. **Derived units.** Let `Vp`, `Vq` be the derived multisets. Every key
   that has a unit with the indeterminate flag in `Vp` or in `Vq` is one
   finding `derived_indeterminate`, and takes no further part. For each
   other key, with `R(k)` and `A(k)` as in step 2: if both are non-empty,
   finding `derived_changed`. If only one is, there is **no** finding; the
   key is counted (`verify.derived.only_pre` when it is `R(k)`,
   `verify.derived.only_post` when it is `A(k)`), once per key whatever
   its multiplicity. No exemption applies to a derived key, so `changed`
   on a key does not prevent `derived_changed`.

4. **Declared on one side, derived on the other.** Where step 2 would
   give `removed` for a key that has a unit in `Vq`, or `added` for a
   key that has a unit in `Vp`, the finding is `changed` instead, unless
   `k` is in `changed`. `removed` and `created` do not prevent it and
   are not used by it, and step 3 does not count the key in `only_post`
   or `only_pre` (a `derived_indeterminate` or `derived_changed` of
   step 3 stands beside it). The two units are not compared: one is
   what was written and the other what the front end made, and their
   streams differ by construction. "Has a unit in `Vq`" is read on the
   differences, as steps 2 and 3 are: `Vq(k) − Vp(k)` is not empty (for
   `added`, `Vp(k) − Vq(k)`), with `Vp(k)` and `Vq(k)` the multisets of
   digests of the key in `Vp` and `Vq`, so that §4.1 holds for it. On
   what the walk produces the two readings are one: a key with a
   determinate declared unit has no derived unit in the same image,
   since a second entity of one key is a collision (`digest.md` §3.4),
   so `Vp(k)` is empty where step 2 gives `removed`. The walk does not
   mark two runs as a collision (`crates/slip_digest/src/units.rs`
   `collides`), and neither breaks this: a run made only of namespace
   units, since a namespace is always in the `tu` group (`digest.md`
   §11), so a namespace key is never in `V`; and a run of the kinds a
   scope may repeat (a using-declaration, a `static_assert`, a friend
   and the others of §3.4) whose digests are all equal, since such a
   unit is keyed by its content under its parent, and a derived one has
   an implicit instantiation for its parent, which no declared unit has
   (an explicit specialisation makes its members declared). Were such a
   key in `D` and `V` of one image all the same, the reading on the
   differences is still the one that holds through groups, and a unit
   of `V` that both images have equally is no difference to report. (WP-108: read as
   "`Vq` has a unit of the key", step 4 gives `changed` over all groups
   and `removed` through `G` for a key whose derived unit is the same in
   a group outside `G`.) `pre`, `post` and `file` are those of §6 for a
   finding about `D`: one of `pre` and `post` is present.

Steps 1 and 2 are over `D` and step 3 over `V`, independently: a key set
aside in one is not thereby set aside in the other. (A key is in both
only if an entity is declared in one image and derived in the other, or
by a collision.) Step 4 is the one place they meet.

**Why step 4.** Every reference to an entity is written by its key. An
explicit specialisation `template <> int pick<int>() { return 2; }` and
the implicit instantiation of `template <class T> int pick() { return
1; }` for `int` have one key. Remove the explicit specialisation and
every caller of `pick<int>()` now calls another body, and no caller's
digest moves: the key it wrote is the same. Without step 4 the declared
unit would be `removed`, which a tool that deleted the specialisation
on purpose declares, and the instantiation would be a derived unit on
one side only, counted and not reported: `verified`, for a program that
returns another value. The argument below ("a derived unit on one side
only is not a difference") holds for a key that the other image does
not have at all, and that is all it is used for now. The exemption that
fits is `changed`, and it says the right thing: this entity is in both
programs and is not the same.

The observer is **verified** when there is no finding.

**Why a derived unit on one side only is not a difference.** A derived unit
exists because something demanded it (`digest.md` §2.2): a reference, an
odr-use, a constant evaluation. That demand is written in the stream of the
unit that made it, as a reference to the derived unit's key or as the
expression that forced it. If the post-image has an instantiation the
pre-image lacks, then some unit of the post-image refers to it. That unit
is either declared (it differs from its pre-image counterpart, which cannot
refer to something that does not exist there, or it is `added`), or it is
itself derived and present only in the post-image, and the argument repeats
until it reaches a declared unit, as it must: nothing derived exists
without a chain of demands that starts at something written. So a
difference in the set of derived units always comes with a finding on a
declared unit, and reporting the derived unit too would only add a second
finding with less to say. (An indeterminate derived unit is reported even
on one side only: nothing can be argued from a digest that cannot be
trusted.) What must not be skipped is a derived unit that
both have and that differs: the same specialisation instantiated to
something else is exactly how a dependent name that resolves differently
shows up ([03](../docs/design/03-equivalence.md) §7 row 6), and no declared
unit need differ then.
The same holds of what an instantiation's definition adds: the fields,
bases and other members of an instantiated class, a function's body, a
variable's initialiser are rows of the entity's resolved unit
`definition`, a derived unit (`digest.md` §2.2), so a field or base that
differs shows as a difference of a derived unit both parses have, not
of the declared unit of the instantiation or of its class.

One kind of derived unit exists without a demand: a resolved
`exception_spec` (`digest-kinds.md` §11.2), which Kiln also resolves when
it defines the function, so a destructor declared `noexcept(false)` on
one side and with no specification on the other, never used, has the unit
on one side only and no declared unit differs. It is safe to skip because
every construct that reads a specification (a call, `noexcept`, forming
the function's type or address, a definition) makes
Kiln resolve it on both sides, so wherever the translation unit observes
the difference the unit is on both sides and differs
(`crates/slip_digest/tests/differ/_/resolved-spec-observed-both-sides`,
`resolved-spec-addr-deferred`, `resolved-spec-addr-deferred-dtor-member`).

This argument assumes the front end's demand is a function of the program.
The demand-independence test of `digest.md` §14 is what checks it.

### 4.1 Through groups

Unit lists are not normally in a shard. The comparison is carried out on
group sums first (`digest.md` §11):

1. If the two `tu` digests are equal, the unit multisets are equal, flags
   included. If, further, no group has an indeterminate unit (every
   `GroupSum.indeterminate` is 0), there is no finding and the observer is
   verified.
2. Otherwise let `G` be the groups, identified by `(class, file)`, whose
   sum is not the same in both shards (a group present in one shard only
   included), together with every group of either shard that has an
   indeterminate unit. The rules of §4 are applied to the units of the
   groups of `G` alone.

A group's **sum** is `(count, indeterminate, digest)`. The sums of one
`(class, file)` are compared as a multiset, so the computation of `G`
needs the groups in no order: a `(class, file)` present twice in one image
only is in `G`, and the comparison of units then rejects it (§4.2). With
equal `tu` digests `G` is the groups with an indeterminate unit. `G` is
ascending by `(class, file)`, no two equal.

Restricting to `G` is exact, not an approximation: it gives the same
findings, the same counts and the same used exemptions as applying §4 to
all units.

- A group outside `G` has no indeterminate unit. So every indeterminate
  unit of either image is in `G`, and the keys that steps 1 and 3 set
  aside are the same keys.
- A group outside `G` has the same units, with the same multiplicities, in
  both images (§3 counts a unit once per group for this reason). For every
  other key, leaving those groups out removes the same multiset `X` from
  `P(k)` and from `Q(k)`, and `(P − X) − (Q − X) = P − Q`: `R(k)` and
  `A(k)` do not change, and they are all that steps 2 and 3 look at.
- Step 4 looks at `R(k)` and `A(k)` of `D` and of `V`, which do not
  change either. The group shortcut cannot skip such a key: step 2 has a
  finding for it, so a declared or `tu` group holding it differs and is
  in `G`; and since the other image's `V` has a unit of it that this
  image's has not, a derived group holding that unit differs too and is
  in `G`. The derived groups of `G` are opened (their unit lists given)
  for this as for step 3: to tell `removed` from `changed`.

The second point is why §4 is stated on differences. A rule that asked
"does the post-image have the key at all" would give a different answer
inside `G` for a key that also has a unit outside it. An entity whose first
declaration moved from one file to another is in two groups of `G`, once on
each side, and compares equal.

Step 2 needs the unit lists of the groups of `G` from both images. §5 says
how the verify job obtains them.

### 4.2 Preconditions

Groups whose units are compared (§4.1 step 2, or all groups) are checked
first, and inputs that break a precondition are rejected, never compared.
Per image, the pre-image first:

1. No `(class, file)` is given twice (`DuplicateGroup`).
2. Then for each group in the order given:
   - its unit list is present (`MissingUnits`);
   - the units are strictly ascending by `(key, digest)` (`digest.md` §11)
     (`UnitsNotAscending`; this covers duplicates, and two units of one key
     and digest that differ only in flags);
   - every unit's `flags` is 0 or 1 (`BadFlags`: an unknown flag bit
     would otherwise be ignored by §4, which compares digests only);
   - `count` is the number of units (`CountMismatch`);
   - `indeterminate` is the number of units with the flag
     (`IndeterminateMismatch`);
   - `digest` is `group_digest(units)` (`DigestMismatch`).

Groups need not be given in order. A group with an empty unit list
(count 0) is accepted. Each error names the group's `(class, file)` and
the image it came from. These checks are why a group's sums are checked
against its units wherever there is a unit list (§8 "Sums are believed").

### 4.3 Interface

`slip_verify` (§3, §4, §4.1, §6), re-exported at the crate root from
`slip_verify::compare`:

```rust
pub struct Exemptions { /* changed, created, removed: Vec<Digest> */ }
impl Exemptions {
    pub fn none() -> Exemptions;
    pub fn new(changed: Vec<Digest>, created: Vec<Digest>, removed: Vec<Digest>)
        -> Result<Exemptions, String>;          // Err: a list not strictly ascending
    pub fn changed(&self) -> &[Digest]; // and created(), removed()
}

pub fn comparable(pre: &Digests, post: &Digests) -> Result<(), String>;   // §3

pub enum AtSums { Equal, Differing(Vec<(GroupClass, Vec<u8>)>) }
pub fn at_sums(pre: &Digests, post: &Digests) -> AtSums;                    // §4.1

pub struct Comparison { pub findings: Vec<Finding>, pub only_pre: u64, pub only_post: u64,
                        pub used: Vec<Digest> }
pub enum CompareError {   // every variant: { class: GroupClass, file: Vec<u8>, post: bool }
    MissingUnits, UnitsNotAscending, BadFlags, CountMismatch, IndeterminateMismatch,
    DigestMismatch, DuplicateGroup,
}
pub fn compare_groups(pre: &[&Group], post: &[&Group], exemptions: &Exemptions)
    -> Result<Comparison, CompareError>;                                   // §4, §4.2

pub fn verdict(comparison: Comparison, pre_shard: Digest, rules: Digest, limit: usize) -> Verdict;
pub fn verified(pre_shard: Digest, rules: Digest) -> Verdict;
pub fn unverifiable(reason: &str, pre_shard: Digest, rules: Digest) -> Verdict;
```

and `compare::REASON_MAX` (4096) and `compare::NO_REASON` (§6).
`CompareError` implements `Display` and `Error`; `post` says which image
the group came from. `compare_groups` walks keys in order over the
occurrences sorted by `(key, digest)`, so the work is linear after
sorting; `R(k)` and `A(k)` are computed by merging the key's two runs,
which are sorted by digest.

The verify job (§5) uses them as: `comparable`; then `at_sums`; `Equal` is
`verified(…)`; `Differing(G)` is `verdict(compare_groups(groups of G from
each image, …), …)`. Every finding therefore has a key.

## 5. The verify job

One job per observer and round, run by a worker (`spec/exec.md` §5, §8.4),
given the observer, the round, the **image** (§5.1), `Declared`, and two
numbers: `limit`, the findings a verdict keeps (default 64), and `locate`,
the findings that are named and localised (default 16; §6.1). The image,
the `Declared` and the round of the pre shard travel in the job
(`Job.post`, `spec/exec.md` §5.2).

### 5.1 The post-image

A post-image has one of two forms. The **tree form** (M1) is a set of
directory trees beside the subject; it is specified first, and what it
says about roots, the post command and guards G1 and L1 is about that form only.
The **overlay form** (M2; the overlay of
[02](../docs/design/02-pipeline.md), which needs Kiln's request KF2) is
specified at the end of this section. The steps of §5.2 are the same for
both.

```
Image   := list<RootMap>              sorted by `subject`
RootMap := subject: path, post: path
```

- Each `subject` and each `post` is the real path of an existing directory
  other than `/`. No root of an image, subject or post, equals another
  root of it or lies under one (`/a/b` lies under `/a`; `/a/bc` does not),
  except that a post root may lie under a subject root (its own or
  another's). Every root is UTF-8, since it becomes part of a command line
  Kiln reads, and no `post` contains `=` (the post command below). An
  image that breaks one of these is refused before any job runs
  (`spec/exec.md` §8.7).
- An image with no root map is the **identity**: the post-image is the
  subject as it is on disk now.
- **Under a root** is decided by file identity, never by spelling. A
  directory is under a root when it, or one of its ancestors, is the
  root's directory, compared by device and inode; the nearest such
  ancestor decides, so a directory under a post root that lies under a
  subject root is under the post root and not under the subject. A file
  is under a root when its directory is: a file's own inode is never
  compared (a post tree made of hard links to the subject's files is
  legitimate). A path is resolved as the kernel resolves it, component
  by component, symbolic links followed; it stops at the first component
  that does not exist, since what does not exist cannot change where the
  path leads. A root whose identity cannot be read is compared as bytes.
- The file whose real path is `s/x`, for a subject root `s` with post root
  `p`, is in the post-image the file `p/x`. A post root holds the whole
  tree: a file that is not in it is a file the edit deleted. A file under
  no subject root is the same file in both images.
- The **subject path** (`digest.md` §11) of a real path `p/x` is `s/x`; of
  any other real path, the path itself.

**The post command** is the observer's `Command` (`spec/model.md` §7)
with `argv[0]` and `env` kept and:

- **Path-bearing words** resolved. A path-bearing word is an input (a
  word that does not start with `-` or `@` and is no option's value), or
  the path in the value of `-I`, `-iquote`, `-isystem`, `-idirafter`,
  `-cxx-isystem`, `-include`, `-include-pch`, `-isysroot`, `-F`, `-B`
  (joined or separate), `--sysroot` and `-resource-dir` (separate or after
  `=`), `--embed-dir=` and `--gcc-toolchain=`. The value of any other
  option (`-D`, `-o`, `-x`, `-Xclang` and the rest of Kiln's options that
  take a separate value) is not one. The path's **leading parts** are
  resolved against `directory` (against `/` for an absolute path), by
  real path, shortest first: for a relative path the empty one (the
  directory itself) first, then each one that ends at a component. The
  first that is in the subject, by identity (a directory under a subject
  root; for the whole path, a file whose directory is), is the part that
  **reaches the subject**, whatever its spelling (`../s` from a
  directory outside every root, another letter case on a file system that
  ignores case, a symbolic link to the subject). It is replaced by the
  same place in the post tree: written relative to the post command's
  `directory` (by its real path) when the path is relative, absolute when
  it is absolute. **The rest of the path is kept byte for byte**: `../s/./inc`
  becomes `../p/./inc`, so two words that reach one directory by two
  spellings stay two spellings. A path that is unchanged by this is kept
  as written. When the rest has a `..` component, the path is instead
  resolved whole to where it ends (in the post tree when that is in the
  subject) and written whole, relative for a relative path (kept as
  written, through the subject, when it followed a symbolic link there).
  Each path so rewritten gives one **map pair** `(post prefix, prefix as
  written)`: the new leading part and the one it replaced, each followed
  by `/` (both bare for a file named whole; an empty leading part stays
  empty); a path kept as written gives the identity pair of the path.
- `directory` resolved the same way (an absolute path; it gives no pair).
- In every other word, and in a path-bearing word that does not reach
  the subject, the **textual rule**: each occurrence of a subject root
  that is followed by `/` or by the end of the word replaced by its post
  root. Occurrences are taken from the left and do not overlap; no two
  roots match at one place, since no subject root lies under another.
- Then, after the last word, one word `-fmacro-prefix-map=<post
  prefix>=<prefix as written>` per map pair other than the identity, each
  once, with the pair `(<post>/, <subject>/)` of every root map among
  them, longest post prefix first (descending bytes, the order Kiln
  applies them in, below).

With the identity image the post command is the observer's command.

**Spellings.** The added words make the spelling of every file the
post-image parse reads the spelling that file has once the edit is
applied to the subject, which is the program the claim of §1 is about;
without them a constant computed from such a path (`sizeof(__FILE__)`)
would differ for every file of the tree. Kiln keeps the entries of
`-fmacro-prefix-map=` and `-ffile-prefix-map=`, the command's own and
the added ones together, sorted by their old prefix in descending byte
order (a stable sort), and replaces the old prefix of the first that is a
plain string prefix of the path (`kiln_source/src/manager.rs:691`,
`set_macro_prefix_map`; `:703`, `remap_macro_path`): of two entries that
both match, the longer. Every spelling of a file reaches the program
through that function: `__FILE__`, `__BASE_FILE__` (the main file's) and
`__FILE_NAME__` (its last component, after the map;
`kiln_pp/src/expand.rs:1300`), `__builtin_FILE()` and
`std::source_location::current().file_name()`
(`kiln_sema/src/builtins.rs:3619`, `:3741`), and a contract's location
(`kiln_sema/src/contracts.rs:219`). The name of a `#line` directive also
goes through it, though it is the source's text and not a path of
Slip's: G5 below.

The invariant the pairs keep, which G5 (§5.3) checks and refuses when it
cannot: **every spelling of a file the post parse can produce is mapped
to the spelling that file has when the post tree stands in the subject's
place.** A file's spelling is made from the path of the word that found
it (an include directory, the main file, `-include`) or from its
includer's spelling, so it begins with the post prefix of that route. Two
routes can reach one file (F6: `-I../s/inc` and the includer's directory
of an absolute main file); the file is then spelled through whichever
route loaded it first, in the post parse as in the real post-image, and
each route's own pair gives its real spelling, provided that no pair of
another route matches it first. That is what G5 checks: when one post
prefix is a prefix of another, the shorter one's pair applied to the
longer post prefix gives exactly the longer one's written prefix, so the
order in which Kiln tries them changes nothing. Keeping a relative word
relative is what makes this hold in the common case: `../p/` and `<p>/`
are not prefix-related. A word that reaches the subject through a
symbolic link or another letter case gets a post prefix another route
also has (`<p>/` for `-I/link/inc` with `link` a link to `s`, and for the
main file `s/a.cpp`) with another written prefix, and is refused.

A second invariant makes the first checkable: **every pair emitted has
its post prefix under a post root.** A prefix map rewrites every spelling
under its prefix, whatever route reached the file: another word kept as
written, an absolute `#include`, `-include`, `#embed`. Only a post root's
files are spelled through mapped words alone; a directory under no root
can be spelled by any of them. So a word spelled through the subject
whose path leaves it, by `..`, to a place under no root (`-I/s/../s2/inc`;
`-I../../ext` from a `directory` in the subject, when the post directory
is not as deep) gets no pair: the job is `unverifiable` under G5, naming
the word. This holds for every way a pair is made, since every
path-bearing word goes through the one rewrite: include directories
(`-I`, `-isystem`, `-iquote`, `-idirafter`), included files (`-include`,
`-imacros`), the main file, and a relative word resolved against the
directory. `directory` makes no pair: spelled through the subject and
leaving it, it is rewritten to where it ends. A relative word that names
the same place from the post directory is kept as written and makes no
pair either (the identity). A word that leaves one subject root into
another is rewritten into that root's post tree; its pair is then under a
post root, and it disagrees with that root's pair, so G5 refuses it by
the consistency check.

"Under a post root" is decided by real paths: a relative post prefix is
walked from the real path of the post command's directory, each component
resolved before the next, a `..` taken from a real path only; where a
component does not exist, the deepest existing ancestor decides, and only
when no `..` follows. Nothing is folded lexically.

The rewrite resolves a word by the subject's walk; the real post-image
walks the post tree in the subject's place. The two agree when every name
the walk looks up in a directory of the subject has the same kind in both
trees (directory and directory, file and file, missing and missing, links
with the same target). G5 checks this for the command's directory and for
every path-bearing word with a `..`: before the word's last `..` every
difference counts (the `..` of the real post-image is taken from where
the post tree's entry leads); after it, only a link in the subject (the
rewrite writes the place the subject's walk reached, its links resolved).
A `..` after a component that does not exist is not folded either: such a
word is kept and refused, since where it ends cannot be decided.

The textual rule rewrites what is not resolved: `-DROOT="/s/"` becomes
`-DROOT="/p/"`, and `-DROOT="/s"` is kept (the root is followed by `"`);
so is a subject root that happens to occur inside a longer path
(`/mnt/s/x` for the root `/s`), which then names nothing, or something
else. A word whose path the rewrite does not see (`-Wp,-I../s/inc`, an
option Slip does not list) is not resolved at all.

**Why a wrong `verified` cannot come of it.** What the tree form
establishes, for the parse of step 1, is: *the post-image parse neither
read a file nor looked for one in any directory under a subject root (and
not under a post root), "under" decided by identity; and every file
outside every root that both images read is the same bytes in both.* The
parse then saw exactly what a parse of the subject with the edit applied,
run with the observer's own command, would see: under the post roots the
post tree, which is that subject's tree; elsewhere the files the pre-image
saw. And, by G6, the subject path of every file the post parse read
under a post root is a real path of the subject, or a created path under
real directories of it: the links the parse followed under a post root
are the subject's, by name and target, and none of the subject path's
existing components is a link. Ownership (§2.1), which is of real paths,
is therefore decided for a post file as for the subject file it stands
for (§5.1 defines the post file of `s/x` as `p/x`, which means that only
where both trees have the same links along the route). The post tree
stands in the subject's place only for what is inside it: a route that
leaves a post root (`#include "../ext/k.h"`, a link `inc -> ../ext`, a
header's `..`) reaches what is beside the post tree, which is not what is
beside the subject when the two trees are not beside each other. O2 does
not see it (the file read there need not be in the pre-image's inputs),
nor G1 or L1 (it is under no subject root). By G6 (iii), every such route
ends where the same route from the subject ends, by real path, for every
place the parse looked at, so that what the parse found and what it did
not find outside the post tree are what the subject's edited tree would
find. A word of the command that leaves by `..` is rewritten to the
subject's place (above) and does not leave. The cost: a post tree placed
elsewhere than beside the subject (at another depth, or under another
parent) whose files or links reach outside it by a relative step is
refused; put the post tree beside the subject (`s` and `post` in one
directory), or reach the outside tree by an absolute path. Each half is
one guard of §5.3:

| Guard | What it establishes | Pinned by (`crates/slip_cli/tests/`) |
|---|---|---|
| G1 | no file the parse read (`inputs`, real paths) is under a subject root, by identity | `verify_job.rs` `g1_*`, `f3_case_insensitive_spelling`; `image.rs` `s5_1_under_subject_by_identity` |
| L1 | no directory of the search list, no directory that would hold a place a lookup tried (found or not, `#include` and `__has_include` in every form, `#embed` and `__has_embed`, `-include`), and no directory of a file read, as spelled, has its resolution look a name up under a subject root | `verify_job.rs` `l1_*`, `f3_symbolic_link_to_the_subject`; `crates/slip_kiln/tests/lookups.rs` for what the log holds |
| G6 | the subject path of every file the post parse read under a post root is a real path of the subject or a created path under real directories of it (the links along the routes are the subject's); and every place the parse looked at by a route that leaves a post root is the place the same route reaches from the subject | `verify_job.rs` `g6_*`; `verify.rs` `g6_link_in_subject_directory_in_post_end_to_end`, `g6_route_leaving_the_root_end_to_end` |
| G4 | no word is a response file, whose words Kiln's driver would read without Slip seeing them | `verify_job.rs` `g4_response_file` |
| G5 | every spelling of a file the post parse makes is mapped to its spelling in the real post-image: every pair's post prefix is under a post root, the map pairs are consistent, no prefix map of the command's own overlaps them, no `#line` name is mapped | `verify_job.rs` `f6_two_routes_to_one_file`, `f6s_two_routes_through_a_link`, `f6r_two_routes_no_edit`, `g5_command_prefix_map`, `g5_line_name`, `z1_word_leaves_the_subject`, `z2_word_leaves_the_subject_absolute_include`, `z_control_word_stays_in_the_subject`, `z_word_leaves_into_another_root`, `a3_post_directory_becomes_a_link`, `a3_link_deeper_in_the_word`; `image.rs` `s5_1_post_command_resolves`, `g5_command_prefix_maps`, `g5_word_leaves_the_subject`, `g5_missing_prefix_under_a_post_root` |
| O2 | every file outside every root that both images read has the pre shard's size and digest; and every file the map parse read, under no subject root, that the post parse did not read is still a regular file at its own real path, of that size and digest (an image other than the identity) | `verify_job.rs` `o2_outside_drift_at_the_sums`, `o2_found_outside_header_deleted*`, `o2_has_include_true_then_deleted*`, `o2_unread_*`, `o2_merged_inputs_pair`, `o2_input_merged_by_symbolic_link` |
| N1 | no file has appeared, outside every subject root, at a place where the map parse looked for one and found none (its negative inputs, `model.md` §8.6; an image other than the identity) | `verify_job.rs` `f4b_*`, `n1_*`; `crates/slip_kiln/tests/lookups.rs` `missed_*` for the places |
| G2, R2, G3 | the pre-image parsed at step 4 is the map phase's, and the post-image did not change during the job | `verify_job.rs` `g2_*`, `r2_*`, `g3_*` |

The rewrite is what makes a verdict possible, not what makes it safe: a
word it misses or spells wrongly leads the parse into the subject, and G1
or L1 refuses; a path that leads somewhere else outside every root (a
miss of the rewrite) names files that are the same in both images (O2),
and finds no file where the map parse found none (N1).
So `-DROOT="/s"` is safe not because G1 refuses something but because a
string becomes a file name only by naming a directory or a file to the
front end, which a lookup or a read then shows (L1, G1).

The rewrite and the guards resolve paths each with its own resolver, at
different moments: a directory swapped for another (a symbolic link
retargeted) between the parse and the guards' resolution is caught only by
G3, and G3 runs only when there is a finding to localise.

G4 refuses a post command with a response file even when the edit is
correct. It cannot arise from a plan: `slip_cdb` expands every response
file when it reads a compilation database (`spec/cdb.md` §4), so an
observer's command never holds an `@` word; only a command given to
`slip verify` by other means can.

**What remains outside.** A post tree written while step 1 runs is not
seen until G3, which runs only when there is a finding to localise. Kiln's
lookup log does not record a lookup of an absolute name
(`kiln_pp/src/resolver.rs:132`, `:161`): a failed `__has_include` or
`__has_embed` of an absolute name under a subject root is not seen (one
that succeeds is a read, which G1 sees). The log does not name the
`--embed-dir` directories; L1 takes them from Kiln's driver
(`Args::embed_dirs`) and treats every one as tried. The driver's own
probes of the toolchain (the GCC installation, the resource directory)
are not lookups; the directories they give are in the search list, where
L1 sees them, and a toolchain under a subject root is refused through
them. A directory the toolchain gives that does not exist is dropped by
Kiln's driver unseen: a header created there after the map phase is not
a negative input N1 checks (the command's own search directories are:
`model.md` §8.4). Nor are the driver's probes that are no `#include`
form: `-fprebuilt-module-path` and the standard module's directory
(`import`), and a PCH beside an `-include` (one found makes the unit
`rejected`, which is safe). A file system that ignores case can let O2
miss a file the two images spell differently. The command itself is the
plan's (§8, "The command").

Two holes are known and pinned by tests that wait (`#[ignore = "needs:
…"]`, `crates/slip_cli/tests/verify_job.rs`):

- **An absolute name looked for and not found** (F7 of the re-check):
  `#if __has_include("<s>/new.h")` with an edit that adds `new.h`. Kiln's
  log does not record it (Kiln `spec/pp.md`, "Lookup log"); Kiln has been
  asked to. `slip_kiln::Lookups` already lists an absolute name where it
  is, whatever the form, so L1 refuses it with no new code once the log
  holds it. Test: `f7_absolute_has_include`; the absolute name is then
  also a negative input (`crates/slip_kiln/tests/lookups.rs`
  `missed_absolute_name`, waiting too).

- **A path that resolves elsewhere while every recorded file still
  exists unchanged**: a symbolic link outside every root, through which
  the map parse found a file, retargeted after the map phase to a
  directory with another file of that name. The place was found, so it
  is no negative input; the file the map parse read is still there,
  unchanged, so O2's second part passes; and O2's first part compares
  only files both parses read, by real path, which the post parse's new
  file is not. Closing it needs the map shard to keep where each found
  lookup led (the place as written and the real path it reached),
  checked as N1 checks the negatives (work package WP-135). Test:
  `o2_link_to_a_found_header_retargeted`.

- **Two recorded inputs made one file by a hard link**: `ext/a.h`
  replaced after the map phase by a hard link of `ext/b.h`, of the same
  bytes (with `#pragma once`, the second include is then skipped). Each
  path is still its own real path, a regular file of the size and digest
  recorded, so O2 passes; and two inputs that are one file at the map
  phase are legitimate (a deduplicated store), so "no two inputs are one
  file now" would refuse good trees. Closing it needs the map shard to
  record which inputs were one file (WP-135). Test:
  `o2_limit_inputs_merged_by_hard_link_wp135`, which records today's
  answer, `verified`.

  But for these two, every way for the map parse's files to be no longer
  the ones a lookup reaches is refused: a file deleted, changed, made a
  link to another (its real path is no longer its own), or shadowed by
  one that appeared, in a directory that was absent or a duplicate of a
  listed one (O2, N1).

F4b of the re-check (a file outside both roots that appears after the map
phase and shadows one the map parse read, with an edit that compensates)
is closed by N1 (work package WP-117): `f4b_outside_header_added_after_the_map`.
The converse, a file outside the roots that the map parse read and that
is deleted (or changed) after the map phase so that the search now finds
another, with an edit that compensates, is closed by O2's second part
(WP-117's fix round): `o2_found_outside_header_deleted`,
`o2_has_include_true_then_deleted`.

**The overlay form.** The image is an overlay (`spec/edits.md` §7.1,
kind `OVLY`): for each file it names, by real path, the size and content
digest the file has in the pre-image and in the post-image, the
post-image's bytes being a blob in the store.

- The post-image is the subject with the content of those files
  replaced. No file is created, removed or moved: the paths of the two
  images are the same.
- **The post command is the observer's command**, unchanged. Nothing is
  rewritten and no word is added, so the paragraph on the textual rule
  does not apply.
- **The subject path of a real path is the path itself.**
- The front end is given the overlay
  ([KF2](../docs/requests/KF2-overlay-and-file-identity.md)): every read
  of a file the overlay names, by whatever spelling the file is reached,
  yields the post-image bytes; every other read goes to the file system.
  The shard's `inputs` then show, for an overlaid file, the size and
  digest of the bytes the front end was given.
- **Loading.** An overlay can name far more files than one observer
  reads. A job first gives the front end the overlaid files that are in
  the pre shard's `inputs` (every file of the overlay if the pre shard is
  not usable). If the parse then read a file the overlay names that was
  not given (an edit added an `#include`), the job gives those too and
  parses again, until no such file is left. Step 1 is the last of these
  parses; each gives strictly more of the overlay than the one before, so
  their number is bounded by the overlay's files, and the shard does not
  depend on it. A blob is checked against its digest when it is read; one
  that is absent or does not hash to its name fails the job
  (`spec/exec.md` §5.4).
- **What guards it.** G1 and L1 guard the tree form's rewrite and have
  no meaning here: there is no subject root, and reading a subject file that the
  overlay does not name is reading the post-image. In its place are the
  two guards O1 and O2 of §5.3 (with G4), over the `inputs` of step 1: an overlaid
  file was read with the overlay's content, and a file that is not
  overlaid has the content the map phase saw. A file in only one of the
  two `inputs` lists is not an error: an edit can change what is
  included, and the comparison of digests is what judges that.

### 5.2 Steps

0. Read the **pre shard**: the observer's shard of phase `map` and of
   round `pre`, from the job's store. `pre` is a parameter of the job
   (`Job.post.pre`, `spec/exec.md` §5.2): 0 for `slip verify`
   (`spec/exec.md` §8.7), and the run's own round for the verify phase of
   a transform's run, which maps and verifies in one round
   (`spec/edits.md` §8). It is *usable* if it is there, is `ok` and has a
   `digests` section.
1. Parse the post-image: the parse of `spec/model.md` §8.4 with the post
   command, digested as `spec/model.md` §8.5 says with `Declared.rules`,
   the read-only files of §2.1 (`ReadOnlyFiles` of `Declared.files`, each
   buffer decided by the subject path of its real path; step 6's walk the
   same), the subject paths of §5.1, and detail `differing` from the pre shard's
   group sums (`digest.md` §11; detail `none` if the pre shard is not
   usable). With an image of the overlay form this is the last parse of
   "Loading" (§5.1). If the parse is not `ok`, the shard has that
   outcome, no `digests` and no `verdict`: the observer is `not_parsed`
   (§7), and the job ends. In the same parse, after the walk: the rule
   use of §9.1 when `Declared.rules` is not empty, and the checks of
   §10.2 when the overlay has obligations.
2. Check, in this order: the pre shard is usable; guards G1, L1, G4, G5, G6
   and, with an image other than the identity, O2 then N1 (tree form), or
   guards O1, O2 then N1 (overlay form); `comparable` (§3). The first that fails makes
   the verdict `unverifiable` with the reason of §5.3, and the job ends.
   L1 reads the lookups of the step 1 parse (Kiln's lookup log, asked for
   by `slip_kiln::ParseOptions::lookups`).
3. Compare group sums (`at_sums`, §4.1). If they are equal and no group
   has an indeterminate unit, the verdict is `verified` and the job ends.
4. Otherwise, with `G` as §4.1 gives it, parse the **pre-image**: the
   observer's own command, digested with no rules, real paths as subject
   paths, and detail for the groups of `G`. Check, in this order: the
   parse is `ok`; guard G2; its `digests`, unit lists apart, are the pre
   shard's. The first that fails makes the verdict `unverifiable`.
5. Apply §4 to the units of the groups of `G` from the two parses
   (`compare_groups`, with the three lists of `Declared`), and make the
   verdict from the comparison with `limit` (§6). A `CompareError` here
   is a defect of the walk, not a fact about the subject: the job fails
   (`spec/exec.md` §5.4), it gives no verdict.
6. If there is a finding and `locate` is not 0, name and localise the
   first `locate` findings (§6.1). This parses the post-image once more.

The parses of a job run one after another; no two are in memory together.
A job whose observer verifies at the sums parses once; one that does not
parses twice, or three times when it localises.

Steps 0 to 5 decide the verdict's `kind`, `total`, counts and `used`, and
each finding's `kind`, `key`, `file`, `pre` and `post`. Step 6 fills
`name` and `where` and can change nothing else, with one exception that
only withdraws: guard G3.

### 5.3 Guards and reasons

| | Checked | `reason` of the `unverifiable` verdict |
|---|---|---|
| P1 | step 2: the pre shard is in the store | `pre-image: no map shard` |
| P2 | step 2: it is `ok` | `pre-image: the map shard is <status>` |
| P3 | step 2: it has a `digests` section | `pre-image: the map shard has no digests` |
| G1 | step 2, tree form: no entry of the post-image parse's `inputs` is under a subject root (§5.1, by identity) | `post-image: read <path>, a file of the subject` (the first such entry) |
| L1 | step 2, tree form: the resolution from the post command's `directory` (§5.1) of each directory of the search list, of the directory that would hold each place a lookup tried, and of the directory of each file read as the front end spelled it, looks no name up in a directory under a subject root, except the name of a post root in it | `post-image: searches <dir>: <subject dir> is a directory of the subject`; `post-image: looked for <name> in <dir>: <subject dir> is …`; `post-image: read <name>: <subject dir> is …` (the first, in that order; `<subject dir>` the first directory of the subject the resolution looked in) |
| G6 | step 2, tree form other than the identity, after G5 and before O2, with or without the run's files: (i) for every file the post-image parse read, resolving the path as the front end spelled it (from the post command's `directory`), every symbolic link followed at a place under a post root (`p/D/n`) is, at `s/D/n` in the subject, a symbolic link with the same target bytes; (ii) for the real path `p/Y` of each file read under a post root, its subject path `s/Y`, resolved in the subject component by component up to the first component that does not exist, has no symbolic link among its existing components; (iii) for every place the parse looked at, as the front end spelled it from the post command's `directory` (each directory of the search list, each location tried, found or not, each file read), whose walk in the file system (component by component, symbolic links followed) is under a post root and then under none (*leaves the post root*), the same route walked from the subject (from the subject's counterpart `s/D` of the first directory `p/D` under a post root the walk was in, with the components it had left there) ends at the same place. A place is the real path the walk reached with the components it could not resolve (from a missing component, or one under a file); the post walk's place is taken to the subject when it is under a post root (a route that leaves and comes back in must come back to the subject's counterpart). A walk that follows more than 40 symbolic links is refused. A link with an absolute target passes (i) only with the same bytes in both trees; with other bytes it is refused, and must be: in place of the subject the post tree's link names what it names now, which is not the subject's | (i) `post-image: <spelled path> follows the link <p/D/n>, which is <a directory, a file, missing, or a link to <target>> in the subject`; (ii) `post-image: <p/Y> stands for <s/Y>, which the subject reaches through the link <s/…>`; (iii) `post-image: <searches <d> \| looks for <name> in <d> \| reads <name>>, which leaves the post root: <place> here, <place> from the subject`, or `…, which leaves the post root: more than 40 symbolic links <here \| from the subject>` (the first; (i) over every file read, then (ii), then (iii) in the order of the places above) |
| G4 | step 2: no word of the post command after `argv[0]` is a response file (`@` and a name, as Kiln's driver reads one) | `post-image: the command names the response file <word>, whose words slip cannot examine` |
| G5 | step 2, tree form: the post command's map pairs (§5.1) are consistent: (a) none other than the identity has a post prefix that is empty or holds `=`; (b) when one pair's post prefix is a prefix of another's, the first applied to the second's post prefix gives the second's written prefix; (c) no `-fmacro-prefix-map=` or `-ffile-prefix-map=` of the command's own (alone, after `-Xclang` or `-Xpreprocessor`, in a `-Wp,` list) has an old prefix prefix-related to a post or written prefix of a pair other than the identity; (d) no `#line` name of the post parse begins with the post prefix of such a pair; (e), checked first: the post prefix of each pair other than the identity and the roots' is under a post root, by real paths; (f), checked last: the command's directory, and every path-bearing word with a `..`, walks names of the subject's directories that have the same kind in the post tree, and has no `..` after a component that does not exist | (a) `post-image: <word> is rewritten to <path>, which no -fmacro-prefix-map word can map back`; (b) `post-image: <word> and <word> spell the files under <post prefix> differently (<pair> and <pair>)`; (c) `post-image: <option word> maps <old>, which overlaps <prefix> of <word>; slip does not compose prefix maps`; (d) `post-image: the #line name <name> begins with <post prefix>, which the post command maps`; (e) `post-image: <word> is spelled through the subject and leaves it (<post prefix> is under no post root)`; (f) `post-image: <word, or "the directory <dir>"> walks <path>, which is <kind> in the subject and <kind> in the post tree` or `post-image: <word> has a `..` after <path>, which does not exist; slip cannot tell where it ends` |
| O1 | step 2, overlay form: every entry of the post-image parse's `inputs` whose path the overlay names has the overlay's `post` size and content digest | `post-image: <path> was not read through the overlay` (the first such path, in path order) |
| O2 | step 2, overlay form, and tree form other than the identity: (i) every entry of the post-image parse's `inputs` whose path the overlay does not name (tree form: that is under no post root) and the pre shard's `inputs` also list has the size and content digest the pre shard gives it; (ii), tree form, after (i): every entry of the pre shard's `inputs` that is under no subject root (`Resolver::under_subject`, judged on the path as recorded: when the nearest of the path and its ancestors that exists now is not its own real path, a link made since, the entry is taken as under no root) and whose path the post-image parse's `inputs` do not list (the same identity as (i): the path's bytes; both are real paths) is, now, links followed, a regular file whose real path is the path itself (the map parse recorded real paths) and of the size and content digest listed (the overlay form's is O1's and O2's own) | (i) `pre-image: <path> changed since the map phase` (the first such path, in path order); (ii) `pre-image: <path> is gone since the map phase (run `slip parse` again)` when no regular file is there, `pre-image: <path> now leads to <real path>, not the file the map parse read (run `slip parse` again)` when its real path is now another, `pre-image: <path> changed since the map phase (run `slip parse` again)` when its size or content differs or it cannot be read (the first such path, in path order) |
| N1 | step 2, tree form other than the identity, after O2, and overlay form: the pre shard has a `negatives` section (`model.md` §8.6), and no path of it at which a file now exists is, in the tree form, outside every subject root, judged by the file's real path now (links and `..` resolved; `Resolver::under_subject`, its directory by identity), not by the path as written (`s/../ext/h.h` is outside); in the overlay form none may exist. A file exists as Kiln's lookup decides it: the path, links followed now, names a regular file that can be opened (a directory, a dangling link, a device, a file that cannot be opened are none) | `pre-image: the map shard has no negative inputs (run `slip parse` again)`; `pre-image: <path> appeared since the map phase` (the first such path, in path order) |
| C | step 2: `comparable(pre, post)` | `digests: <its message>` |
| R1 | step 4: the pre-image parse is `ok` | `pre-image: no longer parses (<status>): <message>` |
| G2 | step 4: its `inputs` are the pre shard's `inputs`, entry for entry | `pre-image: <path> changed since the map phase` (the first path, in path order, whose entry is not the same in both lists or is in one only) |
| R2 | step 4: its `digests`, unit lists apart, equal the pre shard's | `pre-image: its digests are not those of the map shard` |
| G3 | step 6: the second parse of the post-image is `ok` and gives the `inputs` and the `tu` digest of the first | `post-image: changed during the job` |

`<status>` is the name of the status (`diagnosed`, `rejected`, `failed`);
`<message>` the outcome's message; a path is written as its bytes where
they are UTF-8, else with each other byte as `\xNN`. The reason is then
cut as §6 says.

- **G1, L1, G4, G5, G6 and O2** are the tree form's safety argument (§5.1, "Why
  a wrong `verified` cannot come of it"). With the identity image there
  is no root: G1, L1, G6 and G5 hold trivially, and O2 is not checked, since
  every change since the map phase is then the edit compared. G4 is
  checked with every image: Kiln's driver reads a response file from the
  disk as it is when the job runs, and nothing records what it read
  (`kiln_driver/src/rsp.rs`). A command from a plan names none (the plan
  expands them, `spec/cdb.md` §4).
- **O1 and O2** say that what was parsed is the post-image the overlay
  describes over the pre-image the map phase saw: the overlay reached
  the front end for every overlaid file it read, and no other file
  changed under the run. O2 is checked even when the comparison ends at
  the sums, where G2 is never reached. Its second part covers what the
  first cannot see: a file the map parse read that the post parse did
  not, because it is gone (the search then ended elsewhere: a header
  deleted with another behind it, a `__has_include` now false) or
  because the edit no longer includes it (then it must be unchanged, or
  the pre shard no longer describes the pre-image). The map parse
  recorded real paths, so a recorded path whose real path is now another
  (replaced by a link to another input: two files merged into one) is
  refused though its bytes are the same; and the subject filter is
  judged on the path as recorded, so a link made since into a subject
  root does not hide a recorded input. It reads and hashes
  only those files: none when the post parse reads the headers the map
  parse read.
- **N1** is O2's counterpart for what the map parse did not read: O2
  compares the files both parses read, N1 that no file appeared where
  the map parse looked and found none, so that a search now ending
  elsewhere (a file shadowing one the map read, a `__has_include` now
  true) is a drift. It is checked whatever the comparison finds, before
  it. A negative under a subject root is not checked: the tree form's
  post parse does not look under the subject (L1), and a file there is
  the subject's, which the pre-image parse of step 4 checks (G2); a file
  the edit adds is in the post tree. A negative under a post root, or
  reached through a link into one, is checked: the post parse reads the
  post tree, and the map parse did not find the file there. With the
  identity image it is not checked, as O2. A pre shard without the
  section (written before work package WP-117, or by another writer) is
  refused: `slip parse` writes it again.
- **G2 and R2** say that the pre-image parsed in step 4 is the one the map
  phase saw. A subject edited in place after the map phase (the identity
  image used to compare "the tree now" with "the tree as planned") can
  therefore be `verified` at the sums, and is `unverifiable`, never
  `mismatch`, when it is not: there is no pre-image left to compare units
  with.
- **G3**: if the post-image is not the same at the second parse, the job
  cannot say which image its verdict is about.

### 5.4 The post shard

The shard of phase `verify` and the job's round has the plan's observer
id; the outcome, `inputs` (real paths, in the post-image) and counters of
the step 1 parse; the counters of step 1's walk and those of §7; the
`digests` section of step 1 **without unit lists**; and the `verdict`.
Unit lists are not stored: while a corpus has indeterminate units every
group is in `G`, the lists would be the whole translation unit for every
observer, and nothing reads them. M2 adds three sections: `binding`
(§9.1) in every shard a verify job writes, whatever its outcome;
`rule_use` (§9.1) and `spellings` (§10.4) in an `ok` shard, when there
are rules and when there are obligations.

Its content is a function of the post-image's inputs, the pre shard,
`Declared`, the image, `limit` and `locate` (and of the pre-image's
inputs, which G2 ties to the pre shard).

### 5.5 Unchanged observers

An observer is **unchanged** in a round if the image changes no file at
all, or if (a) no file in the `inputs` of its pre shard is modified or
deleted and (b) the image creates no file. Its post-image is its pre-image
byte for byte; it gets no job and the verdict `unchanged`, whatever the
rules are: rules are a device for comparing, not a property of a program
that did not change. Condition (b) is there because a created file can be
found by an `#include` or a `__has_include` that found another file, or
nothing, before; a shard does not record the lookups that failed ("Open
questions").

**Not in M1.** Deciding "unchanged" needs the set of files an image
changes, which an overlay states and a tree does not. In M1 every selected
observer gets a job and no observer is `unchanged`; the state exists in §7
for M2. Invariants (a) and (b) are run with the identity image and no
rules (`spec/harness.md` §9).

**In M2**, with the overlay form, the files the image changes are the
overlay's, and it creates none: an observer is unchanged when no file of
the overlay is in the `inputs` of its pre shard. Two things limit that:

- With a rule table that is not empty, "whatever the rules are" holds
  for the observer's own verdict and not for the run: §9 needs to know
  that the observer has no entity a rule renames, and nothing in a pre
  shard says so. In such a run an observer that would be unchanged gets
  a verify job all the same (`spec/edits.md` §8.2). Its post-image is
  its pre-image; the job is there for its `rule_use`.
- An observer whose pre shard is not usable is never unchanged: there is
  no `inputs` to decide it from.

## 6. Verdicts and findings

Section 21 of a shard of phase `verify` (`spec/model.md` §8), present when
the outcome is `ok`:

```
Verdict := kind:      VerdictKind
           pre:       digest            content id of the pre shard compared against; zero if there was none
           rules:     digest            rules_id
           reason:    str               for `unverifiable`; else empty
           only_pre:  uv                derived keys counted by §4 step 3; 0 when the comparison ended at the sums
           only_post: uv
           used:      list<KeyId>       exemptions that prevented a finding, sorted
           total:     uv                number of findings
           findings:  list<Finding>     the first `limit` of them (default 64) in the order below

VerdictKind := verified = 0 | mismatch = 1 | unverifiable = 2

Finding := kind:  FindingKind
           key:   opt<digest>           always present in a verdict of this spec ("Open questions")
           file:  bytes                 below
           pre:   opt<digest>           below
           post:  opt<digest>
           name:  str                   the entity as a person would write it; for the report only
           where: opt<Where>            §6.1

FindingKind := changed = 0 | removed = 1 | added = 2 | indeterminate = 3
             | derived_changed = 4 | derived_indeterminate = 5
```

Findings are ordered by `(rank, file, key)`: `file` and `key` by their
bytes, and `rank` from the kind:

| Kind | Number in the format | Rank |
|---|---|---|
| `changed` | 0 | 0 |
| `removed` | 1 | 1 |
| `added` | 2 | 2 |
| `derived_changed` | 4 | 3 |
| `indeterminate` | 3 | 4 |
| `derived_indeterminate` | 5 | 5 |

- The findings that say a unit differs come before those that say a unit
  is unknown: a `derived_changed` finding is within the first `limit`
  (and the first `locate`, §6.1) however many indeterminate units the
  observer has.
- The rank is not the kind's number and is not written: the numbers of
  `FindingKind` in the format do not change. The decoder checks the order
  by rank (`spec/model.md` §8.3 rule 4b). No verify shard was written
  under the earlier order (by number): the verify job (§5) is the first
  writer.
- No two findings are equal in all three, so the first `limit` are
  determined.
- The report's groups (§7.1) use the same rank.

`pre`, `post` and `file` are defined on what §4 looked at, so that they too
are the same whether or not the comparison was restricted to `G`:

- For `changed`, `removed`, `added` and `derived_changed`: `pre` is the
  digest in `R(k)` when `R(k)` has exactly one element (counting
  multiplicity), else absent; `post` likewise from `A(k)`. `file` is the
  file of the first group, in the order of `(class, file)`, among the
  groups of the multiset the finding is about (the declared and `tu`
  groups for `D`, the derived groups for `V`) whose units, flags
  included, are not the same in both images, that holds a unit of the key
  in the pre-image; if there is none, in the post-image. (There is one: a
  key with a difference has a unit in a group that differs; were there
  none, it would be a defect of `slip_verify`, which stops with
  `unreachable!`.) A group that holds the key but does not differ is
  skipped even when it comes first. The group named need not be where the
  key itself changed: it can differ on account of another unit.
- A group **differs** when the other image has no group of its
  `(class, file)`, or has one whose unit list, flags included, is not the
  same. Groups are paired across the images by `(class, file)`; so given
  only the groups of `G` the comparison decides the same as given all
  groups, since a group outside `G` has the same units in both.
- For `indeterminate` and `derived_indeterminate`: `pre` is the digest of
  the key's indeterminate unit in the pre-image when there is exactly one
  such unit (counting multiplicity: one unit in two groups is two), else
  absent; `post` likewise. `file` is the file of the first
  group, in the order of `(class, file)`, that holds an indeterminate unit
  of the key, whether or not the group differs, in the pre-image if it has
  one, else in the post-image.
- Declared groups sort before the `tu` group (class order), so a key with
  units in both reports the declared group's file.

The comparison (§4.3) leaves `name` empty and `where` absent; they are for
the report and are filled by step 6 of the job (§6.1).

A verdict is made in one of three ways:

- **From a comparison**: `mismatch` if there is a finding, else
  `verified`; `total` is the number of findings, `findings` the first
  `limit`; `only_pre`, `only_post` and `used` as the comparison gives
  them, also for `verified`; `reason` empty.
- **At the sums** (§4.1 step 1): `verified`, `total` 0, no finding,
  `only_pre` and `only_post` 0, `used` empty, `reason` empty. It equals
  the verdict of an empty comparison.
- **`unverifiable`**: `reason` cut to at most 4096 bytes at the last
  character boundary at or before byte 4096; an empty reason (also one
  whose cut is empty) becomes `no reason given`, so that the verdict has
  the non-empty reason `spec/model.md` §8 requires. `total` 0, no finding,
  counts 0, `used` empty.

### 6.1 Names and localisation

Names and places are for people. They are outside the trusted base: no
verdict depends on them (§5.2), and a `Where` that points at the wrong
token is a defect of the report, not of the verification. They are made
for the first `locate` findings of a verdict, in its order; the other
findings keep an empty `name` and no `where`.

**Names.** A unit's name is the entity as a person would write it. Every
form the walk makes (`crates/slip_digest/src/names.rs`):

| Unit | Name |
|---|---|
| a namespace, class, enumeration, variable, alias, template, concept, member | its qualified name: `ns::C::m`, `(anonymous namespace)::x` |
| a function | its qualified name and its parameter types as Kiln prints canonical types, with `this ` before an explicit object parameter, then ` const`, ` volatile`, ` &` or ` &&` for a member's qualifiers, then ` requires ` and the trailing requires-clause as written: `ns::f(int, char *)`, `ns::C::m() const`, `C::f(this C &)`, `A::f() requires true` |
| a function template | its template head, its return type (none for a constructor, destructor or conversion function), then as a function, the parameter types as the pattern's: `template <class, int> void ns::f(type-parameter-0-0)` |
| a template head (of a function template, a deduction guide template, a constrained partial specialisation) | `template <`, each parameter by its kind, `>`, then ` requires ` and the head's requires-clause as Kiln writes it if it has one. A type parameter is `class`, or its type-constraint in place of `class`: the concept and the arguments after the first, `C`, `C<int>`; a non-type parameter its type; a template template parameter its own head and ` class`, ` auto` or ` concept`; `...` after a pack: `template <C2> void f(type-parameter-0-0)`, `template <class> requires C<T> void f(type-parameter-0-0)` |
| a deduction guide | `<deduction guide for S>` and its parameters as a function: `<deduction guide for S>(char)`; a deduction guide template with its head and return type as a function template |
| a friend function keyed in the class that befriends it (digest-kinds §3.3 `function`) | the class's name, `::friend `, then the function's: `A<int>::friend k(int)`, `C::friend template <class> void g(type-parameter-0-0)` |
| a partial specialisation | as a specialisation, with its pattern for the arguments; if its head has a type-constraint or a requires-clause, the head first, which is what tells two partial specialisations of one pattern apart: `S<type-parameter-0-0 *>`, `template <class> requires C2<T> S<type-parameter-0-0 *>` |
| a function template specialisation | the template's qualified name, its template arguments as a specialisation's below, then the parameter types and qualifiers as for a function: `legacy::take<int>(int)`, `legacy::log<legacy::Mine>(legacy::io::out &, const legacy::Mine &)` |
| a specialisation | the template's name and its canonical arguments as Kiln prints them, with every entity an argument names written by its qualified name (`ns::S<ns::T>`, never `ns::S<T>`: two classes `T` of two namespaces are two specialisations), and except that a null pointer argument whose type is not `std::nullptr_t` is written `(T)nullptr`: `ns::S<int>`, `N<(int *)nullptr>` |
| an unnamed class or enumeration | its scope and `(anonymous K for n)` when it is named by a declarator, typedef or member (digest §3.3), else `(anonymous K N)`, `N` its ordinal among the unnamed types of its scope, else `(anonymous K)`; `K` the class-key or `enum`: `(anonymous struct for P)`, `ns::(anonymous enum 1)`; its members are named under it |
| the unnamed object of an anonymous class (an anonymous union's member object) | its scope and `(object of ` the class's name `)`: `S::(object of (anonymous union 0))` |
| any other entity Kiln prints no name for (an unnamed bit-field) | its scope and `(unnamed N)`, `N` its ordinal (digest §3.3), or `(unnamed)` |
| the variable of a structured binding declaration | its scope and its bindings: `[a, b]`, `n::[a, b]`. The key is named by the first binding only (digest-kinds §5.2); the others are in the name for people |
| the holding variable of a tuple-like binding `a` | `a (holding variable)` |
| a name-independent non-static data member (digest-kinds §3.3 `field`) | its qualified name and ` (name-independent N)`, `N` its ordinal: `S::_ (name-independent 1)` |
| a using-declaration | its scope and `<using ` its qualifier and name `>`: `<using B::f>`, `D::<using B::B>` for an inheriting constructor |
| a using-shadow | its scope and `<shadow ` the plain name of its target `>`: `D::<shadow B::f(int)>` |
| a friend declaration | its scope and `<friend ` what it names `>`: `C::<friend f(int)>`, `C::<friend D>` |
| a using-directive, a using-enum | its scope and `<using_directive n>`, `<using_enum E>` |
| a `static_assert`, file-scope `asm`, `consteval` block | its scope and `<static_assert>`, `<asm>`, `<consteval_block>`. These are content-keyed (digest §3.3): two of one scope have the same name and cannot be named one by one, only by their scope; a finding on one is told apart by its `where` |
| a template parameter object | its scope and `<template_param_object>` |
| a `default_argument` unit (digest-kinds §11.4) | its function's or field's name and `::<default_argument n>`, `n` the parameter's index: `X<int>::f(int)::<default_argument 0>` |
| a resolved unit (digest-kinds §11.1) | its owner's name and `::<` the property `>`, with its discriminator after a space: `S::<key_function>`, `f(int)::<default_immediates 0>`, `X<int>::f(int)::<default_argument 0>::<default_immediates>` |
| the order unit | `<order>` |

The plain name of an entity a unit refers to (a shadow's target, what a
friend names) is its qualified name with a specialisation's arguments and
a function's parameters, or a function template's name as above.

A qualified name names every namespace on the way, an inline namespace
too (`legacy::v2::Mine`, never `legacy::Mine`: both can exist in one
image), although Kiln and other compilers leave inline namespaces out
of what they print.

Two units of one image with different keys have different names, except
the units of one colliding key (digest §10) and the content-keyed kinds
above: everything a key holds that the parts above would not show (a
requires-clause, a type-constraint, a null pointer argument's type, an
explicit object parameter) is printed. `crates/slip_digest/tests/determinism.rs`
`keys_and_names_are_unique` checks it over every pair test.

A requires-clause is printed as Kiln writes the expression of the
declaration the name is read from; for a member of an implicit
instantiation that is the pattern's, so its template parameters keep the
pattern's names: `X<int>::~X() requires (sizeof(T) == 4)`.

A finding's `name` is the name of the key's unit in the pre-image when the
pre-image has one, else in the post-image, where it is the name written
there (the new name of a relocated entity; the key is the mapped one). If
units of one key have different names (a collision, two namespaces merged
by a rule), it is the first in the order of their bytes. It is cut to 4096
bytes at a character boundary. `slip declare` (§2.3) resolves the same
names.

**Traces.** For a list of keys the walk can produce, for each unit that
has one of them, a **trace**:

```
Trace := key: KeyId, digest: digest
         stream: bytes                the unit's stream (digest.md §4), as hashed
         marks:  list<Mark>           ascending by offset
         refs:   list<RefAt>          ascending by offset, not overlapping
Mark  := offset: uv                   the stream's length when the walk began to write the node
         node:   str                  the node's kind: the variant's name in Kiln's `ExprKind` or
                                      `StmtKind`; for a unit's first mark, the name of its key kind;
                                      `declarations` for a declarations mark (below)
         file:   bytes, line: uv, column: uv      subject path, line and column of the node's location in
                                                  the physical file (a `#line` directive does not
                                                  move it); empty, 0, 0 if it has none
RefAt := offset: uv, len: uv          a digest field of the stream (32 bytes) whose value is
         key:   KeyId                 the key of a unit of this image: a reference to it
```

- A mark is made where the walk begins a unit (offset 0, at the unit's
  **place**, below) and where it begins a statement or an
  expression inside one, and a **declarations mark** where a
  declaration's stream (`digest.md` §5.1) ends its `DeclNode` rows and
  begins the rows merged over its entity's declarations (scope,
  attributes, template maps, module rows), at that entity's place. A
  difference in those rows is at the declarations mark: at the entity,
  not at the redeclaration that differs. The `DeclNode` rows themselves
  (the declaration's type, its specifiers) have no mark: a difference
  there is at the unit's mark or at the last expression before it. Types and constant
  values are written as digests (`digest.md` §4.2) and have no mark of
  their own: a difference inside one is at the mark of the node that
  holds it. The same holds for any part of a unit the walk hashes apart
  and writes as a digest.
- A trace is made by writing the unit's stream **again**, after the walk,
  with the marks recorded. It is a trace only if the bytes written the
  second time hash to the unit's digest; otherwise the unit has no trace
  and its finding no `where` (counted: `verify.traces.unstable`, §7.2),
  which is also a defect of the walk to report: a unit's stream must not
  depend on when it is written. The walk that produced the digests is not
  touched by tracing.
- A panic in the second writing withdraws places and nothing else: the
  digests, the verdict and every finding stand; the traces made before it
  are kept; the walk traces nothing more on that image (its state is not
  trusted after the unwind), so the image's remaining wanted units have no
  trace and their findings no `where` (counted:
  `verify.traces.panicked`, §7.2, and not in `verify.traces.unstable`).
- A content-keyed unit and the order unit have the first mark only.
- The location of a node inside a macro expansion is that of the outermost
  expansion (as for a group's file, `digest.md` §11).
- Tracing changes no byte of any stream: with and without it, `digests` are
  the same, and every unit of every pair test's sources has a trace whose
  stream hashes to its digest. `crates/slip_digest/tests/trace.rs` asserts
  both.

**The site of a difference.** For a `changed` or `derived_changed` finding
whose `pre` and `post` are both present, take the trace of the unit with
that digest in each image, and let `d` be the first offset at which the
two streams differ (the length of the shorter if one is a prefix of the
other). In each trace the site is the last mark whose offset is at most
`d`:

```
Where := pre:  Site, post: Site
Site  := file: bytes, line: uv, column: uv      of the mark; empty, 0, 0 if it has none
         node: str                               the mark's `node`
         what: str                               below
```

`what` is `refers to <name>` when `d` lies inside a `RefAt` of that trace,
with the name of the entity the reference is to in that image (its key in
64 hexadecimal digits if it has no unit there); else empty. So a call that
now reaches another function reads, on the two sides, `CallExpr … refers
to lib::f(long)` and `CallExpr … refers to app::f(int)`.

The last mark at or before `d` is the node the walk was writing, or one it
had just finished: a difference in a count or a flag that the enclosing
node writes after its children is reported at the last child. That is one
node off and in the right line or the next; it is what a list of marks can
say without recording where each node ends ("Open questions").

**Other findings.**

| Finding | `where` |
|---|---|
| `changed`, `derived_changed` with `pre` or `post` absent (several units of the key differ) | both sites from the units' first marks (below), `what` empty |
| `removed` | `pre`: the unit's first mark; `post`: the empty site |
| `changed` of §4 step 4 (declared in one image, derived in the other) | the first mark of the declared unit in its image, and in the other image the first mark of the derived unit (where the template's pattern is); `what` empty |
| `added` | `post`: the unit's first mark; `pre`: the empty site |
| `indeterminate`, `derived_indeterminate` | in each image that has an indeterminate unit of the key: the unit's first mark, with `what` the cause as the walk prints it (`unsupported ExprKind/CXXReflectExpr`, `collision`, `pending stmt`); the empty site in the other |

The empty site is an empty `file`, line and column 0, an empty `node` and
an empty `what`. A unit's first mark is the mark at offset 0. Where an
image has several units of the key, the one with the smallest digest is
taken.

A unit's **place**, the location of its first mark, is that of the first
declaration of its chain, in chain order, that the source wrote. A
declaration the front end made, by instantiation or implicitly, has no
place of its own and is skipped, also when it comes first in the chain:
an explicit specialisation `template <> int pick<int>()` in `pick_int.h`
whose chain begins with the declaration Kiln instantiated from the
template in `pick.h` has its place in `pick_int.h`. When no declaration
of the chain is written (a derived unit, an implicit member), the place
is that of the first declaration of the chain that has a location, which
for an implicit instantiation is where its pattern is (the row for step 4
above), and for a class template's explicit instantiation that is the
first to name the specialisation is the directive, where the front end
makes the declaration (`crates/slip_digest/tests/trace.rs`,
`s6_1_place_of_an_explicit_instantiation`).
This is not the finding's `file` (§6): that is the unit's group, the file
of the entity's first declaration including one Kiln made
(`digest.md` §11), and the two differ for such a specialisation
(`file` `pick.h`, place `pick_int.h:1`). Localisation reads the place
from the chain and not from `file`.

**When they are made.** The pre-image's names, traces and causes are taken
while the parse of step 4 is in memory, after the comparison (which needs
no syntax tree) has said which keys are wanted. The post-image's need its
syntax tree again: step 6 parses the post-image a second time, digests it
as step 1 did with detail `none`, checks guard G3 (§5.3), and takes its
names, traces and causes. With `locate` 0 there is no second parse and no
finding has a name.

**Interface.**

```rust
// slip_digest::trace
pub struct Mark  { pub offset: u64, pub node: &'static str, pub file: Vec<u8>, pub line: u64, pub column: u64 }
pub struct RefAt { pub offset: u64, pub len: u64, pub key: Digest }
pub struct Trace { pub key: Digest, pub digest: Digest, pub stream: Vec<u8>, pub marks: Vec<Mark>, pub refs: Vec<RefAt> }
pub struct Traced { pub output: Output, pub traces: Vec<Trace>, pub panicked: bool } // traces sorted by (key, digest)
/// `output` is `digest(input, options)`, byte for byte; `traces` has one
/// entry per unit whose key is in `keys` (sorted, no duplicates).
pub fn trace(input: &Input<'_>, options: &Options<'_>, keys: &[Digest]) -> Traced;

// slip_cli::locate
/// What localisation needs from one image, taken while its parse is in memory.
pub struct Side { /* the `tu` digest; names, causes and traces of the wanted keys */ }
impl Side { pub fn tu(&self) -> &Digest; pub fn unstable(&self) -> u64; pub fn panicked(&self) -> bool; }
/// One walk of `input` under `rules` for the keys of the findings to localise.
pub fn take(input: &Input<'_>, rules: &RuleTable, keys: &[Digest]) -> Side;
/// The keys of the first `locate` findings, sorted, no duplicates.
pub fn wanted(findings: &[Finding], locate: usize) -> Vec<Digest>;
/// Fills `name` and `location` of the first `locate` findings; changes no other field.
pub fn fill(findings: &mut [Finding], locate: usize, pre: Option<&Side>, post: &Side);
/// `verify.traces.unstable` and `verify.traces.panicked` (§7.2) of the two sides.
pub fn unplaced(pre: Option<&Side>, post: &Side) -> (u64, u64);
```

`pre` is `None` when step 4's parse was not reached with a finding to
localise; it cannot be, for a verdict with findings, and `fill` then
takes every name from the post-image.

## 7. The run

A run of the verify phase is over a plan, a round, an image, a `Declared`
and a selection of the plan's observers (`spec/exec.md` §8.7). Each
selected observer ends in one **state**:

| State | When |
|---|---|
| `verified` | its shard of this run is `ok` and has the verdict `verified`, and its map shard is `ok` |
| `unchanged` | it got no job (§5.5); not in M1 |
| `mismatch` | its shard is `ok` and has the verdict `mismatch`, and its map shard is `ok` |
| `not_parsed` | its shard is `diagnosed` or `rejected`, and its map shard is `ok`: the post-image does not parse where the pre-image did |
| `unverifiable` | its shard is `ok` and has the verdict `unverifiable`, whatever its map shard is |
| `never_parsed` | its shard is `diagnosed` or `rejected`, and its map shard is absent or not `ok`: nothing is known to have changed |
| `failed` | anything else: a shard of status `failed`, no shard, an `ok` shard without a verdict, a shard that is not this run's; a `verified` or `mismatch` verdict whose map shard is absent or not `ok` (the pre shard it compared against is not the one the store has now); a shard of another observer or round, or a verdict under rules other than the run's (`Report::add`, §7.3) |

**This run's shard.** The shard of an observer is read from the store and
is this run's if its content id is the `shard` of the observer's last
`Job` entry in the run's record (`spec/model.md` §9). A shard another run
left, or put there since, is not read as a result of this one.

The run has the verdict:

- **verified**: at least one observer was selected, and every selected
  observer is `verified` or `unchanged`;
- **not verified**: some observer is `mismatch` or `not_parsed`;
- **incomplete**: neither of those. A run over no observer is incomplete:
  nothing was checked.

Two conditions over all observers are added to this verdict by M2:
agreement between observers when there are rules (§9.3), and the spelled
entities when the image is an overlay with obligations (§10.3). A run
with no rule and no obligation is judged by the three lines above alone,
and only in a run whose rules are empty can an observer be `unchanged`
(§5.5).

A run that was interrupted has no verdict and no report.

A `mismatch` whose findings are all `indeterminate` or
`derived_indeterminate` says that Slip could not digest something, not
that the edit changed it. The report counts those observers apart
(`indeterminate_only`); the state, the run's verdict and the exit status
are the same either way, because an indeterminate unit is different from
every unit (`digest.md` §10).

### 7.1 The report

The report is a JSON document (`spec/model.md` §4), written where
`--report` says; the text on standard output says the same in fewer words.
The members and lines below are M1's and are all that a run with a tree
image and no rule has; §9.4 and §10.5 list those M2 adds, each present
only when it applies.

```
{
  "report": "slip.verify.v1",
  "plan": <id>, "round": <n>,
  "declared": <id of the DECL container>, "rules": <rules_id>,
  "image": [ { "subject": <path>, "post": <path> }, … ],
  "limit": <n>, "locate": <n>,
  "verdict": "verified" | "not_verified" | "incomplete",
  "observers": { "plan": <n>, "selected": <n>,
                 "verified": <n>, "unchanged": <n>, "mismatch": <n>, "not_parsed": <n>,
                 "unverifiable": <n>, "never_parsed": <n>, "failed": <n>,
                 "indeterminate_only": <n> },
  "findings": { "total": <n>, "listed": <n>, "groups": [ <FindingGroup>, … ] },
  "not_verified": [ { "observer": <id>, "main": <path>, "state": <state>, "message": <str>,
                      "findings": <n>, "indeterminate": <n>, "listed": <n> }, … ],
  "exemptions": [ { "list": "changed" | "created" | "removed", "key": <id>, "observers": <n> }, … ],
  "run": <path of the run record>
}

FindingGroup := { "kind": <FindingKind by name>, "key": <id>, "name": <str>,
                  "observers": <n>, "first": <main file of the first of them in plan order>,
                  "file": <path>, "pre": <id or null>, "post": <id or null>,
                  "where": <Where of spec/model.md §10, or null> }
```

- Ids are 64 hexadecimal digits; paths and names are rendered as
  `spec/model.md` §10 renders bytes and strings.
- `findings.total` is the sum of the verdicts' `total`; `listed` the
  number of findings the verdicts hold. A **group** is the listed findings
  of one `(kind, key)`; `observers` counts the observers that list it.
  `file`, `pre`, `post`, `name` and `where` are those of the first
  observer, in plan order, whose finding has a `name` that is not empty,
  else of the first. Groups are sorted by `(rank, name, key)`, with the
  rank of §6.
- `not_verified` lists, in plan order, every selected observer that is
  neither `verified` nor `unchanged`: its state; the verdict's `reason`
  (`unverifiable`), the shard's message (`not_parsed`, `never_parsed`,
  `failed`; empty if there is no shard; for a shard the report refuses
  as not this run's, §7 and §9.3, the reason it refuses it, never the
  shard's own message: `spec/landed/verify/WP-121.md` §4) or the empty
  string (`mismatch`);
  and for a `mismatch` the verdict's `total`, how many of them are
  `indeterminate` or `derived_indeterminate` (from the counters of §7.2)
  and how many the verdict lists; three zeros otherwise. An observer is
  `indeterminate_only` when `findings` equals `indeterminate`.
- `exemptions` has one entry for every key of every list of `Declared`,
  in the order `changed`, `created`, `removed` and then by key, with the
  number of observers whose verdict has the key in `used`. A verdict's
  `used` does not say which list a key was used from, so a key in two
  lists has the same number in both entries ("Open questions"). An entry
  with 0 observers is an exemption no observer needed.

The text, on standard output:

```
plan <plan id>
round <n>
declared <id> rules <rules_id>
image identity                                   or one line `image <subject> => <post>` per root map
selected <n> of <observers of the plan>
observer <state> <n>                             one line per state in the order of the table, zeros included
mismatch indeterminate_only <n>
finding <kind> <key, first 16 digits> observers <n> <name>          at most 100 groups, in report order
  first <main file>
  pre  <file>:<line>:<column> <node> <what>      the two `where` lines only if it has one;
  post <file>:<line>:<column> <node> <what>      a site with an empty file prints `-`
findings <total> total, <listed> listed, <groups> group(s)
exemption <list> <key, first 16 digits> observers <n>
wall <seconds, three decimals>
peak_rss <bytes> <main file>                     as `parse` prints it (spec/exec.md §8.2)
run <path of the run record>
report <path>                                    if --report was given
verdict verified | not verified | incomplete
```

After it, on standard error, one line per entry of `not_verified`, at
most 100 and then `slip: and <n> more (see the report)`:
`slip: <state>: <main file>: <first line of the message>`, the message of
a `mismatch` being `<findings> finding(s), <indeterminate> indeterminate`.

In the text and on standard error, every path and every string taken from
a shard (names, `node` and `what`, messages, reasons) is written as its
characters, except that each byte that is not UTF-8, each byte below
0x20, 0x7f, the backslash, and each byte of a C1 control character
(U+0080 to U+009F) is written `\xNN` (two lower-case hexadecimal
digits). So no path or name can write a line (`verdict verified`) or a
control sequence, and a `\` always starts an escape. The JSON report is
not escaped this way: its strings are JSON strings.

### 7.2 Counters

Counters of a verify shard, added to those of the parse and of
`digest.md` §12 for step 1's walk:

| Counter | Meaning |
|---|---|
| `verify.groups.differing` | groups of `G` (§4.1); 0 when the sums were equal |
| `verify.findings` | findings (`total`) |
| `verify.findings.changed`, `.removed`, `.added`, `.indeterminate`, `.derived_changed`, `.derived_indeterminate` | findings of each kind, all six always present when there is a verdict |
| `verify.derived.only_pre`, `verify.derived.only_post` | derived keys counted by §4 step 3 |
| `verify.exemptions.used` | exemptions that prevented a finding |
| `verify.reparsed` | parses after the first: 0, 1 (the pre-image) or 2 (and the post-image again) |
| `verify.traces.unstable` | units wanted for localisation whose stream, written again, did not hash to their digest (§6.1); present only when not 0 |
| `verify.traces.panicked` | images (0, 1 or 2) whose second writing for localisation panicked: places withdrawn there, verdict and findings kept (§6.1); a defect of the walk to report; present only when not 0 |

A shard without a verdict has none of them. The counters by kind are what
`indeterminate_only` (§7) is computed from: a verdict lists only its first
`limit` findings.

### 7.3 Interface

`slip_verify::run`, pure and part of the trusted base: the states, the
run's verdict and the report are what a person reads as "verified".

```rust
pub enum State { Verified, Unchanged, Mismatch, NotParsed, Unverifiable, NeverParsed, Failed }
impl State { pub const ALL: [State; 7]; pub fn name(self) -> &'static str; }

/// §7: `verify` is this run's shard of the observer (None: there is none, or
/// it is not this run's); `map` the status of its map shard (None: absent).
pub fn state(verify: Option<&Shard>, map: Option<Status>) -> State;

/// Re-exported: `slip_model::result::RunVerdict` (spec/edits.md §8.5), the
/// one type, `Verified = 0, NotVerified = 1, Incomplete = 2`.
pub use slip_model::result::RunVerdict;
impl RunVerdict { pub const ALL: [RunVerdict; 3]; pub fn name(self) -> &'static str; }
pub fn run_verdict(states: &[State]) -> RunVerdict;

pub struct Header { pub plan: Digest, pub round: u64, pub declared: Digest, pub rules: Digest,
                    pub image: Vec<(Vec<u8>, Vec<u8>)>, pub limit: u64, pub locate: u64,
                    pub plan_observers: u64, pub run: Vec<u8> }
pub struct Report { /* … */ }
impl Report {
    pub fn new(header: Header, declared: &Declared) -> Report;
    /// `add_at` at the next position (the number of calls so far): for a caller
    /// that adds every selected observer in plan order (M1's `slip verify`).
    pub fn add(&mut self, observer: &Digest, main: &[u8], verify: Option<&Shard>, map: Option<Status>);
    /// One call per selected observer, `position` its index in the plan, in any
    /// order: nothing the report holds depends on the order of the calls. Panics if
    /// `position` is not below `plan_observers`, or it or the observer was added
    /// before: a defect of the caller, which would let one observer stand for another.
    pub fn add_at(&mut self, position: u64, observer: &Digest, main: &[u8],
                  verify: Option<&Shard>, map: Option<Status>);
    pub fn verdict(&self) -> RunVerdict;
    pub fn states(&self) -> [u64; 7];
    pub fn to_json(&self) -> slip_model::json::Value;
    /// §7.1 from `plan` to `exemption`: the caller adds `wall`, `peak_rss`,
    /// `run`, `report` and prints `verdict_line()` last.
    pub fn to_text(&self) -> String;
    pub fn verdict_line(&self) -> &'static str;
    /// The lines for standard error (§7.1), each ending in a newline: one per
    /// entry of `not_verified`, at most 100, then `slip: and <n> more (see
    /// the report)` if there are more.
    pub fn error_lines(&self) -> String;
}
```

`add` refuses a shard that is not this observer's result in this run: a
shard whose `observer` is not `observer`, whose `round` is not the
header's, or whose verdict's `rules` is not the header's `rules` makes the
observer `failed`, with an empty message.

`add` keeps the findings of a shard and drops the shard; a report over
4000 observers of 64 findings each holds a quarter of a million findings.

## 8. What this does not show

- **Only analysed observers.** A configuration that is not in a database,
  a branch of `#if` no observer takes, and a template nothing instantiates
  are not verified. The coverage report says which
  ([03](../docs/design/03-equivalence.md) §9). A run over a selection
  (`--only`, `--limit`) says `verified` of the selection; its report gives
  the two numbers.
- **One front end.** Both images are resolved by Kiln. An error Kiln makes
  the same way twice is invisible here; tiers T2 to T4 of
  [03](../docs/design/03-equivalence.md) §8 exist for that.
- **Agreement between observers** is not shown by any one observer's
  verdict. §9 adds the check that shows it, for the observers of the run.
  What remains outside is the first point above: a declaration in a file
  that no observer of the run reads.
- **What a dependency declares where no observer looks.** Ownership is
  read from the declarations an observer has (§2.1). An owned
  declaration of an entity that the dependency also declares, in a
  header that no observer includes together with the owned one, is
  moved as owned, and the dependency's binary keeps the old name. Two
  more forms of the same thing (§2.1, the three holes): a name a
  dependency macro makes by `##` (`#define DECL_INIT() void
  legacy_##init();`, invoked in owned code), which is owned; and an
  owned header that a read-only file includes, where every observer
  enters it directly first and the read-only file's `#include` is
  skipped by its guard, which is owned there although the dependency was
  compiled against it (a moved exception type is no longer caught, a
  function taking a moved type no longer links). WP-116's run-level
  check is to close all three.
- **What a generated includer hides** (the safe direction). A read-only
  file the build generates that includes owned code (a precompiled
  header, a unity-build main file) makes that code read-only for the
  observer (§2.1): a correct migration is reported as a difference there,
  not shown correct. The third class of file of "Open questions"
  (WP-118) is to answer it.
- **Where the map parse looked, as far as Kiln's log shows it.** Guard
  N1 (§5.3) knows the places the map parse looked for a file and found
  none only from Kiln's lookup log and the command's own search
  directories: an absolute name, a toolchain directory Kiln's driver
  dropped and a module probe are not checked; nor is a path that
  resolves elsewhere while every file the map parse read is still there
  unchanged (a link outside the roots through which a file was found,
  retargeted since: §5.1, "What remains outside").
- **Sums are believed.** A group's `count`, `indeterminate` and `digest`
  are checked against its unit list only where there is one. A shard
  whose sums were not computed from its units (a defect of the walk, or a
  shard written by something else) can verify wrongly at §4.1 step 1.
  The walk that writes sums is as much trusted base as the comparison.
- **Text.** Strings, comments and anything else that spells a name without
  referring to it ([03](../docs/design/03-equivalence.md) §7 rows 14, 15).
- **Names in strings the toolchain resolves.** A name or a mangled name
  that the program spells as a string for the compiler's back end or the
  linker: an `asm` label (`int f() __asm__("_ZN6legacy1gEv");`), the
  target of an `alias`, `ifunc` or `weakref` attribute, a `symver`
  attribute, the text of a file-scope `asm` (`.symver`, `.globl`, `.set`
  directives). The string is the same in both images, and
  nothing in the translation unit looks it up (clang resolves an `alias`
  target only when it emits code: `-fsyntax-only` accepts one that names
  nothing), so the unit that holds it is the same too. A rename or a
  relocation of the entity the string names verifies, and the program
  then fails to link or binds another symbol. No row pins this: a `good`
  tree for such an edit would state that a wrong edit is correct. A name
  the front end does look up is not text and has its row:
  `__attribute__((cleanup(fn)))` (R14); `#pragma weak` and `#pragma
  redefine_extname`, whose identifier the front end resolves to the
  declaration it then gives the attribute (R23); and the values of
  `__func__` and `__PRETTY_FUNCTION__` (R12, N18, R21).
- **Other languages and build files.** A name held in a file that is not
  a C++ translation unit of the run: a renamed `extern "C"` function
  called from `.c` files the run does not parse, a linker or version
  script, a `.def` export list, a symbol list in a build rule,
  `dlsym("f")` or `GetProcAddress` with the name as a string. These are
  outside both images. The rename verifies, and the binary no longer
  links, exports the name or finds it at run time. A transform that
  renames an entity with C language linkage, or one a binary exports,
  owes its user the list of such files to edit; Slip reads none of them.
- **Derived inputs.** A file the build generated from the subject (a
  table, a configuration header, a module file) lies in the build tree,
  outside every subject root, and is the same file in both images: the
  edit's effect on it is not seen until it is generated again from the
  post-image, which Slip does not do. A generated file that includes a
  subject file by an absolute path trips G1 (§5.3) for every observer
  that reads it.
- **The command.** Both images are parsed with one command, up to the
  roots (§5.1). An edit to the build (a flag, a definition, an include
  path) is not an edit to the subject and is not verified. Nor is an edit
  to a response file: the plan expanded it when it was made
  (`spec/cdb.md` §4), so the observer's command holds the words it had
  then, in both images.
- **The environment and the clock.** `Command.env` is not applied
  (`spec/model.md` §8.4), and `__DATE__`, `__TIME__` and `__TIMESTAMP__`
  are those of one fixed instant in every parse. A program whose meaning
  depends on either is verified for that environment and that instant.
- **How a file is reached.** Under a tree image the path of a file is
  mapped back to the subject's wherever the front end spells it (§5.1),
  for the spelling the include directories give. A program that computes
  with a path spelled another way (through a symbolic link, or relative
  to a directory outside the roots that the two images reach
  differently) can show a `changed` that the edit did not cause: a false
  finding, never a false `verified`. The overlay image of M2 moves no
  file.

## 9. Agreement between observers

M2. Each observer is verified against its own pre-image. If one
declaration of a renamed entity is edited and another, in a file other
observers use, is not, each observer can verify (in one the entity has
its new name, in the other its old one and nothing to collide with), and
the program no longer links. The same happens if the edits never reach
an observer that declares the entity itself. Per-observer verdicts cannot
see it. The reducer could, but the reducer is not trusted. So the walk
reports, and the run checks, how each rule was used.

This section extends §7: §9.3 says what it adds to "this run's shard"
and to the run's verdict, §9.4 what it adds to the report. It changes no
state of §7 and no finding of §6; the order of findings by rank is as it
was.

### 9.1 Rule use and the binding

**Rule use.** A walk with a rule table that is not empty counts, for each
rule `r` (by its index in `Declared.rules`):

- `via(r)`: the entities whose own `(parent, kind, name, disc)`, parent
  and discriminator mapped, matched `r`, so that the rule gave them their
  key;
- `direct(r)`: the entities whose mapped key is the key `r` produces,
  `(r.to_parent, r.kind, r.to_name, r.disc)`, and whose own key no rule
  was applied to.

Entities are counted once each, declared or derived, whatever number of
declarations they have. An entity counted in `direct(r)` is one that
still is what `r` says was renamed. An entity whose name follows another
entity's mapped name (a specialisation, a member of an instantiation, a
decomposition) is counted by its own key only. A member of an
instantiation whose own key a rule `r` matches and maps to exactly the
key following its pattern gives (`digest.md` §3.4, WP-119) is in
`via(r)`, as the same entity is in a unit that does not instantiate its
class, and in no rule's `direct` (a rule was applied to its key); it is
not counted for the pattern member's rule. An entity with no key is
counted for no rule; an entity whose key is formed but carries a cause is
counted. At most one rule produces a key (§2.1, "One way to each key"),
so an entity no rule matched is in the `direct` of one rule at most. For
a rule of kind `namespace`
`direct` is §9.2's, and a third count, `kept_via`, is added there.

```
RuleUse := rules: digest            rules_id
           uses:  list<Use>         sorted by `rule`; only rules with a count not 0
           kept:  list<uv>          WP-111: indices into Declared.kept of the keys some entity has, ascending
Use     := rule: uv, via: uv, direct: uv, kept_via: uv

Binding := overlay:  opt<digest>    content id of the OVLY container, for an image of the overlay form; absent for a tree
           declared: digest         content id of the DECL container the job was given
           pre:      uv             the round of the pre shard (§5.2 step 0)
```

Sections of a shard of phase `verify` (`spec/model.md` §8): **24,
`binding`** and **29, `rule_use`**. (Ids 22 and 23 are not assigned; 26
is `spellings`, §10.4; 25 is retired, below.)

- `binding` is present in every shard a verify job writes, whatever the
  parse's outcome, and is taken from `Job.post`. (Until the exception of
  §9.3 is removed, a job of a tree image writes it only when the rules
  are not empty: `spec/landed/verify/WP-121.md`.) A shard the coordinator
  wrote for a worker that produced none (status `failed`) has no
  `binding`.
- `rule_use` is present when the outcome is `ok` and the rule table is
  not empty, and absent otherwise. Invariants: `uses` strictly ascending
  by `rule`; no `Use` with its three counts 0; `kept` strictly ascending.
  (`kept` and `kept_via` are WP-111's, and with them the payload took a
  new id: id 25, the payload before them, is retired. No writer writes
  it, and a shard with a section 25 is `UnknownSection`, as one with a
  reserved id is. What the format checks of a shard's age is its
  version (`SHRD` 1) and its section ids, critical ones refused when
  unknown: a shard written before WP-111 is refused by its section 25,
  not read with a payload it does not have. `spec/landed/verify/
  WP-111.md` §5.)

With the binding a verify shard says which declared changes, which
overlay and which pre shards it was made with; M1's shards did not
("Open questions").

### 9.2 Stale observers and unused rules

**Stale.** An observer is **stale** for a rule `r` if its `direct(r)` or
its `kept_via(r)` is not 0, whatever the rule's kind (`kept_via` is only
ever counted for a namespace rule; one not 0 for another kind is stale
too: fail closed). For a namespace rule both counts are the ones below.
A stale observer is one finding of the run, `stale`, with the rule, the
observer and both counts.

**Stale for a namespace rule** (WP-111). The namespace
itself merges, but its members do not: if one observer sees an owned
entity in `acme` and another still sees it in `legacy` (a second
declaration in a header that was not edited), each verifies, since under
the rule both have the key `(legacy, …)`, and the program does not link.
For a rule `r` of kind `namespace` a walk counts instead:

- `direct(r)`: the owned entities, not namespaces, that are members of a
  namespace that no rule matched and whose own key is the key `r`
  produces, or of a namespace inside it, and whose key is not in
  `Declared.kept`. These still live where `r` says the owned members
  left. (With no two producers of a key and no identity rule, §2.1, a
  namespace whose key `r` produces and that no rule matched is the one
  namespace left with the old key: "its own, unmapped key is `T(r)`" and
  "a level no rule matched" are one condition.)
- `kept_via(r)`: the entities whose namespace chain was mapped by `r`
  and whose key is in `Declared.kept`. These moved, and were declared to
  stay.

An observer is stale for `r` if either count is not 0. `kept` lists the
owned entities that are members of the old namespace **on purpose**: one
that stays (an overload that argument-dependent lookup must find through
a dependency's type), and one the edit adds there (a compatibility
declaration, which is also `created`). Dependency entities are not
counted and are not listed: no rule reaches them (§2.1), but for those
of the next paragraph.

**A dependency entity by the per-inclusion rule alone** (WP-111 review
F2). An owned header (`own2.h`, under an owned root, not edited) that an
observer includes only through a read-only file (`dep.h`) makes its
entities dependency entities in that observer (§2.1, the include-chain
rule), and so no rule reaches them there; another observer that includes
it directly, or defines the entity in owned code, may move it. Counted
as a dependency entity it would count for nothing, and a run in which
one observer moved `legacy::f` and the dependency's binary still refers
to it would verify. So for a namespace rule's counts, an entity that is
a dependency entity **only by the per-inclusion rule**, every
declaration of it in a file the run's files say is owned (its expansion
location in a file, a name token spelled in a file in one too), none in
a file read-only by its own path and none of the kinds that are in no
file, is counted as an owned member is: `direct(r)` unless its key is in
`kept`. It cannot be `via` (its key is not mapped). Not kept, the
observer that left it is stale; kept, an observer that moved it is stale
by `kept_via`: both are right, since the dependency's binary holds the
name and it cannot move. An entity the dependency itself declares in
`legacy` (one declaration in a file read-only by its own path, or whose
name a read-only file's macro spells) stays uncounted
(`crates/slip_digest/tests/rules/stale/inclusion-chain-left`,
`-left-kept`, `-moved`, `-moved-kept`, `-readonly-declaration`,
`-readonly-expansion`, `-spelled-in-dependency`). Its
namespace is keyed in its own view, the dependency view, as its key is;
the chain of levels is climbed as for an owned member. *The cost*,
beside "The cost of the include-chain rule" (§2.1): an observer whose
owned code is behind a generated read-only includer (a precompiled
header, a unity-build main file) counts every owned member it reaches
through it that was left in the old namespace, so a correct migration
that leaves members in `legacy` on purpose lists them in `kept` for that
observer as for any other.

Why this is the condition: two observers disagree about an entity with
key `K` when one has it in the old namespace and the other reaches `K`
through `r`. If `K` is in `kept`, the second is counted in `kept_via`;
if it is not, the first is counted in `direct`. Either way one of them
is stale, and neither count needs anything from another observer. A
`kept` key that no observer has is reported like an exemption no
observer needed (§2.2).

`Use` gets the field `kept_via: uv` (0 for a rule that is not of kind
`namespace`); a `Use` is present when any of its three counts is not 0.

**What "inside" and "member" mean** (WP-111's decisions,
`spec/landed/verify/WP-111.md`):

- *Lexically, at every depth*: an entity is counted for each namespace of
  the chain of namespaces around it (its parent, keyed from its own view,
  §2.1 rule 2, and that namespace's parents), up to the global namespace.
  A level whose own key no rule matched and that `r` produces adds `r` to
  the entity's `direct` set; a level a rule `r` matched adds `r` to its
  `via` set. A kept entity counts `kept_via` for its `via` set; any other
  counts `direct` for its `direct` set. Each rule at most once per entity
  and count.
- *An inline namespace and the unnamed namespace* are namespaces of the
  chain like any other: a member of `legacy::v1` or of `legacy::(anonymous)`
  is inside `legacy`.
- *A nested namespace that a second rule maps elsewhere*: a member left in
  it is inside both: `direct` for the rule that produces the nested
  namespace's key and for the outer rule. Fail closed: the outer rule
  still counts it. The cost: an edit that leaves an owned entity in
  `legacy::detail` while `legacy` moves, which is not wrong if it is
  meant, must list it in `kept`.
- *A namespace reached through two rules*: one rule per level (a nested
  namespace mapped by its own rule inside one mapped by another); each
  rule gets the count, once. Two rules that produce one key are no table
  (§2.1).
- *Who is a member*: an entity keyed directly under a namespace, not a
  namespace, and owned (a dependency entity is keyed in the dependency
  view, and no rule reaches it), or a dependency entity by the
  per-inclusion rule alone (above). A class member follows its class; a
  specialisation and a member of one are keyed under their template, and
  a friend under its class, so neither is counted; a deduction guide is
  not counted **only when its template is a dependency entity** (pitfall
  N29 `good-5`: an owned guide beside a dependency's template, which
  [temp.deduct.guide] keeps in the template's scope, which the run cannot
  move); a guide of an owned template is counted like any owned member,
  left behind `direct` (`tests/rules/stale/deduction-guide-of-dependency`,
  `guide-of-owned-template-left`, `guide-of-owned-template-moved`). That
  a guide left in another scope than its template's does not parse is
  not relied on: Kiln accepts one at this pin, where clang 19 rejects it
  (`spec/landed/verify/WP-111.md`, For Kiln). An entity with C
  language linkage has the parent `c_linkage` and is not counted. A
  namespace's own key is not counted in `direct` (the merge of §2.1);
  a namespace a rule matched counts in `via`, as before.
- *`Declared.kept`* is compared with the entity's mapped key, the key it
  has under the rules, as the walk gives it. The walk records which
  indices of `kept` some entity had (`RuleUse.kept`); a kept key that no
  read shard of the run has is listed in the report as an exemption of
  list `kept` with 0 observers (§7.1, after `removed`), and changes no
  verdict.

**Why this is the condition.** The claim of §1, for a rule, is that the
entity it names has the new name. Two observers disagree about an entity
when one reaches its key through the rule and the other has it without.
`direct(r) = 0` in every observer says no observer has it without: every
observer that has the entity reached it through the rule, or no observer
has it at all. In an observer that has the entity both ways the two
collide and both are indeterminate (§2.1), so that case was already a
finding; the count catches the observer that has only the old one.

**A stated limit: TU-local entities of one name** (WP-111 review F3).
Two translation units may each have their own entity of one name and
key in `legacy` with internal linkage (`static int helper()`, or one in
an unnamed namespace inside `legacy`). If the edit moves one and leaves
the other, the program is correct (no symbol is shared), yet one observer
is stale whether or not the key is kept: not kept, the one that left it
counts `direct`; kept, the one that moved it counts `kept_via`. The run
fails closed: such an edit cannot be declared at this version (a rule
for the moved one alone, of any kind, produces the key the other still
has, which counts in that rule's `direct`; with no rule the moved one is
a difference). Pinned by `tests/rules/stale/tu-local-left`,
`tu-local-moved-kept` and `unnamed-member-left`; "Open questions".

**Unused rules.** A rule with every count 0 in every observer names
an entity no observer has. It is reported with the run, like an exemption
no observer needed (§2.2), and changes no verdict: a rule that matches
nothing changes no key (`digest.md` §3.4, "A rule that matches nothing
changes nothing"), so it cannot make a comparison equal; what it can do
is hide a typo, which the report's line shows (WP-140 item 3).

**A namespace used through its owned view** (WP-140). A dependency
namespace that owned code opens, or that has an owned member, has a unit
in the owned view (`digest.md` §3.4); when that unit is written (its key
differs from the namespace's own), the rule that matched its key counts
`via` once for the namespace, however many owned openings it has
(`tests/rules/views/two-owned-openings`). Before WP-140 that key was only
a level of its members' chains and the rule had `via 0`, which read as
unused in an observer whose every owned member of the namespace moved
(`tests/rules/stale/dependency-opens-target-moved`: `via 1` now). Its
members' `direct` and `kept_via` are as above: the owned view's key is
the level their chain climbs.

### 9.3 What is added to the run (§7)

**This run's shard.** To the condition of §7 (its content id is the
`shard` of the observer's last `Job` entry in the run's record) M2 adds:
if a worker wrote it (its status is not `failed`), its `binding` is the
run's (the same `declared`, the same `overlay` or the same absence of
one, the same `pre`); and if it is `ok` and the run's rules are not
empty, it has a `rule_use` whose `rules` is the run's `rules_id`. A shard
that fails either is not this run's: the observer is `failed` (§7), as
for any shard another run left. So is an `ok` shard with a `rule_use`
under no rule or naming a rule past the table or a `kept` index past
`Declared.kept`, and one whose `spellings`
is absent although the overlay has obligations, present although it has
none, or refused by the fold (§10.4). A run with a tree image and no
rule is M1's: its shards need no `binding`, and one that has a binding
must have the run's (`spec/landed/verify/WP-070.md` §2.1). The verify job keeps to
it: with a tree image it writes `binding` and `rule_use` when the rules
are not empty and neither when they are (`spec/landed/verify/WP-121.md`).
This exception is temporary: a verify job is to write `binding` in every
shard (§9.1), M1's `slip verify` included, after which a package of the
coordinator's removes it and a shard without a binding is never this
run's.

**Which observers.** The check is over every selected observer. With
rules, no selected observer whose map shard is `ok` is `unchanged`
(§5.5): each has a job, so each has a `rule_use` or is in a state that
already keeps the run from being verified.

**The run's verdict**, in full, with the additions in italics:

- **verified**: at least one observer was selected; every selected
  observer is `verified` or `unchanged`; *no selected observer is stale;
  and every obligation passed (§10.3)*;
- **not verified**: some observer is `mismatch` or `not_parsed`; *or
  some observer is stale; or some obligation failed or was seen by no
  observer (§10.3)*;
- **incomplete**: neither of those.

A stale observer and a failed obligation are facts about the post-image,
like a `mismatch`: they make the run not verified also when other
observers are `unverifiable` or `failed`. Staleness is read from the
`rule_use` of this run's `ok` shards only.

### 9.4 What is added to the report (§7.1)

The report keeps its name, `slip.verify.v1`: every member and line of
§7.1 is unchanged, and a run with a tree image and no rule writes exactly
the M1 document. Members added, each present only when it applies:

```
"overlay": <id of the OVLY container>                      for an image of the overlay form; "image" is then []
"agreement": { "rules": <n>,                               when Declared.rules is not empty
               "stale": [ { "rule": <index>, "observer": <id>, "main": <path>, "direct": <n>, "kept_via": <n> }, … ],
               "unused": [ <index>, … ] }
"spellings": …                                             §10.5
```

`stale` is sorted by `(rule, the observer's position in the plan)` and
holds every stale pair; `unused` is ascending. `"overlay"` follows
`"image"`; `"agreement"` and then `"spellings"` follow `"exemptions"`
and precede `"run"`. An `unseen` entry of `spellings` has `observers`
0, `first` and `found` null and an empty `name`
(`spec/landed/verify/WP-070.md` §2.6).

Lines added to the text, after the `exemption` lines and before `wall`:

```
image overlay <overlay id>                       in place of the `image` lines of §7.1, for the overlay form
agreement rules <n> stale <n> unused <n>         when there are rules; `stale <n>` counts pairs
stale rule <index> direct <n> kept_via <n> <main file>    at most 100, then `stale: <n> more (see the report)`
unused rule <index>
```

Nothing is added to the lines on standard error: those list observers by
state, and a stale observer's state is whatever its verdict made it.

### 9.5 Interface

The walk's half, in `slip_digest` (trusted base):

```rust
// Options gains `rule_use: bool`; Output gains `rule_use: Vec<slip_model::Use>`  (empty when off or when there is no rule)
// WP-111: Output gains `kept: Vec<u64>` (RuleUse.kept); `Declared.kept` is given by
pub fn digest_kept(input: &Input<'_>, options: &Options<'_>, kept: &[Digest]) -> Output;   // `digest` is `digest_kept` with none
```

`RuleTable::map` must then say whether a rule was applied; how
(`map_traced`, a second return value) is the implementation's.

The transforms that must name entities by key (`spec/edits.md` §4.1,
§9; `spec/api.md` §7.5) and the check of §10 get keys from
`slip_digest::query::Keys` (`docs/wp/WP-087.md`): one interface, over the
functions the walk uses, so a key it gives is the key a unit of the map
shard has. `Keys::with_rules` gives the keys of a post-image in the
pre-image's key space. A transform that gets a key wrong produces a rule
that matches nothing: the rule is unused and the entity's unit is
`removed` and `added`. It cannot produce a wrong `verified`.

The run's half, in `slip_verify::run` (trusted base), added to §7.3:

```rust
pub struct Stale { pub rule: u64, pub observer: Digest, pub direct: u64, pub kept_via: u64 }

/// §9.3: the verdict of the states, with the two conditions over all observers.
pub fn combine(states: RunVerdict, stale: usize, obligations_not_passed: usize) -> RunVerdict;

impl Report {
    /// Before the first `add`, for a run whose image is an overlay: `add` then expects
    /// that overlay in each binding, and folds the overlay's obligations (§10.3).
    pub fn overlay(&mut self, id: Digest, overlay: &slip_model::Overlay);
    /// Before the first `add`: the round of the pre shards (default 0).
    pub fn pre_round(&mut self, pre: u64);
    pub fn stale(&self) -> &[Stale];                       // sorted by (rule, plan position)
    pub fn unused(&self) -> Vec<u64>;
    /// §9.2: the keys of `Declared.kept` no read shard of the run has, in its order.
    pub fn unseen_kept(&self) -> Vec<Digest>;
    pub fn spellings(&self) -> &[spelling::Outcome];       // §10.6; one per obligation of the overlay
}
```

`Report::add` reads `binding`, `rule_use` and `spellings` from the shard
it is given and applies "this run's shard" as far as the shard itself
decides it (the caller still decides it from the run record, §7);
`Report::verdict` is `combine` of `run_verdict(states)`. `state` and
`run_verdict` are as in §7.3.

## 10. Spelled entities

M2 and M3. An edit's replacement can hold a **spelled-entity piece**: the
transform said which entity a name is to denote and Slip, or the
transform, chose the text (`spec/edits.md` §4.2; `spec/api.md` §7.1,
§7.2). The native rename writes one at every resolved reference it
renames (`spec/edits.md` §9.3). A digest cannot check that choice where
the unit that holds the name is declared `changed` (§2.2), and that is
where a respelled callee sits. So the choice is checked directly, by the
trusted base, in the post-image: the name where the piece landed must
denote the entity the piece records.

The check takes nothing from a transform, the reducer or the applier but
the obligations, and an obligation can only add a failure: no obligation
makes anything pass that §4 would not.

### 10.1 Obligations

```
Obligation := file: uv        index into the overlay's `files`
              lo:   uv
              hi:   uv        lo < hi <= the file's post size
              key:  KeyId     the entity meant, in the key space of the pre-image (§2)
```

The bytes `[lo, hi)` of the post-image file are the text of one spelled
piece, or of one identifier piece with a key (`api.md` §7.2 rule 6;
WP-132), of one accepted edit, and `key` is the `KeyId` the piece
carries. The two are one obligation: nothing below tells them apart.
Apply derives the range when it composes the post-image
(`spec/edits.md` §7.1). The obligations of a run are the `spellings`
list of its overlay, sorted by `(file, lo, hi, key)` with no two equal;
an obligation is named by its index in that list. An image of the tree
form has no obligation.

### 10.2 In one observer

In the parse of §5.2 step 1, after the walk, for every obligation whose
file is in the parse's `inputs`:

1. The **written references** of the unit are the entries of Kiln's
   written-reference table that have a referent: the names that were
   resolved while the unit was parsed, **in every role Kiln records**
   (at this pin expression name, member name, qualifier component, type
   name, template name, base specifier, member initialiser and concept
   name; a role Kiln adds takes part the day it records it). The check
   asks what a name denotes, and a name denotes something whatever the
   syntax around it: a concept name in one use of a macro argument
   vetoes the argument as surely as an expression name in another.
   (`spec/api.md` §6.5 rule 3 lists the roles a transform is given as
   references, which is another question.) Entries equal in role, name
   range and referent are one reference. The table's entries **without a
   referent** (a dependent name, a name whose resolution failed) are
   **unresolved names**; they have name ranges as references do.
2. A reference's (or unresolved name's) **name range** is the file range
   of its name: the bytes
   of one file that translation turned into exactly the name's tokens,
   at tier 0 or 1 (`spec/api.md` §5.4; `slip_kiln::ranges`, which is
   Kiln's `editable_file_range`, `spec/edits.md` §2.2). A reference
   whose name has no such range takes no part. Neither does one whose
   range lies in a file Kiln re-encoded when it loaded it (invalid
   UTF-8, `spec/edits.md` §2.3 `file_recoded`: its offsets are not the
   post-image's bytes), or in a buffer with no subject path (no file):
   an obligation there is answered by nothing, so it is not seen, never
   held.
3. A reference or an unresolved name **answers** the obligation if its
   name range is in the obligation's file, ends at `hi` and starts at or
   after `lo`. (A
   spelling `::a::b::f` is answered by the reference `f`; `a` and `b`
   end before `hi`.)
4. For each answering reference take the **origin** of its referent: the
   referent itself, or, for a specialisation or a partial
   specialisation of a template or a member of an instantiated class,
   the template or the member of the template it was made from (a member
   of a partial specialisation keeps its own origin: it is not a member
   of the template; and a friend declaration that does not define the
   function is not followed to the pattern's, `digest.md` §3.4;
   `slip_kiln::written::origin`). Take the origin's `KeyId` under the run's rules
   (`Declared.rules`; mapped keys, `digest.md` §3.4) **and the run's
   ownership**: the read-only files the verify job decides from the
   declared container's `files` (§2, §2.1 "Rules rename owned entities
   only"), the same `ReadOnly` the walk of the same parse is given. That
   is its key in the pre-image's key space. A name that denotes a
   dependency entity is compared under the dependency's own key, never a
   mapped one; whether a key collides (rule 5) is marked under the same
   ownership.
5. The obligation, in this observer:
   - **holds** if at least one reference answers, no unresolved name
     answers, and every answering reference's origin has the
     obligation's `key`;
   - **fails** if some answering reference's origin has another key, or
     no key (it is not a unit, is keyed by content, or its key is
     indeterminate or collides), or an unresolved name answers (`found`
     is absent; what a dependent name denotes is decided per
     instantiation, and the table does not say per instantiation);
   - is **not seen** if nothing answers: the bytes are in a conditional
     group the observer skips, in a macro argument the macro does not
     use, or in a name the table does not record.

   An answer covers the names the table records at the range, and no
   other: a use of the same bytes that Kiln does not record, or records
   with no name range, cannot make the obligation fail (§10.7, the named
   hole).

A name in a macro argument that is substituted twice gives two
references with one name range; each must have the key.

A failed obligation does not change the observer's `Verdict` (§6): the
comparison is of digests and stays what it is. It is recorded beside the
verdict (§10.4) and judged over the run.

### 10.3 Over the run

For each obligation of the overlay, over this run's `ok` shards whose
state is `verified` or `mismatch` (§7, §9.3):

| The obligation | Is |
|---|---|
| fails in some observer | **failed**, `denotes_other`: with the number of such observers, the first in plan order, and what that observer found |
| holds in at least one observer and fails in none | **passed**; the observers where it is not seen are counted |
| holds in no observer and fails in none | **failed**, `unseen`: no observer could check it |

A run with a failed obligation is not verified (§9.3). An obligation is
`unseen` also when the observers that would have seen it did not parse
or did not finish; the run is then not verified on that account as well
as theirs.

`unseen` is the conservative reading, and it is the rule
(`spec/api.md` §7.2 rule 5): text that no observer resolves was written
on a transform's word alone.

### 10.4 The `spellings` section

Section **26** of a shard of phase `verify` (`spec/model.md` §8); present
when the outcome is `ok` and the job's image is an overlay with at least
one obligation, and absent otherwise.

```
Spellings := seen:   uv              obligations whose file is in this shard's `inputs`
             held:   list<Span>      the obligations that hold, as runs of indices; ascending, no two touching
             failed: list<Failed>    strictly ascending by `index`
Span   := first: uv, count: uv       count at least 1
Failed := index: uv
          found: opt<KeyId>          the key, under the rules, of the origin of the first answering reference
                                     in unit order that does not have the obligation's key; absent if it has none,
                                     or if the first that fails is an unresolved name (§10.2 rule 1)
          name:  str                 that entity as a person would write it (§6.1), or, for an unresolved name,
                                     `<unresolved name: dependent>` or `<unresolved name: resolution failed>`;
                                     for the report only
```

Invariants: no index is in `held` twice or in both lists; the number of
indices in the two lists is at most `seen`. The obligations not seen are
the `seen` minus those. The decoder cannot check an index against the
overlay; the fold (§10.6) refuses an index that is not one of the
overlay's obligations or whose file the shard's `inputs` do not list, and
a `seen` that is not the number of obligations whose file the `inputs`
list, as a shard that is not this run's.

Counters of the shard, beside those of §7.2: `verify.spelling.obligations`
(`seen`), `verify.spelling.holds`, `verify.spelling.fails`,
`verify.spelling.not_seen`; all four present when the section is.

### 10.5 What is added to the report (§7.1)

```
"spellings": { "obligations": <n>, "passed": <n>,          when the overlay has obligations
               "failed": [ { "file": <path>, "lo": <n>, "hi": <n>, "key": <id>,
                             "why": "denotes_other" | "unseen",
                             "observers": <n>, "first": <main file, or null for unseen>,
                             "found": <id or null>, "name": <str> }, … ] }
```

`failed` is in the obligations' order. `lo` and `hi` are offsets in the
post-image file. In the text, after the lines of §9.4:

```
spelling obligations <n> passed <n> failed <n>
spelling <why> <file>:<lo>-<hi> <key, first 16 digits> observers <n> <name>     at most 100, then `spelling: <n> more (see the report)`
```

### 10.6 Interface

In one observer, in `slip_digest` (trusted base; `docs/wp/WP-096.md`):

```rust
// crates/slip_digest/src/spelling.rs
pub struct Obligation { pub file: Vec<u8> /* subject path */, pub lo: u32, pub hi: u32, pub key: Digest }
pub enum Spelling { Holds, NotSeen, Fails { found: Option<Digest>, found_name: String } }

/// One observer of the post-image. `obligations` are those whose file the unit read;
/// the result is parallel to it. `input` is the digest's view of `unit`.
pub fn check_spellings(unit: &slip_kiln::Unit<'_>, input: &Input<'_>,
                       rules: &slip_model::keys::RuleTable,
                       read_only: ReadOnly<'_>,           // the walk's `Options::read_only`
                       obligations: &[Obligation]) -> Vec<Spelling>;
```

It gets keys from `Keys::with_rules` (§9.5) under the same rules and
read-only files as the walk of the same parse, so the key it compares is
the key that walk gave the entity. It takes the unit
because the written references and the origin are
`slip_kiln::written`'s, which read a `Unit`
(`spec/landed/verify/WP-096.md`). The verify job
(`spec/exec.md` §8.6) calls it and writes the section.

Over the run, in `slip_verify` (trusted base), naming no Kiln type:

```rust
// crates/slip_verify/src/spelling.rs
pub enum Outcome { Passed { not_seen: u64 },
                   DenotesOther { observers: u64, first: usize /* plan position */, found: Option<Digest>, name: String },
                   Unseen }
pub struct Fold { /* one slot per obligation */ }
impl Fold {
    pub fn new(overlay: &slip_model::Overlay) -> Fold;
    /// One `ok` shard of this run; `Err` if the section names an obligation the overlay
    /// does not have or a file the shard did not read, breaks an invariant of §10.4, or
    /// has a `seen` its `inputs` do not give (§10.4).
    pub fn add(&mut self, position: usize, inputs: &[FileRef], section: &slip_model::Spellings) -> Result<(), String>;
    pub fn finish(self) -> Vec<Outcome>;
}
```

`position` is the observer's position in the plan (the `position` of
`Report::add_at`, §9.5), never the order of the calls, so the outcome
does not depend on it. `Report::add_at` folds the section of a
`verified` or `mismatch` shard only. That of an `unverifiable` one is
checked as `add` checks it, and a section it refuses makes the observer
`failed` as for any other, but it is not folded: its observer's
post-image is not the run's (a guard of §5.3 failed, or the parse found
something it could not compare), so neither what it held nor what it
failed is of the image the obligations are about. The observer already
keeps the run from being verified; folding its `held` could only let an
obligation pass on its word.

### 10.7 What it shows

For a name written as a spelled piece: in every observer that resolves
the name where the piece landed, it denotes the entity whose key the
piece carries, and at least one observer resolves it. It does not show
that the edit was right to name that entity, and it shows nothing about
a literal piece, a copy, or a name the front end does not record a
reference for (a declaration's own name). Those are judged by the
comparison of digests alone.

**The named hole.** `Holds` covers the uses of the piece's bytes that the
written-reference table records with a name range (§10.2 rule 5). A use
of the same bytes elsewhere — the same macro argument substituted twice,
or the same header entered twice — that the table does not record, or
records without a range, cannot veto. At this pin those are: a
designator (`.x = 1`); a using-declaration's name (`using a::x;`); a
type-constraint's concept (`template <a::C T>`); a friend's elaborated
name (`friend struct a::x;`); an explicit instantiation's or
specialisation's target (`template int a::t<int>(int);`); and a use made
by token pasting (`x##_`), which has no name range. (A plain elaborated
name, `struct x s;`, is recorded as a type name and takes part.) Each
shape has a pending test in `crates/slip_digest/tests/spelling.rs`
(`s10_7_hole_*`); the hole closes as Kiln records them
(`spec/landed/verify/WP-096.md`, "For Kiln").

## Tests

- `crates/slip_verify/tests/compare.rs` — §3, §4, §4.1, §4.2 and §6, over
  unit lists written by hand: every rule of every step, each exemption
  preventing exactly its finding and no other, multiplicities, two units
  of one key, derived keys on one side, a key declared in one image and
  derived in the other, `file`, `pre` and `post` of each kind, the order
  of findings by rank (a `derived_changed` finding is within the first
  `limit` when ten thousand indeterminate findings exist, and alone
  with `limit` 1), the three ways of making a verdict, malformed groups,
  `comparable`, and `at_sums` over unsorted and duplicate groups.
- `crates/slip_verify/tests/groups.rs` — §4.1: for generated pairs of
  images (random units, groups, moves between groups, duplicates), the
  outcome through groups equals the outcome over all units; 12000 pairs
  from the fixed seed `0x5eed0016c0def00d` (`SLIP_VERIFY_SEED` overrides),
  the property asserted on every pair.
- `crates/slip_verify/tests/run.rs` — §7, §7.1, §7.3 over shards written
  by hand: every row of the state table, each of them against each map
  status; the run's verdict for every combination of two states and for
  no observer; `indeterminate_only` from the counters, including a
  verdict that lists fewer findings than it has; groups across observers
  and their order; `not_verified`; exemptions used by some,
  by none, and one key in two lists; golden JSON and text in
  `crates/slip_verify/tests/golden/run/`.
- `crates/slip_model/tests/declared.rs` — §2: the `DECL` container round
  trip; rejected forms (unsorted, duplicate, two rules with one match).
- `crates/slip_digest/tests/rules.rs` — §2.1: mapped keys under rules, one
  directory per case: a relocated namespace, a merged namespace, a renamed
  member, a swap, a collision, a rule that captures a pre-existing entity.
- `crates/slip_digest/tests/records.rs` — §2.3 "Records": the records of
  snippets, a fallback key, two entities of one name, and the id of every
  record's `Key` being its `KeyId`.
- `crates/slip_cli/tests/declare.rs` — §2.3 over records written by hand
  and over snippets: every line form, every error with its line number,
  the rule of a relocation into the global namespace, into an existing
  namespace and into a namespace another line relocates, `=`, and each
  rule checked by digesting the post-image with it and comparing.
- `crates/slip_digest/tests/trace.rs` — §6.1: tracing changes no digest
  (every pair test's sources, with and without) and every unit's second
  writing hashes to its digest; marks ascending and inside the stream;
  the site of a changed callee, a changed literal, a changed type of a
  declaration, a changed default argument; `refs` are the digest fields
  that are keys of units.
- `crates/slip_cli/tests/image.rs` — §5.1: refused images; the post
  command for roots at the start, in the middle and at the end of a word,
  twice in one word, as a prefix of a longer name (`/s` in `/sx`: not
  rewritten), `argv[0]` and `env` kept, `directory`, the added
  `-fmacro-prefix-map=` words and `__FILE__` under them in a parse;
  subject paths; the identity.
- `crates/slip_cli/tests/verify_job.rs` — §5.2–§5.4, the task run in
  process on trees the test writes: verified at the sums with one parse
  (`verify.reparsed` 0); a changed callee (mismatch, two parses or
  three); a post-image that does not parse; every row of the table of
  §5.3, each with its reason, G1 by an absolute `-I` into the subject and
  by a symbolic link, G2 by editing a header between the map phase and
  the job; the post shard without unit lists; the counters; `limit` and
  `locate`; a `CompareError` as a failed job; the five reproducers of the
  WP-052 review (a failed probe and a shadowing header through a relative
  `-I` from outside the roots, a response file, another letter case and a
  symbolic link to the subject, drift outside the roots), each against the
  truth (the subject with the edit applied, mapped); L1 by a failed probe,
  a successful lookup and a lookup from outside both roots; the rules
  applied to the post-image and not to the pre-image.
- `crates/slip_kiln/tests/lookups.rs` — what Kiln's lookup log holds, each
  form through a real parse: `#include` both forms, `#include_next`,
  `__has_include`, `__has_include_next`, `#embed`, `__has_embed`,
  `-include`; `-imacros` not accepted; an absolute name not recorded; a
  lookup after five thousand; a lookup from another directory; the
  toolchain's directories in the list.
- `crates/slip_cli/tests/locate.rs` — §6.1 through the task: names from
  each image, the site of each planted difference at its line, the
  `what` of a reference and of an indeterminate unit, G3.
- `crates/slip_cli/tests/verify.rs` — §5, §7 and `spec/exec.md` §8.7–§8.9
  through the executable: the identity (verified, one parse per
  observer), an edit that changes a callee (mismatch, localised), a
  relocation with its declared file (verified), an edit that breaks the
  parse, a pre-image that did not parse (`never_parsed`), no map run, an
  empty selection, a stale shard of another run, the report and each exit
  status.
- `spec/pitfalls.md` and its tests — each catalogued hazard as a correct
  edit that verifies under its rules and a wrong edit that does not.
- `crates/slip_digest/tests/rule_use.rs` — §9: `via` and `direct` for a
  renamed function, an overload set with one rule per overload, a member
  of a specialisation, a declaration left under its old name beside the
  renamed one (a collision and a `direct`), one left alone in the unit
  (`direct` only), a merged namespace (not stale); the digests of every
  pair test are the same with `rule_use` on and off.
- `crates/slip_digest/tests/api_keys.rs` — §9.5: `Keys`
  (`docs/wp/WP-087.md` lists the cases).
- `crates/slip_verify/tests/agreement.rs` — §9.3, §9.4 over shards
  written by hand: stale and unused rules; a shard without a binding,
  bound to another overlay, to another `Declared`, to another `pre`; an
  `ok` shard without `rule_use` and with another `rules_id` (each
  `failed`); `combine` for every pair of a states' verdict and the two
  counts; the added members and lines against goldens, and an M1 run's
  report unchanged byte for byte.
- `crates/slip_digest/tests/spelling.rs` — §10.2, on post-image snippets
  with marked obligations: holds (a bare name behind a written
  qualifier, a fully qualified name, a function template whose referent
  is a specialisation, one overload among several); fails (another
  overload, a nearer declaration of the same name, a variable, an origin
  without a key, a name in a unit declared `changed`); under a rename
  rule (the post-image's new name has the pre-image's key); not seen (a
  skipped group, an unused macro argument); twice substituted with one
  use that fails.
- `crates/slip_verify/tests/spelling.rs` — §10.3, §10.4: every row of
  the table; an index the overlay does not have and a file the shard did
  not read; the invariants of the section.
- `crates/slip_cli/tests/verify_overlay.rs` — §5.1 "The overlay form"
  through the task: the post command and subject paths; O1 by a loading
  loop that a test hook stops before its end, so that an overlaid file
  is read from disk; O2 by a subject file changed after the map phase;
  an edit that adds an `#include` of another overlaid file (two parses,
  one shard); a missing blob and a blob that does not hash to its name
  (each a failed job);
  the `binding`, `rule_use` and `spellings` sections; an unchanged
  observer with and without rules (§5.5).

## Open questions

- **Keeping a word that leaves the subject** (if a corpus needs it). G5
  refuses a word spelled through the subject whose path leaves it to a
  place under no root (§5.1). Keeping such a word byte for byte in the
  post command, with no pair, is sound only when every subject directory
  the word walks through exists, unchanged as a directory (not a link),
  at the same place in the post tree: the post parse then walks the post
  tree's copy and reaches the same place, and spells it as the pre parse
  did. Checking that condition, per word, against both trees is a design
  of its own; it is the way to lift the refusal.
- **The tree form as an overlay** (long term; not to be built now). When
  Kiln's overlay is in the pin (`docs/wp/WP-066.md`), an image of the tree
  form can be parsed as an overlay of the post tree's files onto the
  subject's paths, with the observer's command unchanged: every file is
  then reached and spelled by its subject path, as in the real
  post-image. That removes the rewrite of §5.1, the map pairs and guards
  G1, L1 and G5 together, leaving the overlay form's O1 and O2.

- **A third class of file: not edited, and rebuilt from source.**
  `spec/edits.md` §2.1 has owned and read-only files; a file the build
  writes and compiles with the rest (a precompiled header, a unity-build
  main file, a generator's `.inc` or `config.h`) is not edited by Slip,
  but no binary outside the run holds its names. Read-only today, it
  costs false differences (§2.1, "The cost of the include-chain rule").
  The class `rebuilt`, the user's statement about a path, is
  `docs/wp/WP-118.md`.

- **A literal operator as a rule's new name.** `relocate` moves a literal
  operator with `=` and cannot rename its suffix (`NotRenamable`: `<name>`
  is an identifier). A rename of `_km` to `_m` is a refactoring a tool
  makes; it needs a `<name>` form for `operator""_m` and nothing else of
  §2.1. Not urgent: such a rename is reported today, never silent
  (`spec/landed/pitfalls/WP-109.md`).

- **Name-derived values that changed** (`digest.md` §9). A relocation
  changes what `__func__`, `__PRETTY_FUNCTION__`,
  `std::source_location` and `typeid(T).name()` yield for the entities
  it renames, and the verdict does not count it. A program that
  observes such a string (a parsed log, a registry keyed by a type's
  name, a persisted identifier) should be told. Proposed: an
  informational section of the report listing each unit that holds such
  a builtin and whose name, or a name in whose scope it is, a rule
  changes; no verdict depends on it. The walk would count them per unit
  (a counter beside `rule_use`, §9.1). Open: whether a run may ask for
  them to be findings (`--strict-names`), and whether
  `typeid(T).name()`, whose value is the implementation's, is listed.
- **Agreement costs a job per observer** (§9, §5): with rules, no
  observer is unchanged. `spec/edits.md` ("Open questions") proposes a
  filter of key ids in the map shard so that an observer that cannot have
  a renamed entity is shown not to, without a parse.
- **Agreement for namespaces** (§9.2, "Stale for a namespace rule").
  A relocation that moves only part of a namespace's owned members under
  a namespace rule lists every member that stays in `kept`, which for a
  phased migration is most of the namespace. Whether a phase is better
  declared with one rule per moved entity, or `kept` given as a set of
  files, is to be decided with the namespace migration (M4).
- **TU-local entities of one name in two units** (§9.2, "A stated
  limit"). One left in the old namespace and one moved is correct, and
  one observer is stale either way. A `kept` key that holds for one
  translation unit only (a `kept` given per file, as above, or an entity
  of internal linkage told apart by its unit) would let it be declared
  with a namespace rule; until then it fails closed.
- **References to a namespace that two views key** (WP-140; §2.1,
  "A dependency that itself opens `acme`"; `digest.md` §3.4). A
  namespace with a unit in each view keeps **one** key wherever it is
  written as a reference: its own, which for a namespace a dependency
  opens is the dependency's, unmapped. Nothing in the post-image says
  which pre-image namespace an owned reference meant, and a reference to
  a dependency's namespace by the dependency's name is what rule 1
  already promises. So after relocating `legacy` into `acme`, an owned
  reference that named `legacy` and now names `acme` writes `acme`,
  the pre-image wrote `legacy`, and the holding unit differs. The move is
  correct and fails closed; the motivating migration meets this
  wherever owned code aliases, `using namespace`s, or qualifies a
  dependent name with the moved namespace. Where it is written (every
  path that reaches `write_ref` or `write_canonical_nns` with a
  namespace), with fixtures in `crates/slip_digest/tests/rules/views/`:

  - an alias's target (`namespace L = legacy;`): held by the alias, own
    key, one per unit; `changed L` (`alias-redirected`).
  - a using-directive's target: held by the directive, keyed by content
    (the target is the key), one per unit; `removed <using_directive
    legacy>`, `added <using_directive acme>` (`directive-redirected`).
  - a using-declaration's qualifier (`using legacy::g;`): held by the
    using-declaration, keyed by content, one per unit; removed and added
    (`using-declaration-redirected`).
  - a qualifier written in a body, of a name not resolved at the
    definition (a dependent call `legacy::h(t)` with more than one `h`,
    a dependent-scope reference, a member-overload qualifier): held by
    the function, own key, any number per unit; `changed f`
    (`qualifier-dependent-overloaded`). Resolved at the definition (one
    callee, `qualifier-dependent-redirected`; a non-dependent call,
    `qualifier-non-dependent-in-template`), the callee is written, not
    the qualifier, and it verifies; so does a qualifier whose next level
    is a type (`dependent-type-redirected`, `member-qualifier-spelled`).
  - a qualifier in a signature's profile (`decltype(legacy::h(t))`):
    held by the function, whose key the profile is part of, any number
    per key; removed and added (`profile-qualifier-overloaded`; with one
    `h`, `profile-qualifier-redirected` verifies).
  - a qualifier in a type (`typename legacy::X<T>::type`, a dependent
    template name): held by whatever unit writes the type, own key or
    content as that unit is; differs where the namespace level is
    written, as above.
  - a reflection (`^^legacy`): unsupported, indeterminate either way.
  - a spelling (`spelled-nns`, the own spelling of a qualifier): writes
    a named namespace's identifier, not its key, and nothing for a
    namespace level of the own spelling; not affected.

  What a user can declare today: `changed L` for the alias, and a
  declared removal with `created` for a unit keyed by content
  (`removed <using_directive legacy>`, `created <using_directive acme>`):
  both verify, `crates/slip_cli/tests/verify.rs`
  `wp140_redirected_references_declared`. A function whose body differs
  is declared `changed` the same way; one whose profile differs needs a
  removal and a `created` of its two signatures. Each declaration is per
  unit, so the cost grows with the references, not the namespaces. A
  key that names which pre-image namespace an owned reference meant
  (the post-image cannot say, above) or a rule form that states it for
  a set of files is open.
- **Ownership differs between observers** (§2.1). An entity the
  dependency declares only in a header some observers do not include is
  owned in those and a dependency entity in the others. Each comparison
  is consistent with itself; whether the run should compare ownership
  across observers (a count per key is too much; a count per rule of
  entities reached both ways is not exact) is open.
- **Indeterminate units cost a second parse.** Every group with an
  indeterminate unit is in `G` (§4.1), so an observer that has one needs
  unit lists even when nothing changed. Proposed: a map shard stores the
  unit lists of its groups that have an indeterminate unit, which are
  few. To be decided with the verify job (§5) from the corpus's
  `digest.indeterminate` counts.
- **Created units by key** only works where the reducer can compute the
  key: a namespace, a class, a variable, an alias, a using-declaration. A
  created function's key holds its signature's digest and a content-keyed
  unit's holds its own. Proposed: the walk marks units all of whose
  declarations lie inside text the overlay inserted, and `created` becomes
  "marked", with the inserted ranges as the declaration. Needs locations
  in the walk's interface, which is why it is not in M1.
- **`changed` checks little about the unit.** A spelled piece inside an
  exempt unit is checked (§10); nothing else in it is. The design asks
  for more of the edit's own obligations to be checked (the callee at
  this site is now `K`, for a callee the transform did not spell). The
  native rename exempts a pattern that holds a renamed dependent name
  (`spec/edits.md` §9.4), and a dependent name has no reference to
  check; the spelling rule proposed there would remove that use.
- **An obligation no observer resolves** fails the run (§10.3). Whether
  it should be a counted residue instead needs the preprocessing record
  to say why the bytes were not parsed (KR4). `spec/api.md` §7.2 rule
  5 states the rule as it is.
- **One key per spelled piece.** A name that different observers resolve
  to different overloads of a renamed function has one obligation with
  one key, and fails where the other overload is chosen; reduce already
  rejects such a site as `divergent`, which is decided
  (`spec/edits.md` §5.3). An obligation with a set of keys would accept
  it, and is not proposed while that decision stands.
- **What a reference found through a using-declaration refers to** in
  Kiln's table (the using-shadow or its target) decides whether step 4 of
  §10.2 needs to look through it. To be fixed by the first test of
  `crates/slip_digest/tests/spelling.rs` that has one.
- **The written-reference table in the verify parse.** §10.2 needs it
  for every observer that reads a file with an obligation. Its cost on
  the corpus is to be measured with the first run that has obligations.
- **A compatibility declaration that takes the old name** of a relocated
  entity with the same kind and signature (a forwarding function) collides
  with the relocated entity's mapped key. A using-declaration or an alias
  does not (another kind). Supporting the former needs a rule form that
  gives the new declaration a key of its own.
- **Rules match in mapped terms** (§2.1, last point): the false collision
  could be removed by matching on unmapped parents, at the cost of every
  key being computed twice.
- **Created files and pruning** (§5): with the front end's failed lookups
  recorded per observer (a request to Kiln, with KF1), an observer could
  stay `unchanged` when the overlay creates files it could not have found.
- **Warnings.** A new warning in the post-image is information about the
  edit but not a difference of the program. Whether a run may be asked to
  treat it as a finding.
- **Memory and the estimate.** The parses of a job run one after another
  (§5.2), so its peak is that of one parse and the admission estimate of
  the map job is the right one. `spec/exec.md` §6 takes rule 1 from the
  latest record *of the same phase*, so the first verify round falls back
  to rule 2. Proposed: a verify job's estimate is taken from the latest
  map record when no verify record has the observer.
- **Three parses.** An observer with a finding is parsed three times when
  it is localised (post, pre, post). Parsing the pre-image inside the
  closure of the second post-image parse would make it two, with both
  syntax trees in memory; whether Kiln's front end can run inside its own
  closure (thread-local caches, the crash scope of
  `kiln_driver/src/crash.rs`) is not established. Keeping the first
  post-image parse alive across the pre-image parse has the same
  question.
- **Where a node ends.** A trace has marks for where nodes begin (§6.1).
  With an end offset per node the site would be the innermost node that
  contains the first differing byte, which is exact; it costs a second
  hook at every dispatch point of the walk.
- **`used` by list.** A verdict's `used` is a list of keys; the report
  cannot tell which exemption list prevented the finding when a key is in
  two (§7.1). A `Verdict` field per list at the next change of the
  section.
- **A round's parameters are recorded in part.** Since M2 a verify
  shard's `binding` names the `Declared`, the overlay and the round of
  the pre shards (§9.1), and a transform's run takes a round of its own
  (`spec/exec.md` §2). A tree image's roots, `limit` and `locate` are in
  the report only, and a later `slip verify` with the same round number
  replaces the shards. Proposed: the run record's `run` section gains
  them.
- **The tree image rewrites commands** (§5.1). G1, L1, G6, G4 and O2 make it
  safe; how often a word it does not resolve costs an `unverifiable` on
  real build trees is to be seen on the corpus. The overlay (KF2)
  rewrites nothing. Kiln could close the two gaps L1 has: record
  lookups of absolute names, and name the `--embed-dir` directories in
  the log.
- **In place.** The identity image compares the tree as it is with the
  map shards, and can only answer `verified` or, through G2,
  `unverifiable`. A copy of the pre-image kept at map time (or the
  overlay, inverted) would give findings.
- **The pre-image is parsed again** for every observer that does not
  verify at the sums. Storing unit lists for owned files at map time
  (`digest.md` "Open questions") would remove most of those parses and cost
  shard size; to be measured on the corpus.
- **Mapping allocates.** `RuleTable::map` encodes each key's match to
  look it up; with a table that is not empty that is one allocation per
  key of the post-image. To be measured on the corpus with the verify job.
- **`limit` (64) and `locate` (16)** are the defaults for M1; to be
  looked at again with the counts of the first corpus run.
- **Exit status of a `mismatch` that is only indeterminate** (§7): the
  same as any other mismatch. While the walk has areas that are not
  written, that is every observer with a function body.
- **`changed <order>` is coarse.** It exempts every change of
  initialisation order in the translation unit, not only the one a
  declared removal or creation causes. A narrower form (the order unit
  compared with the declared keys taken out of both lists) is to be
  specified.
- **`Finding.key` is optional in the format** and nothing produces a
  finding without one since §4.1 names every finding. Make it required at
  the next change of the section.
- **The `file` of a `changed` finding** can name a group whose copy of the
  key did not change, when that group differs for another key's sake
  (§6). It is the same through groups; a rule that named the group where
  the key's own units differ would need the per-group multiset of the key,
  also exact, at some cost.
- **The kind of a finding for a key with several units** follows the
  differences (§4 step 2): one of two equal copies removed is `removed`,
  and a second, different namespace merged onto a key is `added`. Whether
  a transform author expects to write `removed`/`created` for these is to
  be seen with the first transforms that do it (M3).
- **A derived key with units on one side only** is not a finding even when
  the other side has other units of that key (§4 step 3). Two different
  units of one derived key in one image are a collision and so
  indeterminate (`digest.md` §3.4), which is a finding; the case left is a
  content-keyed derived unit, of which `digest-kinds.md` has none yet.
- **A partial specialisation equal to the primary, removed or added**
  (WP-108, item 3, attempts 3b and 3c). Removing
  `template <class T> struct S<T *> { int a; };` where the primary also
  has `int a` leaves the program the same, and the members of `S<int *>`
  differ: each writes the pattern it was instantiated from, `S<T *>::a`
  on one side and `S<T>::a` on the other (`crates/slip_digest/src/templ.rs`
  `decl_maps`, `instantiated_from`, `:557-562`, for a field and an
  enumerator; `member_spec_info`, `MemberSpecInfo.instantiated_from`,
  `:330-333`, for a member enumeration). The finding is
  `derived_changed`, which nothing can declare: a correct edit the
  verifier rejects. It is kept for now: the verifier errs on the side
  that cannot verify a wrong program. Two ways out are rejected. A
  declared `changed K` must not exempt a derived finding of `K` (step
  3): exemptions are per run, so a `changed box<Mine>::found` would make
  row E11 `bad-6` verify and hide an argument-dependent lookup that
  resolves otherwise in that specialisation in every observer.
  Dropping the pattern's identity from an instantiation's unit needs an
  audit of `digest-kinds.md` §11.4 first: that reference carries the
  facts the instantiated declaration leaves out on purpose (whether a
  default argument exists, initialisers still pending, definitions not
  instantiated), and it is safe to drop only where each such fact, when
  it matters, shows in a derived unit made on use. That would be a
  package of the digest with that audit as its obligation.
