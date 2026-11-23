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
```

Each list is sorted and has no duplicate (rules by the bytes of their
encoded `(parent, kind, name, disc)`, keys by their bytes); a key may be in more than one of
the three exemption lists. Every `KeyId` is in the key space
of the pre-image: for an entity a rule renames, the key it had before. The
container form is kind `DECL`, version 1, one required section `declared`
(id 1) holding this record; its JSON rendering follows `spec/model.md` §10.
`rules_id = H("slip.rules.v1", rules)` names the rule table.

With all four lists empty the claim is plain equality. That is the check of
corpus invariants (a) and (b).

### 2.1 Rules

A relocation "the entity with key `K = (parent, kind, name, disc)` is now
`(parent', name')`" is declared as the rule that **undoes** it:

```
Rule { parent: map(parent'), kind, name: name', disc,  to_parent: parent, to_name: name }
```

where `map(parent')` is the new parent as the walk maps it under the same
rule table (`parent'` itself if no rule reaches it; `root`, `c_linkage` and
`builtin` map to themselves). Both parents of a rule are `Parent`s
(`digest.md` §3.4). The walk of the
post-image applies the rules bottom-up (`digest.md` §3.4), so the relocated
entity, its members, and every type and signature that mentions it are
written with the keys they had in the pre-image, and the comparison below
needs no map.

- A rule is applied to an entity at most once; rules do not chain. Renaming
  `f` to `g` and `g` to `f` in one run is two rules.
- **Namespaces merge.** A rule of kind `namespace` may give its entity the
  key that another namespace of the post-image has: `acme` mapped to
  `legacy` while `legacy` still exists for the entities that stay in it.
  The two are then one unit (`digest.md` §3.4); if their digests differ
  there are two units with one key, and the comparison reports it (§4).
  For every other kind, two entities with one mapped key are a collision
  and both are indeterminate.
- Rules match in mapped terms. After `acme` is mapped to `legacy`, a rule
  with parent `legacy` matches members of either namespace. A rule that
  renames a relocated member to a name that a staying member of the old
  namespace has therefore collides, and is reported, although the edit may
  be correct ("Open questions").

**Why applying rules to the post-image is sound.** Let `φ` be the function
from the entities of `B` to keys that the walk computes. If no unit is
indeterminate, `φ` is injective except on merged namespaces. Equal unit
sets then say that `φ` is a bijection between the entities of `B` and those
of `A` (namespaces aside) under which every unit's content is the same,
every reference inside a unit included, because references are written as
mapped keys. That is an isomorphism of the two resolved programs; the rules
only choose which isomorphism is tried. A wrong or missing rule cannot make
two different programs compare equal: it changes which keys the post-image
gets, and any key that then fails to line up is a reported difference.

Two namespaces sharing a key is the one place `φ` is not injective. A
namespace's unit holds only what the namespace itself is (`digest.md` §5),
its members are units of their own with keys that do collide if two of
them coincide, and no generated code depends on which namespace an entity
is in except through names, which is what a relocation declares.

A pre-existing entity that a rule would capture is caught by the same
comparison: if the pre-image already had an entity with the key a rule
matches, the post-image's entity of that key is mapped away, and the
pre-image's unit has no counterpart (`removed`).

### 2.2 Exemptions

- `changed`: a unit with this key may have any digest in either image. It
  must still exist in both.
- `created`: a unit with this key may be absent from the pre-image.
- `removed`: a unit with this key may be absent from the post-image.

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
| `created` | the post-images | the rules this file gives |

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
So a line whose new parent another line relocates comes after that line.

**Errors**, each naming the line number. One error is reported: the
first `Syntax` error in file order if the text has one; else the first
error in file order among the `relocate`, `changed` and `removed` lines;
else the first among the `created` lines, which are resolved last because
they need the rules the file gives:

| Error | When |
|---|---|
| `Syntax` | an unknown keyword; a missing `<entity>`; a `relocate` without ` => `, without `::`, or with a `<name>` that is neither an identifier nor `=`; an unknown `[<kind>]` |
| `NoUnit` | the name resolves to no unit; the message lists up to eight unit names of the set that contain the text's last component |
| `Ambiguous` | units of that name have more than one `KeyId`; the message lists their kinds |
| `Fallback` | the unit has a fallback key: there is no `Key` to make a rule from, and no stable key to exempt |
| `NotRenamable` | `relocate` with an identifier for an entity whose `Name` is not an identifier |
| `Duplicate` | two `relocate` lines give rules with one `(parent, kind, name, disc)` (one entity relocated twice to one place, or two entities of one kind and discriminator relocated to one place; two overloads to one name have two discriminators and are not); two lines of one exemption list give one key. Reported at the later line |

A file with no line gives `Declared::none()`.

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
pub fn records(input: &Input<'_>, rules: &RuleTable) -> Result<Vec<Record>, String>;

// slip_cli::declare
pub struct Entity { pub kind: Option<KeyKind>, pub text: String }
pub enum Line { Relocate { entity: Entity, parent: Option<Entity>, name: Option<String> },   // None: the global namespace; `=`
                Changed(Entity), Created(Entity), Removed(Entity) }
pub enum ErrorKind { Syntax, NoUnit, Ambiguous, Fallback, NotRenamable, Duplicate }
pub struct LineError { pub line: u64, pub kind: ErrorKind, pub detail: String }
pub fn read(text: &str) -> Result<Vec<(u64, Line)>, LineError>;            // line numbers from 1
pub struct Needs { pub post: bool, pub post_mapped: bool }                 // which post-image parses the table asks for
pub fn needs(lines: &[(u64, Line)]) -> Needs;
pub struct Resolved { pub rules: RuleTable, pub changed: Vec<Digest>, pub removed: Vec<Digest> }
pub fn resolve(lines: &[(u64, Line)], pre: &[Record], post: &[Record]) -> Result<Resolved, LineError>;
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

Steps 1 and 2 are over `D` and step 3 over `V`, independently: a key set
aside in one is not thereby set aside in the other. (A key is in both
only if an entity is declared in one image and derived in the other, or
by a collision.) A key declared in one image and derived in the other is
in `D` on one side and in `V` on the other: step 2 gives `removed`
(declared before) or `added` (declared after), which its exemption may
prevent, and step 3 counts it in `only_post` or `only_pre`.

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
says about roots, the post command and guard G1 is about that form only.
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
  root of it or lies under one (`/a/b` lies under `/a`; `/a/bc` does not).
  Every root is UTF-8, since it becomes part of a command line Kiln
  reads, and no `post` contains `=` (the post command below). An image
  that breaks one of these is refused before any job runs
  (`spec/exec.md` §8.7).
- An image with no root map is the **identity**: the post-image is the
  subject as it is on disk now.
- The file whose real path is `s/x`, for a subject root `s` with post root
  `p`, is in the post-image the file `p/x`. A post root holds the whole
  tree: a file that is not in it is a file the edit deleted. A file under
  no subject root is the same file in both images.
- The **subject path** (`digest.md` §11) of a real path `p/x` is `s/x`; of
  any other real path, the path itself.

**The post command** is the observer's `Command` (`spec/model.md` §7)
with:

- `directory` (a path as the database gave it, compared as bytes) replaced
  by its image if it is a subject root or starts with one followed by `/`,
  else kept;
- `argv[0]` kept;
- in every other word of `argv`, each occurrence of a subject root that is
  followed by `/` or by the end of the word replaced by its post root.
  Occurrences are taken from the left and do not overlap; no two roots
  match at one place, since none lies under another;
- then, after the last word, one word `-fmacro-prefix-map=<post>/=<subject>/`
  per root map, in the image's order;
- `env` kept.

With the identity image the post command is the observer's command.

The added words make `__FILE__` and the other spellings of a file's path
those the file will have once the edit is applied to the subject, which
is the program the claim of §1 is about; without them a constant computed
from such a path (`sizeof(__FILE__)`) would differ for every file of the
tree. Kiln replaces the prefix of the first entry that matches, as a
plain string prefix (`kiln_source/src/manager.rs:627`), so the command's
own `-fmacro-prefix-map=` and `-ffile-prefix-map=` entries, which come
first and were rewritten with the rest, keep their effect, and the
trailing `/` keeps `/p` from matching `/pq`. The mapping cannot make two
different files spell alike: it is one to one on the paths under the post
roots, and G1 (§5.3) excludes a post-image parse that read the subject.

The rule is textual because a command line is text: `-I/s/include`,
`--sysroot=/s/x` and `-DROOT="/s"` are all rewritten. So is a subject root
that happens to occur inside a longer path (`/mnt/s/x` for the root `/s`),
which then names nothing, or something else; and a path that reaches the
subject without spelling a root (a relative `-I../src` from a directory
outside every root, a symbolic link) is not rewritten at all. Neither can
give a wrong `verified`: guard G1 (§5.3) refuses a post-image parse that
read any file of the subject, so what was compared is the post-image or
nothing. They cost a failed parse, a difference or an `unverifiable`.

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
- **What guards it.** G1 guards the textual rewrite and has no meaning
  here: there is no subject root, and reading a subject file that the
  overlay does not name is reading the post-image. In its place are the
  two guards O1 and O2 of §5.3, over the `inputs` of step 1: an overlaid
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
   the subject paths of §5.1, and detail `differing` from the pre shard's
   group sums (`digest.md` §11; detail `none` if the pre shard is not
   usable). With an image of the overlay form this is the last parse of
   "Loading" (§5.1). If the parse is not `ok`, the shard has that
   outcome, no `digests` and no `verdict`: the observer is `not_parsed`
   (§7), and the job ends. In the same parse, after the walk: the rule
   use of §9.1 when `Declared.rules` is not empty, and the checks of
   §10.2 when the overlay has obligations.
2. Check, in this order: the pre shard is usable; guard G1 (tree form)
   or guards O1 then O2 (overlay form); `comparable` (§3). The first
   that fails makes the verdict `unverifiable` with the reason of §5.3,
   and the job ends.
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
| G1 | step 2, tree form: no entry of the post-image parse's `inputs` is a subject root or lies under one | `post-image: read <path>, a file of the subject` (the first such entry) |
| O1 | step 2, overlay form: every entry of the post-image parse's `inputs` whose path the overlay names has the overlay's `post` size and content digest | `post-image: <path> was not read through the overlay` (the first such path, in path order) |
| O2 | step 2, overlay form: every entry of the post-image parse's `inputs` whose path the overlay does not name and the pre shard's `inputs` also list has the size and content digest the pre shard gives it | `pre-image: <path> changed since the map phase` (the first such path, in path order) |
| C | step 2: `comparable(pre, post)` | `digests: <its message>` |
| R1 | step 4: the pre-image parse is `ok` | `pre-image: no longer parses (<status>): <message>` |
| G2 | step 4: its `inputs` are the pre shard's `inputs`, entry for entry | `pre-image: <path> changed since the map phase` (the first path, in path order, whose entry is not the same in both lists or is in one only) |
| R2 | step 4: its `digests`, unit lists apart, equal the pre shard's | `pre-image: its digests are not those of the map shard` |
| G3 | step 6: the second parse of the post-image gives the `tu` digest of the first | `post-image: changed during the job` |

`<status>` is the name of the status (`diagnosed`, `rejected`, `failed`);
`<message>` the outcome's message; a path is written as its bytes where
they are UTF-8, else with each other byte as `\xNN`. The reason is then
cut as §6 says.

- **G1** is what makes the textual rule of §5.1 safe. With the identity
  image there is no subject root and it holds trivially.
- **O1 and O2** say that what was parsed is the post-image the overlay
  describes over the pre-image the map phase saw: the overlay reached
  the front end for every overlaid file it read, and no other file
  changed under the run. O2 is checked even when the comparison ends at
  the sums, where G2 is never reached.
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
| a specialisation | the template's name and its canonical arguments as Kiln prints them, except that a null pointer argument whose type is not `std::nullptr_t` is written `(T)nullptr`: `ns::S<int>`, `N<(int *)nullptr>` |
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
                                      `StmtKind`; for a unit's first mark, the name of its key kind
         file:   bytes, line: uv, column: uv      subject path and presumed line and column of the node's
                                                  location; empty, 0, 0 if it has none
RefAt := offset: uv, len: uv          a digest field of the stream (32 bytes) whose value is
         key:   KeyId                 the key of a unit of this image: a reference to it
```

- A mark is made where the walk begins a unit (offset 0, the location of
  the unit's first declaration) and where it begins a statement or an
  expression inside one. A declaration's own rows (its type, its
  specifiers, each redeclaration) have no mark: a difference there is at
  the unit's mark or at the last expression before it. Types and constant
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
| `added` | `post`: the unit's first mark; `pre`: the empty site |
| `indeterminate`, `derived_indeterminate` | in each image that has an indeterminate unit of the key: the unit's first mark, with `what` the cause as the walk prints it (`unsupported ExprKind/CXXReflectExpr`, `collision`, `pending stmt`); the empty site in the other |

The empty site is an empty `file`, line and column 0, an empty `node` and
an empty `what`. A unit's first mark is the mark at offset 0. Where an
image has several units of the key, the one with the smallest digest is
taken.

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
pub struct Traced { pub output: Output, pub traces: Vec<Trace> }           // traces sorted by (key, digest)
/// `output` is `digest(input, options)`, byte for byte; `traces` has one
/// entry per unit whose key is in `keys` (sorted, no duplicates).
pub fn trace(input: &Input<'_>, options: &Options<'_>, keys: &[Digest]) -> Traced;

// slip_cli::locate
/// What localisation needs from one image, taken while its parse is in memory.
pub struct Side { /* the `tu` digest; names, causes and traces of the wanted keys */ }
impl Side { pub fn tu(&self) -> &Digest; }
/// One walk of `input` under `rules` for the keys of the findings to localise.
pub fn take(input: &Input<'_>, rules: &RuleTable, keys: &[Digest]) -> Side;
/// The keys of the first `locate` findings, sorted, no duplicates.
pub fn wanted(findings: &[Finding], locate: usize) -> Vec<Digest>;
/// Fills `name` and `location` of the first `locate` findings; changes no other field.
pub fn fill(findings: &mut [Finding], locate: usize, pre: Option<&Side>, post: &Side);
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
  `failed`; empty if there is no shard) or the empty string (`mismatch`);
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
    /// One call per selected observer, in plan order.
    pub fn add(&mut self, observer: &Digest, main: &[u8], verify: Option<&Shard>, map: Option<Status>);
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
- **Sums are believed.** A group's `count`, `indeterminate` and `digest`
  are checked against its unit list only where there is one. A shard
  whose sums were not computed from its units (a defect of the walk, or a
  shard written by something else) can verify wrongly at §4.1 step 1.
  The walk that writes sums is as much trusted base as the comparison.
- **Text.** Strings, comments and anything else that spells a name without
  referring to it ([03](../docs/design/03-equivalence.md) §7 rows 14, 15).
- **Derived inputs.** A file the build generated from the subject (a
  table, a configuration header, a module file) lies in the build tree,
  outside every subject root, and is the same file in both images: the
  edit's effect on it is not seen until it is generated again from the
  post-image, which Slip does not do. A generated file that includes a
  subject file by an absolute path trips G1 (§5.3) for every observer
  that reads it.
- **The command.** Both images are parsed with one command, up to the
  roots (§5.1). An edit to the build (a flag, a definition, an include
  path) is not an edit to the subject and is not verified.
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
still is what `r` says was renamed.

```
RuleUse := rules: digest            rules_id
           uses:  list<Use>         sorted by `rule`; only rules with `via` or `direct` not 0
Use     := rule: uv, via: uv, direct: uv

Binding := overlay:  opt<digest>    content id of the OVLY container, for an image of the overlay form; absent for a tree
           declared: digest         content id of the DECL container the job was given
           pre:      uv             the round of the pre shard (§5.2 step 0)
```

Sections of a shard of phase `verify` (`spec/model.md` §8): **24,
`binding`** and **25, `rule_use`**. (Ids 22 and 23 are not assigned; 26
is `spellings`, §10.4.)

- `binding` is present in every shard a verify job writes, whatever the
  parse's outcome, and is taken from `Job.post`. A shard the coordinator
  wrote for a worker that produced none (status `failed`) has no
  `binding`.
- `rule_use` is present when the outcome is `ok` and the rule table is
  not empty, and absent otherwise. Invariants: `uses` strictly ascending
  by `rule`; no `Use` with both counts 0.

With the binding a verify shard says which declared changes, which
overlay and which pre shards it was made with; M1's shards did not
("Open questions").

### 9.2 Stale observers and unused rules

**Stale.** An observer is **stale** for a rule `r` whose kind is not
`namespace` if its `direct(r)` is not 0. (For a namespace, an entity that
keeps the key a rule maps another onto is the merge of §2.1.) A stale
observer is one finding of the run, `stale`, with the rule and the
observer.

**Why this is the condition.** The claim of §1, for a rule, is that the
entity it names has the new name. Two observers disagree about an entity
when one reaches its key through the rule and the other has it without.
`direct(r) = 0` in every observer says no observer has it without: every
observer that has the entity reached it through the rule, or no observer
has it at all. In an observer that has the entity both ways the two
collide and both are indeterminate (§2.1), so that case was already a
finding; the count catches the observer that has only the old one.

**Unused rules.** A rule with `via` and `direct` 0 in every observer names
an entity no observer has. It is reported with the run, like an exemption
no observer needed (§2.2), and changes no verdict.

### 9.3 What is added to the run (§7)

**This run's shard.** To the condition of §7 (its content id is the
`shard` of the observer's last `Job` entry in the run's record) M2 adds:
if a worker wrote it (its status is not `failed`), its `binding` is the
run's (the same `declared`, the same `overlay` or the same absence of
one, the same `pre`); and if it is `ok` and the run's rules are not
empty, it has a `rule_use` whose `rules` is the run's `rules_id`. A shard
that fails either is not this run's: the observer is `failed` (§7), as
for any shard another run left.

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
               "stale": [ { "rule": <index>, "observer": <id>, "main": <path>, "direct": <n> }, … ],
               "unused": [ <index>, … ] }
"spellings": …                                             §10.5
```

`stale` is sorted by `(rule, the observer's position in the plan)` and
holds every stale pair; `unused` is ascending.

Lines added to the text, after the `exemption` lines and before `wall`:

```
image overlay <overlay id>                       in place of the `image` lines of §7.1, for the overlay form
agreement rules <n> stale <n> unused <n>         when there are rules; `stale <n>` counts pairs
stale rule <index> direct <n> <main file>        at most 100, then `stale: <n> more (see the report)`
unused rule <index>
```

Nothing is added to the lines on standard error: those list observers by
state, and a stale observer's state is whatever its verdict made it.

### 9.5 Interface

The walk's half, in `slip_digest` (trusted base):

```rust
// Options gains `rule_use: bool`; Output gains `rule_use: Vec<slip_model::Use>`  (empty when off or when there is no rule)
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
pub struct Stale { pub rule: u64, pub observer: Digest, pub direct: u64 }

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
piece of one accepted edit, and `key` is the `KeyId` the piece carries.
Apply derives the range when it composes the post-image
(`spec/edits.md` §7.1). The obligations of a run are the `spellings`
list of its overlay, sorted by `(file, lo, hi, key)` with no two equal;
an obligation is named by its index in that list. An image of the tree
form has no obligation.

### 10.2 In one observer

In the parse of §5.2 step 1, after the walk, for every obligation whose
file is in the parse's `inputs`:

1. The **written references** of the unit are the entries of Kiln's
   written-reference table: the names that were resolved while the unit
   was parsed, in the roles expression name, member name, qualifier
   component, type name, template name, base specifier and member
   initialiser (the roles of `spec/api.md` §6.5 rule 3; a role Kiln adds
   later takes part from the day `spec/api.md` announces it). Entries
   equal in role, name range and referent are one reference.
2. A reference's **name range** is the file range of its name: the bytes
   of one file that translation turned into exactly the name's tokens,
   at tier 0 or 1 (`spec/api.md` §5.4; `slip_kiln::ranges`, which is
   Kiln's `editable_file_range`, `spec/edits.md` §2.2). A reference
   whose name has no such range takes no part.
3. A reference **answers** the obligation if its name range is in the
   obligation's file, ends at `hi` and starts at or after `lo`. (A
   spelling `::a::b::f` is answered by the reference `f`; `a` and `b`
   end before `hi`.)
4. For each answering reference take the **origin** of its referent: the
   referent itself, or, for a specialisation of a template or a member
   of an instantiated class, the template or the member of the template
   it was made from. Take the origin's `KeyId` under the run's rules
   (`Declared.rules`; mapped keys, `digest.md` §3.4), which is its key in
   the pre-image's key space.
5. The obligation, in this observer:
   - **holds** if at least one reference answers and every answering
     reference's origin has the obligation's `key`;
   - **fails** if some answering reference's origin has another key, or
     no key (it is not a unit, is keyed by content, or its key is
     indeterminate);
   - is **not seen** if no reference answers: the bytes are in a
     conditional group the observer skips, in a macro argument the macro
     does not use, in a place whose name is resolved only at
     instantiation, or in a role the table does not record.

A name in a macro argument that is substituted twice gives two
references with one name range; each must have the key.

A failed obligation does not change the observer's `Verdict` (§6): the
comparison is of digests and stays what it is. It is recorded beside the
verdict (§10.4) and judged over the run.

### 10.3 Over the run

For each obligation of the overlay, over this run's `ok` shards (§7,
§9.3):

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
                                     in unit order that does not have the obligation's key; absent if it has none
          name:  str                 that entity as a person would write it (§6.1); for the report only
```

Invariants: no index is in `held` twice or in both lists; the number of
indices in the two lists is at most `seen`. The obligations not seen are
the `seen` minus those. The decoder cannot check an index against the
overlay; the fold (§10.6) refuses an index that is not one of the
overlay's obligations or whose file the shard's `inputs` do not list, as
a shard that is not this run's.

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
/// the result is parallel to it.
pub fn check_spellings(input: &Input<'_>, rules: &slip_model::keys::RuleTable,
                       obligations: &[Obligation]) -> Vec<Spelling>;
```

It gets keys from `Keys::with_rules` (§9.5), so the key it compares is
the key the walk of the same parse gave the entity. The verify job
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
    /// does not have or a file the shard did not read (§10.4).
    pub fn add(&mut self, position: usize, inputs: &[FileRef], section: &slip_model::Spellings) -> Result<(), String>;
    pub fn finish(self) -> Vec<Outcome>;
}
```

### 10.7 What it shows

For a name written as a spelled piece: in every observer that resolves
the name where the piece landed, it denotes the entity whose key the
piece carries, and at least one observer resolves it. It does not show
that the edit was right to name that entity, and it shows nothing about
a literal piece, a copy, or a name the front end does not record a
reference for (a declaration's own name, a dependent name, a name in a
using-declaration). Those are judged by the comparison of digests alone.

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
  `locate`; a `CompareError` as a failed job.
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

- **Agreement costs a job per observer** (§9, §5): with rules, no
  observer is unchanged. `spec/edits.md` ("Open questions") proposes a
  filter of key ids in the map shard so that an observer that cannot have
  a renamed entity is shown not to, without a parse.
- **Agreement for namespaces** (§9). A relocated namespace merges with
  the one that stays, so `direct` says nothing for it. The members that
  move have rules of their own only if the transform declares each; how
  a partial relocation is shown to agree is to be specified with the
  namespace migration (M4).
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
- **The tree image rewrites text** (§5.1). G1 makes it safe; how often it
  costs an `unverifiable` on real build trees is to be seen on the
  corpus. The overlay (KF2) rewrites nothing.
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
