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

Each list is sorted and has no duplicate. Every `KeyId` is in the key space
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

where `map(parent')` is the mapped id of the new parent under the same rule
table (the id of `parent'` itself if no rule reaches it). The walk of the
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

## 3. What is compared

**Shards.** The pre shard is the observer's shard of phase `map`; the post
shard is its shard of phase `verify`, round `r` (the `r`-th application of
edits). Both must be `ok` and carry a `digests` section of the same
`scheme`, the one this build of `slip_verify` implements; and the post
shard's digests must have been computed with the rule table `rules_id`
(§5 records it).

**Units as a multiset.** The units of an image are the triples `(key,
digest, flags)` of all its groups; a triple that occurs in `n` groups has
multiplicity `n`. (Only a unit keyed by content can occur in two groups of
one class: the same `static_assert` written in two files.) Declared units
and the units of the `tu` group are compared together as the **declared
multiset** `D`; derived units as the **derived multiset** `V`.

## 4. Comparison

Let `Dp`, `Dq` be the declared multisets of the pre and post shards.

1. **Indeterminate.** Every unit of `Dp` or `Dq` with the indeterminate
   flag is a finding `indeterminate`. It takes no further part.
2. **By key.** For each key `k` among the remaining units, let `P(k)` and
   `Q(k)` be the multisets of digests of that key in `Dp` and `Dq`.
   - `P(k) = Q(k)`: no finding.
   - both non-empty and different: finding `changed`, unless `k` is in
     `changed`.
   - `Q(k)` empty: finding `removed`, unless `k` is in `removed`.
   - `P(k)` empty: finding `added`, unless `k` is in `created`.

   A key normally has one unit on each side; several arise only from a
   content-keyed unit written in several files (equal digests, a
   multiplicity) and from merged namespaces that are not the same (unequal
   digests). The rule above covers both without naming them.
3. **Derived units.** For each key `k` that both `Vp` and `Vq` have: if any
   of its units is indeterminate, finding `derived_indeterminate`; else if
   the digest multisets differ, finding `derived_changed`. A derived key
   that only one image has is **not** a finding. It is counted
   (`verify.derived.only_pre`, `verify.derived.only_post`).

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
finding with less to say. What must not be skipped is a derived unit that
both have and that differs: the same specialisation instantiated to
something else is exactly how a dependent name that resolves differently
shows up ([03](../docs/design/03-equivalence.md) §7 row 6), and no declared
unit need differ then.

This argument assumes the front end's demand is a function of the program.
The demand-independence test of `digest.md` §14 is what checks it.

### 4.1 Through groups

Unit lists are not normally in a shard. The comparison is carried out on
group sums first (`digest.md` §11):

1. If the two `tu` digests are equal, the unit multisets are equal, flags
   included. The observer is verified if `digest.indeterminate` is 0; else
   it has that many `indeterminate` findings, unnamed until unit lists are
   produced.
2. Otherwise let `G` be the groups, identified by `(class, file)`, whose
   sum is not the same in both shards, a group present in one shard only
   included. The rules of §4 are applied to the units of the groups of `G`
   alone.

Restricting to `G` is exact, not an approximation: a group outside `G` has
the same units, with the same multiplicities, in both images, and adding
equal multisets to both sides of each comparison of §4 changes no outcome.
(This is why §3 counts a unit once per group instead of once per image.) An
entity whose first declaration moved from one file to another is in two
groups of `G`, once on each side, and compares equal.

Step 2 needs the unit lists of the groups of `G` from both images. §5 says
how the verify job obtains them.

## 5. The verify job

One job per observer and round, run by a worker (`spec/exec.md` §5), given
the observer, the round, the post-image and `Declared`:

1. Parse the post-image and digest it with `Declared.rules`. If the parse
   is not `ok`, the shard has that outcome and no verdict: the observer is
   `not_parsed` (§6).
2. Read the pre shard from the store. If it is absent, not `ok`, has no
   `digests` section or has another `scheme`, the verdict is `unverifiable`
   with the reason.
3. Compare group sums (§4.1). If the verdict is reached there, write it.
4. Otherwise parse the pre-image again and digest it with no rules and
   with detail for the files of `G` (the pre-image is the subject itself;
   edits exist only in the overlay). Its group sums must equal the stored
   pre shard's: if not, the subject changed under the run or the front end
   is not deterministic, and the verdict is `unverifiable` saying so.
5. Apply §4 to the units of `G`, then localise (§6.1) the first findings.

The post shard carries the `digests` section of the post-image, with the
unit lists of the groups of `G` (detail `differing`, `digest.md` §11), and
the `verdict` section. Its content is a function of the post-image's
inputs, the pre shard and `Declared`.

The second parse happens only for an observer that does not verify at the
level of sums. In a run that verifies, every observer is parsed once.

**Unchanged observers.** An observer is **unchanged** in a round if the
overlay modifies, deletes or creates no file at all, or if (a) no file in
the `inputs` of its pre shard is modified or deleted and (b) the overlay
creates no file. Its post-image is its pre-image byte for byte; it gets no
job and the verdict `unchanged`, whatever the rules are: rules are a device
for comparing, not a property of a program that did not change. Condition
(b) is there because a created file can be found by an `#include` or a
`__has_include` that found another file, or nothing, before; a shard does
not record the lookups that failed ("Open questions"). A run may be asked
to verify unchanged observers anyway (this is how invariants (a) and (b)
are run); it must then be given no rules.

## 6. Verdicts and findings

Section 21 of a shard of phase `verify` (`spec/model.md` §8), present when
the outcome is `ok`:

```
Verdict := kind:      VerdictKind
           pre:       digest            content id of the pre shard compared against; zero if there was none
           rules:     digest            rules_id
           reason:    str               for `unverifiable`; else empty
           only_pre:  uv                derived keys only the pre-image has; 0 when the comparison ended at the sums
           only_post: uv
           used:      list<KeyId>       exemptions that prevented a finding, sorted
           total:     uv                number of findings
           findings:  list<Finding>     the first `limit` of them (default 64) in the order below

VerdictKind := verified = 0 | mismatch = 1 | unverifiable = 2

Finding := kind:  FindingKind
           key:   opt<digest>           absent only for an unnamed `indeterminate` (§4.1 step 1)
           file:  bytes                 the group's file, pre-image group if there is one; empty if unknown
           pre:   opt<digest>           the unit's digest in each image
           post:  opt<digest>
           name:  str                   the entity as a person would write it; for the report only
           where: opt<Where>            §6.1

FindingKind := changed = 0 | removed = 1 | added = 2 | indeterminate = 3
             | derived_changed = 4 | derived_indeterminate = 5
```

Findings are ordered by kind as listed, then by `file`, then by `key`.
`mismatch` if there is a finding, else `verified`. An observer whose post
shard is not `ok` has no verdict and is `not_parsed`; an observer without a
job is `unchanged`. Those five are the verdicts of an observer.

A difference in the number of warnings between the pre and post shards is
not a finding ("Open questions").

### 6.1 Localisation

A finding of kind `changed` or `derived_changed` says that two streams
differ. To say where, the walk can produce, for a named unit, a **trace**:
the unit's stream, and for each node of it (`digest.md` §4) the offset at
which the node starts, its tag, and the source location of the syntax it
was made from. The job takes the traces of the unit from both images (both
parses are in memory at step 5), finds the first byte at which the streams
differ, and in each trace the innermost node that contains that byte.

```
Where := pre:  Site, post: Site
Site  := file: bytes, line: uv, column: uv      subject path; 0, 0 if the node has no location
         node: str                               the node's kind, by name
         what: str                               the differing field, rendered: for a reference, the entity's name
```

Localisation is for people. It is outside the trusted base: no verdict
depends on it, and a `Where` that points at the wrong token is a defect of
the report, not of the verification. It is done for at most the first
sixteen findings of an observer that can have one. `added`, `removed` and
`indeterminate` findings carry the location of the entity's first
declaration in the image that has it, in the corresponding `Site`, with the
other one empty.

## 7. The run

A run of the verify phase over a plan and a round has the verdict:

- **verified**: every observer is `verified` or `unchanged`;
- **not verified**: some observer is `mismatch` or `not_parsed`;
- **incomplete**: neither of those; some observer is `unverifiable`, or has
  a shard of status `failed`, or the run was cancelled.

The run's report lists the count of observers per verdict, every finding
grouped by `(kind, key)` with the observers that have it (a mismatch in a
shared header is one line, not one per observer), every exemption with the
number of observers that used it, and the exemptions no observer used.

Counters of a verify shard, added to those of `digest.md` §12:

| Counter | Meaning |
|---|---|
| `verify.groups.differing` | groups of `G` |
| `verify.findings` | findings (`total`) |
| `verify.derived.only_pre`, `verify.derived.only_post` | derived keys in one image only |
| `verify.exemptions.used` | exemptions that prevented a finding |
| `verify.reparsed` | 1 if the pre-image was parsed again |

## 8. What this does not show

- **Only analysed observers.** A configuration that is not in a database,
  a branch of `#if` no observer takes, and a template nothing instantiates
  are not verified. The coverage report says which
  ([03](../docs/design/03-equivalence.md) §9).
- **One front end.** Both images are resolved by Kiln. An error Kiln makes
  the same way twice is invisible here; tiers T2 to T4 of
  [03](../docs/design/03-equivalence.md) §8 exist for that.
- **Agreement between observers.** Each observer is verified against its
  own pre-image. If one declaration of a relocated entity is edited and
  another, in a file other observers use, is not, each observer can verify
  (in one the entity has its new name, in the other its old one and nothing
  to collide with), and the program no longer links. The condition that
  rules it out is: an entity's key is reached through a rule in every
  observer that has it, or in none. Checking it needs, per unit, whether a
  rule was applied to it or to an enclosing entity, which the `digests`
  section does not hold yet ("Open questions"). Until it does, this is
  left to the reducer, which sees every observer's declarations, and to
  tier T3.
- **Text.** Strings, comments and anything else that spells a name without
  referring to it ([03](../docs/design/03-equivalence.md) §7 rows 14, 15).

## Tests

- `crates/slip_verify/tests/compare.rs` — §4, over unit lists written by
  hand: every rule of every step, each exemption preventing exactly its
  finding and no other, multiplicities, two units of one key, derived keys
  on one side, the order of findings.
- `crates/slip_verify/tests/groups.rs` — §4.1: for generated pairs of
  images (random units, groups, moves between groups, duplicates), the
  outcome through groups equals the outcome over all units.
- `crates/slip_verify/tests/declared.rs` — §2: the `DECL` container round
  trip; rejected forms (unsorted, duplicate, two rules with one match).
- `crates/slip_digest/tests/rules.rs` — §2.1: mapped keys under rules, one
  directory per case: a relocated namespace, a merged namespace, a renamed
  member, a swap, a collision, a rule that captures a pre-existing entity.
- `crates/slip_cli/tests/verify.rs` — §5, §6, §7 through the executable:
  the identity (verified, one parse per observer), an edit that changes a
  callee (mismatch, localised), an edit that breaks the parse, a pre-image
  that did not parse, an unchanged observer, the report.
- `spec/pitfalls.md` and its tests — each catalogued hazard as a correct
  edit that verifies under its rules and a wrong edit that does not.

## Open questions

- **Agreement between observers** (§8). Proposed: a per-group digest of the
  keys of the group that were reached through a rule, so that observers
  sharing a header can be compared group by group without unit lists, and
  a reduction over distinct groups. To be specified with reduce (M2).
- **Created units by key** only works where the reducer can compute the
  key: a namespace, a class, a variable, an alias, a using-declaration. A
  created function's key holds its signature's digest and a content-keyed
  unit's holds its own. Proposed: the walk marks units all of whose
  declarations lie inside text the overlay inserted, and `created` becomes
  "marked", with the inserted ranges as the declaration. Needs locations
  in the walk's interface, which is why it is not in M1.
- **`changed` checks nothing about the unit.** The design asks for the
  edit's own obligations to be checked instead (the callee at this site is
  now `K`). Not specified.
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
- **Memory.** Step 5 holds two parses of one translation unit. The
  admission estimate of a verify job (`spec/exec.md` §6) should be that of
  its map job for step 1 and twice that once step 4 starts; the protocol
  has no way to raise an estimate in the middle of a job.
- **The pre-image is parsed again** for every observer that does not
  verify at the sums. Storing unit lists for owned files at map time
  (`digest.md` "Open questions") would remove most of those parses and cost
  shard size; to be measured on the corpus.
- **`limit` and the sixteen** of §6 are guesses.
