# edits — claims, reduce, apply, the verify loop, native rename

Status: draft (M2). Implemented by `slip_model` (the formats), `slip_reduce`
(§5, §6), `slip_apply` (§7), `slip_refactor` (§9), `slip_kiln` (§2.3, §8.4)
and `slip_cli` (§8, §10). Design rationale:
[02-pipeline.md](../docs/design/02-pipeline.md),
[04-macros-and-configurations.md](../docs/design/04-macros-and-configurations.md).
Formats use the encodings and the container of [model.md](model.md); keys,
units and rules are those of [digest.md](digest.md); what "verified" means
is [verify.md](verify.md).

Nothing in this file is trusted. A transform, the reducer and the applier
can each be wrong; what catches them is verification, which takes from them
only the post-image and the declared changes (`verify.md` §2). No rule
below makes a wrong edit safe. Each one either makes a wrong edit less
likely, or makes it be reported earlier and more precisely than
verification would report it.

## 1. Principles

1. **Edits are claims.** An observer does not edit. It claims that certain
   bytes of a file should be replaced, or must stay, or cannot be edited.
   Reduce decides from all claims; apply performs; verification judges.
2. **A claim is addressed to bytes of a file of known content.** The file is
   named by real path and content digest (`model.md` §6). No edit is ever
   applied to a file whose bytes do not have that digest.
3. **Determinism.** The edit set, the declared changes, the overlay, the
   post-image and the result are functions of the plan, the transform
   request and the contents of the files read. They do not depend on the
   number of workers, on the order in which observers finish, on the time,
   or on the store's path.
4. **All or nothing per group.** Claims belong to atomic groups. A group
   is applied whole or not at all. A native rename is one group.
5. **Conservative, single pass.** Reduce never applies part of a group, and
   never iterates to rescue what remains after a rejection ("Open
   questions").
6. **Byte exact.** Apply changes the claimed bytes and no others. It does
   not reformat, re-encode, or normalise line endings.
7. **The working tree is not touched** until a result permits it and the
   user asks (§7.3).

## 2. Files, ownership and tiers

### 2.1 Ownership

A transform request carries the **owned set**: the files Slip may edit.

```
Ownership := owned:    list<path>     real paths of directories or files; sorted, no two equal
             readonly: list<path>     the same; sorted, no two equal; none also in `owned`
```

An entry **covers** a real path `p` if it equals `p` or is a proper prefix
of `p` ending at a `/` of `p`. A file is **owned** if some entry covers it
and the longest entry covering it is in `owned`. Every other file is
**read-only**: system headers, dependencies, generated files in a build
tree, and whatever `readonly` carves out of an owned directory.

`owned` must not be empty. Entries are made real paths when the request is
built (§10.1); an entry that does not exist is an `Input` error.

### 2.2 Tiers

The **tier** of a token the parser saw
([04](../docs/design/04-macros-and-configurations.md) §2):

| Tier | The token's bytes are | M2 |
|---|---|---|
| 0 | written in a file, outside every macro invocation | editable |
| 1 | written in a file, inside the argument list of a macro invocation, and reached the parser only by parameter substitution | editable; every substitution is a vote (§4.3) |
| 2 | written in the replacement list of a macro definition | refused (`macro_body`) |
| 3 | made by `##` or `#`, or by a builtin macro | refused (`pasted`) |
| X | not one token of one of the above: a name whose tokens come from different expansions, or from a buffer that is not a file (command-line definitions, predefines) | refused (`crossing`, `not_a_file`) |

A token is tier 1 only if every expansion between its bytes and the parser
is the substitution of a parameter by an argument. An argument that is
itself the name of a macro whose body holds the token gives a tier 2
token.

Tier 0 and tier 1 are properties of an observer, not of the bytes: the
same bytes are an argument in an observer where the name before `(` is a
macro and plain text where it is a function. Reduce records every tier a
site was claimed at (§5.4).

### 2.3 File ranges

A claim's range is `[start, end)` in byte offsets of the file's bytes: the
bytes its content digest is computed over. `slip_kiln` maps a Kiln location
range to a file range ([KF3](../docs/requests/KF3-location-utilities.md)).
The mapping fails, and the claim is a refusal with the reason given, when:

| Reason | Condition |
|---|---|
| `macro_body`, `pasted`, `crossing`, `not_a_file` | the tier (§2.2) |
| `not_owned` | the file is read-only (§2.1) |
| `file_recoded` | the front end's buffer for the file is not the file's bytes (it replaced invalid UTF-8, or removed a byte-order mark), so offsets cannot be trusted |
| `not_plain` | the bytes of the range are not the token's spelling: the name is written with a universal-character-name, a line splice or a trigraph |

When several conditions hold, the reason is the first in the order of the
table.

## 3. The transform request

Kind `TRFM`, version 1, deterministic. One required section `transform`
(id 1):

```
Transform := kind:      TransformKind
             ownership: Ownership           §2.1
             rename:    opt<Rename>         present iff kind = rename

TransformKind := rename = 1

Rename := target:   list<bytes>     §9.1: the qualified name, outermost component first; at least one
          kind:     TargetKind
          new_name: bytes           an identifier

TargetKind := any = 0 | function = 1 | variable = 2
```

Every component of `target` and `new_name` matches
`[A-Za-z_][A-Za-z0-9_]*`, is not a keyword of any C++ standard Kiln
implements, and `new_name` differs from the last component. (Identifiers
outside the basic character set: "Open questions".)

The **transform id** is the content id of the container. A map shard names
the transform it ran (§4), so shards of different transforms are never
reduced together.

## 4. Claims: the `edits` section

Section 17 of a shard of phase `map` (`model.md` §8). Present iff the
job carried a transform and the outcome is `ok`.

```
Edits := scheme:    uv               1
         transform: digest           the transform id
         files:     list<FileRef>    the files claims name; sorted by path, no two equal
         subjects:  list<Subject>    sorted by `id`, no two equal
         links:     list<Link>       sorted by (a, b), no two equal
         claims:    list<Claim>      sorted as below, no two equal in the sort key
```

Every entry of `files` is also in the shard's `inputs` with the same size
and content, or is the main file.

### 4.1 Subjects

An observer lists the entities the transform is about, as far as it can
see them.

```
Subject := id:      KeyId            H("slip.key.v1", key)
           key:     Key              digest.md §3: the entity's key in this observer, with no rules
           renamed: Name             the name the entity's key has once the transform is applied
           role:    Role
           of:      list<KeyId>      for a follower: the entities it follows; sorted, not empty. Else empty
           flags:   uv               SubjectFlags
           display: str              the entity as a person would write it; for reports only

Role := seed = 0          the request names it (§9.1)
      | candidate = 1     it must change if an entity it is linked to does (§9.2)
      | follower = 2      its key holds the old name because of the entities in `of` (§9.2)

SubjectFlags := bit 0  read_only      some declaration of the entity is in a read-only file
                bit 1  unsupported    the transform cannot change an entity of this kind (§9.5)
                bit 2  name_taken     the scope already declares the new name (§9.5)
                bit 3  unreachable    the entity is referenced where the transform cannot find the name (§9.5)
```

```
Link := a: KeyId, b: KeyId           a < b; both are subjects of this shard
```

A link says the two entities change together or not at all.

### 4.2 Claims

```
Claim := file:     uv                index into `files`
         start:    uv
         end:      uv                start <= end <= the file's size
         rule:     uv                which part of the transform made the claim (rename: 0)
         slot:     uv                distinguishes claims of one rule on one range (rename: 0)
         stance:   Stance
         text:     bytes             the replacement; empty unless stance = replace
         subjects: list<KeyId>       the entities the bytes denote here; sorted, no two equal
         reason:   Reason            `none` unless stance = refuse
         group:    uv                the atomic group (rename: 0)
         flags:    uv                ClaimFlags
         holders:  list<KeyId>       units that hold the name by spelling; sorted; empty unless `dependent`
         tiers:    uv                bit t set: the claim was derived at tier t (t = 0, 1, 2, 3; bit 4: X)
         macro:    opt<MacroSite>    present iff a tier other than 0 is set
         votes:    uv                times this observer derived the claim; at least 1

Stance := replace = 0        these bytes become `text`
        | keep = 1           these bytes denote something the transform must not change
        | refuse = 2         these bytes would have to change and cannot
        | unresolved = 3     these bytes may denote a subject; this observer cannot tell

Reason := none = 0 | macro_body = 1 | pasted = 2 | crossing = 3 | not_a_file = 4 | not_owned = 5
        | file_recoded = 6 | not_plain = 7 | mixed_set = 8 | mixed_instances = 9

ClaimFlags := bit 0  dependent       the name is in a template pattern and is digested by spelling

MacroSite := name_start: uv          offset, in the claim's file, of the name token of the outermost
                                      macro invocation the bytes are an argument of or were produced by
             name:       bytes       that macro's name
```

The **site** of a claim is `(file path, start, end, rule, slot)`. Claims
are sorted by `(file, start, end, rule, slot, stance, text, subjects,
reason)`, compared field by field (`text` and each id by bytes, `subjects`
as a sequence).

- `replace`, `keep` and `unresolved` claims have tiers 0 and 1 only, and
  their range is the token's file range (§2.3).
- A `refuse` claim's range is where a person must look: for `macro_body`
  the token's bytes in the macro definition; for every other reason the
  name token of the outermost invocation, or `[0, 0)` of the main file if
  there is none. `subjects` is not empty.
- `keep` is claimed for every name token whose bytes equal a `replace`
  the transform would write elsewhere and that this observer resolved to
  an entity that is not a subject (rename: every name spelled like the
  old name that denotes something else). `subjects` is then empty.
- `subjects` of a `replace` are subjects of the shard, of any role.

### 4.3 Votes

One range of file bytes can be seen by the parser several times in one
observer: an argument substituted twice, a header included twice. Each
time is one derivation. Derivations that agree in every field of the sort
key are one claim, with `votes` their number, `tiers` the union and
`macro` the least `MacroSite` (by `name_start`, then `name`). Derivations
that disagree are several claims at one site, and reduce sees the
disagreement.

### 4.4 Invariants

| Rule | Offset | JSON path |
|---|---|---|
| `scheme` is 1 | the field | `sections.edits.scheme` |
| `files` strictly ascending by path | that `FileRef`'s first byte | `sections.edits.files[i]` |
| `subjects` strictly ascending by `id` | that subject's first byte | `sections.edits.subjects[i]` |
| `id` is the key's tagged digest | the `id` field | `sections.edits.subjects[i].id` |
| `of` not empty iff role is `follower`; strictly ascending | the `of` field | `sections.edits.subjects[i].of` |
| flag bits above 3 are 0 | the `flags` field | `sections.edits.subjects[i].flags` |
| `links` strictly ascending; `a < b` | that link's first byte | `sections.edits.links[i]` |
| `file` is an index of `files`; `start <= end <= size` | the claim's first byte | `sections.edits.claims[i]` |
| `text` empty unless `replace`; `reason` is `none` iff not `refuse` | the `text` field | `sections.edits.claims[i].text` |
| `holders` not empty iff `dependent` | the `holders` field | `sections.edits.claims[i].holders` |
| `tiers` not 0, bits above 4 are 0; `macro` present iff `tiers` is not 1 | the `tiers` field | `sections.edits.claims[i].tiers` |
| `votes` at least 1 | the `votes` field | `sections.edits.claims[i].votes` |
| `claims` strictly ascending in the sort key | that claim's first byte | `sections.edits.claims[i]` |

Checked in this order, the claim rules for each claim in turn. That a
subject or a link names a key of the shard, and that `files` agrees with
`inputs`, are checked by reduce (§5.1), which reports them as `Input`
errors naming the shard.

## 5. Reduce

Reduce is a function

```
reduce(plan, round, the map shards of that round for the selected observers)
    -> (EditSet, opt<Declared>)
```

It reads shards and nothing else: no source file, no front end. The
selected observers are in plan order, and **the index** of an observer is
its position in the plan.

### 5.1 Inputs

- A selected observer with no shard, or whose shard is not `ok`, is
  **unparsed**. Reduce counts it and goes on; it contributes nothing.
- Every `ok` shard must have an `edits` section with `scheme` 1 and the
  same `transform`. Otherwise reduce fails with an `Input` error naming
  the shard. A shard that breaks a rule of §4.4's last paragraph fails the
  same way.
- If no shard is `ok` the edit set is refused with `no_observer`.

### 5.2 Subjects

1. **Union.** The subjects of all shards are joined by `id`. Two records
   of one `id` must have equal `key` and `renamed` (anything else is an
   `Input` error: a corrupt shard). The joined record is a `seed` if any
   record is; else a `candidate` if any is; else a `follower`. `flags` are
   or-ed; `of` is the union; `display` is that of the least observer
   index. Links are joined as a set.
2. **Selection.** The **selected** subjects are the least set that holds
   every seed and, with one end of a link, the other. A follower is
   selected if every entity in its `of` is selected.
3. **Refusals of subjects**, each one refusal of the edit set (§6.1):

| Cause | Condition |
|---|---|
| `no_subject` | there is no seed |
| `subject_read_only` | a selected subject or follower has `read_only` |
| `subject_unsupported` | a selected subject or follower has `unsupported` |
| `subject_unreachable` | a selected subject or follower has `unreachable` |
| `name_taken` | a selected subject or follower has `name_taken` |
| `mixed_follower` | a follower has some of its `of` selected and some not |

A candidate that is not selected is not a subject. A `replace` claim all
of whose `subjects` are unselected is treated as `keep`; a `refuse` claim
all of whose `subjects` are unselected is dropped. A `replace` or `refuse`
claim with some subjects selected and some not is the refusal
`mixed_set` at its site.

These three rules are applied to collapsed claims (§5.3), whose
`subjects` is the union over the observers that made the claim: selection
needs every shard's subjects and links, and the collapse does not. A
claim so turned into a `keep` loses its `text` and is joined with the
site's `keep` claim if there is one.

### 5.3 Collapse

Claims are joined by site, then within a site by `(stance, text)`. The
join of claims with one site, stance and text is one **collapsed claim**:

| Field | Join |
|---|---|
| `subjects`, `holders` | union |
| `tiers` | bitwise or |
| `flags` | bitwise or |
| `macro` | least, as in §4.3; absent if none has one |
| `reason` | least number other than `none` |
| `group` | must be equal, else the site is `divergent` |
| `observers` | the number of observers that made the claim |
| `votes` | the sum |
| `first` | the least 8 observer indices |

"Identical claims" are claims equal in site, stance and text. Ten thousand
observers that claim the same replacement of the same bytes are one
collapsed claim with `observers` 10000 and eight sample observers. Two
observers that resolve one name token to two different selected subjects
agree if they write the same text.

The join is associative and commutative, so the collapsed claims do not
depend on the order or the grouping in which shards are read.

### 5.4 Outcome of a site

For each site, from its collapsed claims after §5.2's treatment of
unselected subjects:

| The site has | Outcome |
|---|---|
| `replace` claims with one text, and nothing else | **live**: a candidate edit |
| `replace` claims with one text, and `unresolved` | **live** |
| `replace` claims with two or more texts | rejected: `divergent` |
| `replace` and `keep` | rejected: `contested` |
| `refuse` (whatever else) | rejected: `site_refused` with the reason |
| `keep` only, or `keep` and `unresolved` | **kept**: listed, not an edit |
| `unresolved` only | **unresolved**: residue of category `dependent` (§8.4) |

A live site whose claims were made at tier 1 in some observers and at
tier 0 in others is live; both bits are in the edit.

**Geometry.** Two live sites of one file **overlap** if they share a byte;
or both are empty ranges at one offset and differ in `(rule, slot)` only;
or one is an empty range strictly inside the other. A site is rejected
with `overlap` if it overlaps another live site. An empty range at either
end of a non-empty one does not overlap it. (A native rename produces no
empty range and no overlap; the rule is the general one, and nested edits
are an M3 question.)

**File agreement.** For every file named by a live site, every `ok` shard
that has the path in `inputs` or in `files` must give it the same size and
content digest. Otherwise every live site of the file is rejected with
`file_differs`: the file changed while the phase ran, or two observers
read different files through one real path.

### 5.5 Groups

A group is **rejected** if any site of the group is rejected, or any
refusal of §5.2 applies to a subject one of its claims names. All
rejections are computed first, over the live sites of every group; then
groups are rejected. A site that was rejected for overlapping a site of a
group that is itself rejected stays rejected: there is one pass.

The edits of the edit set are the live sites of the groups that are not
rejected. If every group is rejected, or there is no live site, the edit
set's `state` is `refused` and it has no edits.

### 5.6 Cost

Reduce must run in memory that does not grow with the number of observers
or of claims:

- Shards are read in runs of at most `fan_in` (default 64) in plan order.
  Each run is merged by site into a temporary file of collapsed claims
  under `tmp/` of the store; temporary files are merged `fan_in` at a time
  until one remains. Collapsed claims have bounded size (`first` holds at
  most 8 indices; `subjects` and `holders` are bounded by the distinct
  entities at one token), so a site claimed by every observer costs one
  record at every level.
- At any time reduce holds at most `fan_in` shards' `edits` sections, the
  joined subjects and links, and one buffer per open temporary file.
- File agreement reads each shard's `inputs` once, in a second pass over
  the shards, keeping only the paths of live sites.
- The first level may run on several threads (one run of shards each).
  By §5.3 the result is the same.

Work is `O(C log C)` in the total number of claims `C` in the worst case
and `O(C)` when the collapsed claims fit in one run. Reduce registers the
counters of §6.3.

### 5.7 Interface

`slip_reduce`:

```rust
pub struct Options { pub fan_in: usize, pub threads: usize, pub scratch: PathBuf }

pub trait Shards {
    /// The shard of the observer at `index` of the plan, or `None` (unparsed).
    fn open(&self, index: usize) -> Result<Option<Shard>, String>;
    fn len(&self) -> usize;
    fn observer(&self, index: usize) -> Digest;
}

pub struct Reduced { pub edits: EditSet, pub declared: Option<Declared> }

pub fn reduce(plan: Digest, round: u64, shards: &dyn Shards, options: &Options)
    -> Result<Reduced, ReduceError>;

pub enum ReduceError { Input { observer: Digest, message: String }, Io(String) }

/// §6.2, from an edit set alone: `None` for a refused one.
pub fn declared(set: &EditSet) -> Result<Option<Declared>, String>;
```

`slip_reduce` depends on `slip_model` only. `Shards` is implemented over
the store by `slip_cli` and over a vector by tests.

## 6. The edit set

Kind `EDTS`, version 1, deterministic. One required section `set` (id 1).
The **edit set id** is the container's content id.

```
EditSet := scheme:     uv                    1
           plan:       digest
           round:      uv
           transform:  digest
           basis:      digest                H("slip.basis.v1", list<Basis>): the shards reduced
           state:      SetState
           files:      list<FileRef>         sorted by path; the files `edits`, `kept`, `refusals` and `unresolved` name
           subjects:   list<Subject>         the selected subjects and followers, joined (§5.2); sorted by id
           edits:      list<Edit>            sorted by (file, start, end, rule, slot); empty unless state = accepted
           kept:       list<Range>           sorted by (file, start, end), no two equal
           unresolved: list<Range>           the same
           refusals:   list<Refusal>         sorted as below
           unparsed:   list<digest>          observers without an `ok` shard; in plan order
           counters:   list<Counter>         §6.3; sorted by name

Basis    := observer: digest, shard: opt<digest>      in plan order; `shard` the content id, absent if unparsed
SetState := accepted = 0 | refused = 1

Edit := file: uv, start: uv, end: uv, rule: uv, slot: uv, group: uv
        text:      bytes
        subjects:  list<KeyId>
        flags:     uv                ClaimFlags, or-ed
        holders:   list<KeyId>
        tiers:     uv
        macro:     opt<MacroSite>
        observers: uv
        votes:     uv
        first:     list<digest>      the observers of the least 8 indices that made the claim, in plan order

Range := file: uv, start: uv, end: uv

Refusal := cause:     Cause
           file:      opt<uv>         absent for a refusal of a subject
           start:     uv              0 when `file` is absent
           end:       uv
           rule:      uv
           slot:      uv
           reason:    Reason          for `site_refused`; else `none`
           subject:   opt<KeyId>      the subject, for a refusal of a subject; else the least subject of the site, if any
           macro:     opt<MacroSite>
           observers: uv
           first:     list<digest>

Cause := no_observer = 0 | no_subject = 1 | subject_read_only = 2 | subject_unsupported = 3
       | subject_unreachable = 4 | name_taken = 5 | mixed_follower = 6 | mixed_set = 7
       | site_refused = 8 | contested = 9 | divergent = 10 | overlap = 11 | file_differs = 12
```

Refusals are sorted by `(cause, file, start, end, rule, slot, subject)`,
an absent `file` or `subject` before any present one; no two are equal in
that key. For a refusal of a subject, `observers` and `first` count the
observers whose shard set the flag (for `no_subject` and `no_observer`:
0 and empty). For `contested`, they count the observers that claimed
`keep`; for the others, the observers of the rejected claims.

A refused edit set keeps its `kept`, `unresolved` and `refusals` lists:
they are the report.

### 6.1 Invariants

| Rule | Offset | JSON path |
|---|---|---|
| `scheme` is 1 | the field | `sections.set.scheme` |
| `basis` is the tagged digest of the list implied by `unparsed` and the reader's shards | not checked by the format | — |
| `files` strictly ascending by path | that `FileRef` | `sections.set.files[i]` |
| `edits` empty if `state` is `refused` | the first edit | `sections.set.edits[0]` |
| `edits` strictly ascending; no two of one file overlap (§5.4) | that edit | `sections.set.edits[i]` |
| every range within its file's size | that record | its path |
| `first` has at most 8 entries and at most `observers` | the field | its path |
| `refusals` strictly ascending | that refusal | `sections.set.refusals[i]` |
| `state` is `refused` if there is a refusal of a subject, `no_observer`, or no edit | the `state` field | `sections.set.state` |

### 6.2 Declared changes of an edit set

Reduce derives `Declared` (`verify.md` §2) from an accepted edit set. A
refused one has none.

- **Rules.** For every selected subject or follower `s` whose `renamed`
  differs from `s.key.name`:

  ```
  Rule { parent: s.key.parent, kind: s.key.kind, name: s.renamed, disc: s.key.disc,
         to_parent: s.key.parent, to_name: s.key.name }
  ```

  This is the rule of `verify.md` §2.1 for a relocation that changes the
  name only. `parent` and `disc` are in mapped terms; they are the
  pre-image's, because the transform (§9) changes no entity that is the
  parent of a subject or occurs in a subject's discriminator. A transform
  that does must give the mapped forms itself (M3).
- **`changed`**: the union of `holders` over the edits.
- **`created`, `removed`**: empty.

Two rules with one `(parent, kind, name, disc)` cannot arise from distinct
keys. Reduce checks it and fails with an `Input` error if it does.

That `Declared` is derived by untrusted code is the design: a rule that is
wrong or missing cannot make two different programs compare equal
(`verify.md` §2.1). An exemption can: `changed` withdraws a unit from the
comparison. §9.4 says which units a rename exempts and why, the report
prints them, and "Open questions" proposes a replacement that exempts
nothing.

### 6.3 Counters

In `EditSet.counters`:

| Counter | Meaning |
|---|---|
| `reduce.shards` | `ok` shards read |
| `reduce.unparsed` | selected observers without one |
| `reduce.claims` | claims read |
| `reduce.claims.replace`, `.keep`, `.refuse`, `.unresolved` | by stance |
| `reduce.sites` | distinct sites |
| `reduce.sites.live`, `.kept`, `.unresolved`, `.rejected` | by outcome (§5.4) |
| `reduce.subjects`, `reduce.subjects.selected` | joined subjects; selected ones, followers included |
| `reduce.edits` | edits in the set |
| `reduce.edits.shared` | edits claimed by more than one observer |
| `reduce.edits.max_observers` | the largest `observers` of an edit |
| `reduce.files` | files with at least one edit |
| `reduce.runs` | temporary files written (§5.6) |

Map shards that carry a transform add to their `counters`:
`edits.claims`, `edits.claims.replace`, `edits.claims.keep`,
`edits.claims.refuse`, `edits.claims.unresolved`, `edits.subjects`,
`edits.links`, `edits.files`.

## 7. Apply

### 7.1 The overlay

`slip_apply` turns an accepted edit set into an **overlay**: for each file
with an edit, the post-image bytes.

1. Read the file at its real path. If it cannot be read, or its size or
   content digest is not the `FileRef`'s, apply fails with `stale_file`
   naming the path. Nothing is published.
2. The post-image is the file's bytes with each edit's range replaced by
   its text, edits taken in ascending order. Bytes outside the ranges are
   copied. No byte is added, removed or recoded: a byte-order mark, the
   line endings, a missing final newline and invalid UTF-8 all pass
   through.
3. The post-image is published as a **blob**: a file of exactly those
   bytes, named by their content digest (§10.4).

```
Overlay := edits: digest                     the edit set id
           files: list<OverlayFile>          sorted by path, no two equal; not empty

OverlayFile := path: path                    real path
               pre:  Content
               post: Content

Content := size: uv, content: digest
```

Kind `OVLY`, version 1, deterministic, one required section `overlay`
(id 1). The **overlay id** is the container's content id. Invariants:
`files` strictly ascending by path; `pre` differs from `post`.

An overlay replaces the content of existing files. It creates and removes
none (M3). A file whose edits leave its bytes unchanged is an error of the
edit set (`Input`), not an empty entry.

**Parsing through an overlay.** The front end is given the overlay's
files and blobs ([KF2](../docs/requests/KF2-overlay-and-file-identity.md)).
Every read of a file the overlay names yields the post-image bytes, by
whatever spelling the file is reached; every other read goes to the file
system. The post-image has the pre-image's paths, so a subject path
(`digest.md` §11) is the file's real path in both images.

### 7.2 The patch

`--patch <file>` writes the overlay as a unified diff, a function of the
overlay and the pre-image bytes:

- One file section per overlay file, in path order: `--- <path>` and
  `+++ <path>`, then hunks with three lines of context, hunks merged when
  their context touches or overlaps.
- `<path>` is the real path, or, under `--relative-to <dir>`, the path
  relative to that directory's real path (an overlay file outside it is a
  usage error). Paths are written as their bytes.
- A line ends at `\n`. A `\r` before it is part of the line. A last line
  without `\n` is followed by `\ No newline at end of file`. Bytes are
  copied as they are.

`patch -p0` applied to the pre-image must give the post-image byte for
byte, for every file whose lines `patch` can represent; the test of
"Tests" is that property.

### 7.3 Writing the tree

`--write` replaces the pre-image files by the post-image. It is permitted
only by a result whose state allows it (§8.5), and it is all or nothing.
The **staged name** of an overlay file is, in the file's own directory,
`.<file name>.slip-<the first 16 hexadecimal digits of the overlay id>`.

1. **Check.** For every overlay file: it is owned (§2.1), it is a regular
   file with one hard link, its directory is writable, no file has its
   staged name, and its bytes have the `pre` size and digest. Any failure
   refuses the write (`not_owned`, `hard_linked`, `not_writable`,
   `stale_file`) and nothing is touched.
2. **Journal.** The pre-image bytes of every file are published as blobs.
   Then `<store>/write.journal` is published:

   ```
   Journal     := overlay: digest, files: list<JournalFile>      in path order
   JournalFile := path: path, staged: bytes, pre: Content, post: Content
   ```

   one record in the encoding of `model.md` §2, with no container; it is
   not addressed by content and never leaves the store.
3. **Stage.** For every file, in path order: the post-image blob is
   copied to the staged name (created exclusively, with the original's
   permission bits) and synced.
4. **Check again** as in step 1, except that the staged files exist. A
   failure removes the staged files and the journal; nothing was
   replaced.
5. **Commit.** Each staged file is renamed over its original, in path
   order, and the directories are synced. Then the journal is removed.

From step 2 on, a failure that the command cannot undo, or an
interruption, leaves a journal, and the journal names every file the
write may have created. Every Slip command that opens the store
exclusively finds it and refuses to run (`Input`: the store has an
unfinished write). `slip apply --recover` finishes it: every file whose
bytes have the `pre` digest is left; every file with the `post` digest is
restored from its pre-image blob (written to the staged name and renamed
over the file); any other content is reported and left; staged files are
removed; the journal is removed if no file was reported. So a file is at
every moment wholly the pre-image or wholly the post-image, and an
interrupted write returns to the pre-image.

A symbolic link that names an overlay file is not touched: the file at
the real path is replaced. A file is never written outside the owned set.

### 7.4 Interface

`slip_apply` (depends on `slip_model` only):

```rust
pub trait Files {                                   // the pre-image; the file system, or a map in tests
    fn read(&self, path: &[u8]) -> std::io::Result<Vec<u8>>;
}
pub struct Built { pub overlay: Overlay, pub blobs: Vec<(Digest, Vec<u8>)> }   // post-image bytes, in overlay order
pub enum ApplyError { StaleFile(Vec<u8>), Unchanged(Vec<u8>), NotAccepted, Invalid(String), Io(Vec<u8>, String) }

pub fn splice(pre: &[u8], edits: &[(u64, u64, &[u8])]) -> Result<Vec<u8>, ApplyError>;
pub fn build(set: &EditSet, set_id: Digest, files: &dyn Files) -> Result<Built, ApplyError>;
pub fn patch(overlay: &Overlay, pre: &dyn Files, post: &dyn Fn(Digest) -> std::io::Result<Vec<u8>>,
             relative_to: Option<&[u8]>, out: &mut dyn std::io::Write) -> Result<(), ApplyError>;
```

`build` holds one file's pre- and post-image at a time when given a sink
instead of collecting (`build_into`, same contract).

The write of §7.3 is over traits for the tree, the blobs and the journal,
so that a failure at every operation of steps 1 to 5 is tested:

```rust
pub struct Stat { pub regular: bool, pub links: u64, pub mode: u32, pub dir_writable: bool }
pub trait Tree {
    fn stat(&self, path: &[u8]) -> std::io::Result<Option<Stat>>;           // None: no such file
    fn read(&self, path: &[u8]) -> std::io::Result<Vec<u8>>;
    fn create(&self, path: &[u8], mode: u32, bytes: &[u8], exclusive: bool) -> std::io::Result<()>;   // written and synced
    fn rename(&self, from: &[u8], to: &[u8]) -> std::io::Result<()>;
    fn remove(&self, path: &[u8]) -> std::io::Result<()>;
    fn sync_dir(&self, dir: &[u8]) -> std::io::Result<()>;
}
pub trait Blobs {
    fn get(&self, id: Digest) -> std::io::Result<Vec<u8>>;
    fn put(&self, bytes: &[u8]) -> std::io::Result<Digest>;
}
pub trait JournalStore {
    fn publish(&self, bytes: &[u8]) -> std::io::Result<()>;
    fn read(&self) -> std::io::Result<Option<Vec<u8>>>;
    fn remove(&self) -> std::io::Result<()>;
}
pub enum Refused { NotOwned(Vec<u8>), HardLinked(Vec<u8>), NotWritable(Vec<u8>), StaleFile(Vec<u8>) }
pub enum WriteError { Refused(Refused), Io { path: Vec<u8>, message: String, journal_left: bool }, Journal(String) }
pub struct Recovered { pub restored: Vec<Vec<u8>>, pub left: Vec<Vec<u8>>, pub reported: Vec<Vec<u8>> }

pub fn write(overlay: &Overlay, overlay_id: Digest, ownership: &Ownership,
             tree: &dyn Tree, blobs: &dyn Blobs, journal: &dyn JournalStore) -> Result<u64, WriteError>;
pub fn recover(tree: &dyn Tree, blobs: &dyn Blobs, journal: &dyn JournalStore) -> Result<Option<Recovered>, WriteError>;
```

`write` returns the number of files replaced; `recover` returns `None`
when there is no journal.

## 8. The verify loop

One **run** of a transform over a plan is one **round** `r`: the map phase
writes `shards/<plan>/map.<r>/`, the verify phase `verify.<r>/`. `r` is the
least number for which the plan has no shard directory of either phase.
(A round is one application of edits, `verify.md` §3; a run that applies
edits more than once is M4.)

### 8.1 Steps

| Step | Does | Parallel |
|---|---|---|
| map | per observer: parse, digest (`digest.md`), run the transform; shard with `inputs`, `digests`, `edits` | yes |
| reduce | §5: the edit set and the declared changes | first level |
| apply | §7.1: the overlay | no |
| verify | per observer: parse through the overlay with the declared rules, compare (`verify.md` §5) | yes |
| scan | §8.4: residue | per file |
| conclude | §8.5: the result | no |

A refused edit set ends the run after reduce: there is no overlay, no
verify phase, and the result's state is `refused`.

### 8.2 Which observers are parsed again

Every selected observer whose map shard is `ok` gets a verify job when the
declared rules are not empty. The post-image of an observer that read no
overlay file is its pre-image, and its per-observer verdict is not in
doubt; the job is there for the agreement check (`verify.md` §9), which
needs to know, from trusted code, whether that observer has an entity a
rule renames. With no rules, an observer that read no overlay file is
`unchanged` and gets no job (`verify.md` §5).

An observer **read** an overlay file if the file's path is in the
`inputs` of its map shard.

("Open questions": a trusted filter of keys in the map shard would let
most observers that read no overlay file skip the job.)

### 8.3 The verify job

As `verify.md` §5, with the post-image given as an overlay id and the
declared changes as a `DECL` id, both in the store. In addition:

- The post shard's `inputs` are checked against the pre shard's
  (`verify.md` §5, "Inputs under an overlay").
- The post shard carries the `binding` and `rule_use` sections
  (`verify.md` §9).

### 8.4 Residue

Verification covers what the planned observers compile. The **residue** is
the text that spells the old name and that no observer accounted for. It
is found by a scan that is not part of the trusted base and changes no
verdict; it says where verification did not look.

The scan reads, raw-lexed with Kiln's lexer (no preprocessing):

- every owned file that is in the `inputs` of some `ok` map shard, and
- every regular file under an owned directory entry whose name ends in one
  of `.h .hh .hpp .hxx .h++ .c .cc .cpp .cxx .c++ .ipp .tcc .inc .def`
  (`--residue-ext` replaces the list; directories are walked in byte
  order of names, symbolic links are not followed).

For a rename, each identifier token whose spelling is the old name is one
of:

| The token | Is |
|---|---|
| covered by an edit | edited |
| covered by a `kept` range | not residue: some observer resolved it to something else |
| covered by an `unresolved` range, or by a refusal | residue `dependent`, or already reported |
| in a file some observer read, covered by nothing | residue `not_compiled`: an excluded conditional group, an argument never substituted, a macro definition |
| in a file no observer read | residue `not_planned` |

A comment or a string or character literal that contains the old name as
a whole word (not preceded or followed by an identifier character) is a
**mention**. Mentions are listed and never change the state.

```
Residue := category: Category, file: path, start: uv, end: uv, line: uv, column: uv
Category := dependent = 0 | not_compiled = 1 | not_planned = 2 | mention = 3
```

sorted by `(file, start)`; line and column are 1-based, the column in
bytes.

### 8.5 The result

Kind `RSLT`, version 1, deterministic. One required section `result`
(id 1). The **result id** is the content id.

```
Result := scheme:    uv               1
          plan:      digest
          round:     uv
          transform: digest
          edits:     digest           the edit set id
          declared:  opt<digest>      content id of the DECL container
          overlay:   opt<digest>
          state:     State
          partial:   bool             the run selected fewer observers than the plan has
          tally:     Tally
          agreement: list<Stale>      verify.md §9; sorted by (rule, observer)
          unused:    list<uv>         indices of rules no observer reached an entity through
          residue:   list<Residue>
          exempt:    list<KeyId>      Declared.changed, repeated here for the report

Tally := selected: uv, unparsed: uv, verified: uv, unchanged: uv, mismatch: uv,
         not_parsed: uv, unverifiable: uv, failed: uv

Stale := rule: uv, observer: digest, direct: uv

State := verified = 0 | verified_with_residue = 1 | not_verified = 2 | incomplete = 3 | refused = 4
```

Invariants: `agreement` strictly ascending by `(rule, observer)`;
`unused` and `exempt` strictly ascending; `residue` sorted by
`(file, start)`; `overlay` and `declared` are absent iff `state` is
`refused`, and a `refused` result has a tally of 0 in every field but
`selected` and `unparsed`.

The state is the first that applies:

1. `refused`: the edit set is refused.
2. `not_verified`: some observer is `mismatch` or `not_parsed`, or
   `agreement` is not empty.
3. `incomplete`: some observer is `unverifiable` or `failed`, a verify
   shard is missing or not bound to this overlay and these declared
   changes (`verify.md` §9), the run was cancelled, or `unparsed` is not 0
   and the run was not given `--allow-unparsed`.
4. `verified_with_residue`: the residue has an entry that is not a
   `mention`.
5. `verified`.

`--write` and `--patch` act only on `verified`; on
`verified_with_residue` under `--allow-residue`; and never on a `partial`
result unless `--allow-partial` is given. Under `--allow-unparsed` the
main file of every unparsed observer is scanned as a file no observer
read.

A state other than `verified` withdraws nothing from the store: the
overlay and the shards stay, for the report and for diagnosis. **Repair**
(changing the edit set in response to findings and verifying again) is
M4; its hook is that a result names everything a second round needs.

## 9. Native rename

`slip::refactor::rename` at Level A
([01](../docs/design/01-transform-api.md) §6): implemented in Rust in
`slip_refactor`, configured by `Rename` (§3), run in the map phase after
the digest walk. It renames **functions and variables**.

### 9.1 The target

`target` is a qualified name from the global namespace: `a::b::f` is
`[a, b, f]`. An entity **matches** if its key (`digest.md` §3) has the
name `identifier(f)`, its parent chain to `root` is, innermost first,
entities named `b` then `a` of kind `namespace`, `class` or
`class_template`, and its kind is allowed:

| `TargetKind` | Key kinds |
|---|---|
| `function` | `function`, `function_template` |
| `variable` | `variable`, `variable_template` |
| `any` | all four |

Every match is a **seed**: all overloads of the name in that scope are
renamed together. Members of unnamed namespaces and of inline namespaces
match only when the request names the namespace, which for an unnamed one
it cannot ("Open questions").

An entity that has the target's name and scope and another key kind (a
class, a field, an enumerator, an alias, a namespace, a concept) is a seed
with the flag `unsupported`.

### 9.2 Subjects

- **Seeds**: §9.1.
- **Candidates**: every virtual member function whose name is the old
  identifier, in every observer, whatever class it is in. With each, a
  **link** to every function it overrides. Overriding ties functions
  across classes that no single observer need see together; reduce closes
  the seeds under the links (§5.2), so a rename of one virtual function
  renames its whole family and no other.
- **Followers**, each with `of` the subjects it follows and `renamed` its
  key's name with the old identifier replaced by the new:
  - a using-declaration that names subjects, and each using-shadow it
    introduces (`of`: the entities named);
  - a member of a class template specialisation instantiated from a
    subject, when its key has the old name (`of`: the member of the
    template);
  - an unnamed class or enumeration that the subject names (`digest.md`
    §3.3, `unnamed_by`).

  Entities whose keys hold a subject's key as parent or in a
  discriminator (a function template's specialisations, local types,
  default arguments) need no record: they map with the subject
  (`digest.md` §3.4).

For every subject of any role `renamed` is `identifier(new_name)` unless
stated otherwise above.

### 9.3 Written references

For each subject of the observer, rename claims `replace` with the new
name at every token that is the subject's name:

| Reference | Notes |
|---|---|
| Every declaration and the definition of the entity: its declarator-id's last name | Redeclarations in other files, friend declarations, block-scope `extern` declarations, out-of-line definitions (`void N::f() {}`: `f` only), a template's declarations, explicit specialisations |
| An id-expression: a call, `&f`, `&C::f`, a use of a variable; in evaluated and unevaluated operands, default arguments, default member initialisers, template arguments | The function overload resolution selected is the subject. A name that selects a non-subject overload of another scope is a `keep` |
| A class member access `a.f`, `p->f`, `a.C::f` | |
| A qualified name: only the last component is the name | Qualifiers are not edited: a rename moves nothing |
| A using-declaration's name | The using-declaration is a follower (§9.2) |
| An overriding function | It is a subject by a link (§9.2) |
| A non-dependent name in a template pattern | Resolved at the definition |
| A dependent name in a template pattern | §9.4 |
| An explicit instantiation or specialisation that names a subject template | |
| A name in a macro argument (tier 1) | Each substitution votes |
| A name in a macro body, pasted, or crossing (tiers 2, 3, X) | `refuse` |

A name token spelled like the old name that denotes an entity that is not
a subject of the observer is claimed `keep` (§4.2).

Not references, never claimed: comments; string and character literals;
the operand of `#`; identifiers in preprocessor directives; text in a
conditional group the observer skips; a macro argument that is never
substituted. Verification does not cover them (`verify.md` §8, "Text");
§8.4 lists them.

A reference the language makes without a token (the `begin` of a
range-based `for`, a coroutine's `await_ready`, `get` for a structured
binding) cannot be edited. Rename claims nothing for it; the post-image
then fails to parse or resolves differently, and the run is not verified.

### 9.4 Dependent names

A dependent name in a pattern has no entity until instantiation, and its
identifier is part of the pattern's digest (`digest.md` §8).

| The name is | The front end gives | Claim |
|---|---|---|
| the unqualified callee of a dependent call | the declarations `S` found at the definition | `S` not empty and all subjects: `replace`, `dependent`. `S` not empty and none a subject: `keep`. Mixed: `refuse(mixed_set)`. `S` empty: next row |
| any other dependent name (`T::f`, `x.f`, `x->f`, a call found only by argument-dependent lookup) | per instantiated specialisation, the entity the name resolved to | every one a subject: `replace`, `dependent`. None a subject: `keep`. Mixed: `refuse(mixed_instances)`. Not instantiated in this observer, or the front end has no such record: `unresolved` |

`holders` of a `dependent` claim is the key of the unit whose stream holds
the name: the innermost enclosing declared unit of the pattern. Reduce
puts it in `Declared.changed` (§6.2).

This is where rename guesses, and where it leans on verification most:

- A `replace` from the first row can be wrong for one instantiation: the
  call there may have selected, by argument-dependent lookup, a function
  that is not a subject. The post-image then selects something else in
  that specialisation. Its derived unit differs (`derived_changed`), and
  the run is not verified.
- An `unresolved` name that should have changed leaves an instantiation
  that does not parse, or one that resolves differently, in every
  observer that instantiates it. If no observer instantiates it, nothing
  detects it; it is residue (`dependent`).
- The exemption of a holder withdraws the pattern's own unit from the
  comparison. What is still compared is every instantiation any observer
  makes, and every other unit. An error confined to an exempt pattern
  that no observer instantiates is not detected. The report lists every
  exempt unit.

### 9.5 Refusals

Rename sets a subject's flags (§4.1), which refuse the run (§5.2):

| Flag | When |
|---|---|
| `read_only` | a declaration of the entity is in a read-only file. A function that overrides one of a library is refused this way, through its link |
| `unsupported` | another kind (§9.1); a constructor, destructor, operator, conversion function, literal operator or deduction guide reached through a link; an entity with C language linkage; `main`; a member of a class template, until the package that adds followers of specialisations lands |
| `name_taken` | the scope that declares the entity (for a namespace, every part of it the observer sees; for a class, the class and its bases) declares an entity named `new_name` |
| `unreachable` | the subject is referenced where the front end records no name token: an explicit instantiation, an attribute argument, a designator |

and claims `refuse` (§4.2) at a reference it found and cannot edit (§2.3).

Not refused, and left to verification: a use that would resolve to
another entity once renamed (the new name hides, or is hidden by, a
declaration in another scope); a reference without a token (§9.3); a
string that names the entity for the linker (`alias`, `ifunc`: "Open
questions").

### 9.6 What a rename declares

By §6.2: one rule per selected subject and follower whose key holds the
old name, and `changed` for the holders of dependent edits. Nothing is
`created` or `removed`.

The claim verified is that of `verify.md` §1: in every selected observer
the post-image is the same program as the pre-image except that the
entities the rules name have the new name, and except for the exempt
units; and (`verify.md` §9) no observer still has one of those entities
under its old name.

### 9.7 Interface

`slip_refactor` (depends on `slip_model`, `slip_kiln`, `slip_digest`; it
may name `slip_kiln::kiln`, `spec/harness.md` §3):

```rust
pub fn run(transform: &Transform, unit: &slip_kiln::Unit<'_>, keys: &slip_digest::Keys<'_>,
           files: &dyn Fn(kiln::source::FileId) -> Option<FileRef>) -> Result<Edits, String>;
```

`slip_digest::Keys` gives a declaration's key, with no rules, and the
unit that holds it (`verify.md` §9 lists what the walk exposes). `run` is
a function of the unit: two parses of the same inputs give equal `Edits`.

## 10. Commands

Added to `spec/exec.md` §8:

```
slip rename --from <qualified name> --to <identifier> [--kind function|variable]
            (--plan <id or file> | --db [<name>=]<compile_commands.json>…)
            --owned <path>… [--readonly <path>…]
            [--write | --patch <file>] [--relative-to <dir>]
            [--allow-residue] [--allow-unparsed] [--allow-partial] [--residue-ext <list>]
            [--store <dir>] [-j <n>] [--only <substring>]… [--limit <n>]
            [the memory, timeout and worker options of `parse`]
slip reduce --plan <id or file> --round <n> [--store <dir>] [-j <n>]
slip check  --plan <id or file> --round <n> --edits <id or file> [--declared <id or file>]
            [--allow-unparsed] [--residue-ext <list>] [--store <dir>] [-j <n>] [options of `parse`]
slip apply  --result <id> (--write | --patch <file>) [--relative-to <dir>]
            [--allow-residue] [--allow-partial] [--store <dir>]
slip apply  --recover [--store <dir>]
```

An `<id>` is a prefix of 4 to 64 hexadecimal digits that matches exactly
one file of its kind in the store, as for `--plan` (`exec.md` §8.2).

### 10.1 `rename`

1. Opens the store exclusively; builds or finds the plan as `parse` does.
2. Builds the transform request (§3): `--from` split at `::` (a leading
   `::` is allowed and ignored), `--owned` and `--readonly` made real
   paths. Publishes it.
3. Runs the steps of §8.1 for a new round. The map phase is the phase of
   `exec.md` §5 with jobs that carry the transform id; the verify phase
   with jobs that carry the overlay and declared ids.
4. Publishes the result and prints the report (§10.5).
5. With `--write` or `--patch`, and a state that permits it (§8.5), acts
   as `apply` does.

`rename` without `--write` or `--patch` changes nothing outside the store.

### 10.2 `reduce`, `check`, `apply`

- `reduce` runs §5 over the map shards of a round and publishes the edit
  set and the declared changes. It prints `edits <id>`, `state <state>`,
  and `declared <id>` if there is one.
- `check` takes an edit set, which need not be the one reduce made, builds
  the overlay, runs the verify phase, scans and concludes. `--declared`
  defaults to the declared changes of the edit set (§6.2), derived again.
  It is `rename` from the apply step on, and it is how a test gives the
  trusted base an edit set that is wrong on purpose (`harness.md` §6.4).
  The edit set's `plan`, `round` and `transform` must be those of the
  round's map shards.
- `apply --result` performs §7.2 or §7.3 for a published result.
- `apply --recover` is §7.3's recovery. It is the only command that runs
  on a store with a journal.

`rename` is `plan` (with `--db`), then the map phase, then `reduce`, then
`check`, then `apply`. `slip parse` and `slip verify` are not part of it:
a map shard made by `parse` has no `edits` section, and `verify`
(`verify.md`) checks a post-image that someone else made.

### 10.3 Protocol

`exec.md` §5.2: the `Job` message gains, after `observer`:

```
transform: opt<digest>      phase map: the transform to run
post:      opt<Post>        phase verify: the post-image
Post := overlay: digest, declared: digest
```

and `Ready.protocol` is 2. A worker reads the transform, the overlay, its
blobs and the declared changes from the store, which it opens shared.

An overlay can name far more files than one observer reads, so a verify
job first loads the blobs of the overlay files that are in the pre
shard's `inputs`. If the post-image then read an overlay file whose blob
was not loaded (an edit added an `#include`), the job loads those too
and parses again, until no such file is left. The shard is that of the
last parse; each parse reads strictly more of the overlay than the one
before, so the number of parses is bounded, and the shard does not
depend on it.

### 10.4 The store

Added to `exec.md` §2:

```
<store>/
  transforms/<transform id>.trfm
  edits/<edit set id>.edts
  declared/<id>.decl                 DECL containers, by content id
  overlays/<overlay id>.ovly
  results/<result id>.rslt
  blobs/<content digest>             file contents: the bytes, no container
  runs/<plan id>/transform.<round>.<started_unix_ns>.run       RUNT (§10.6)
  write.journal                      §7.3; present only during a write
```

A blob is published like every file (`exec.md` §2). A reader checks a
blob's digest when it reads it; a blob that does not hash to its name is
an `Input` error.

### 10.5 Output and exit status

On standard output, after the plan lines of `parse` (`exec.md` §8.2) when
the plan was built:

```
transform <id>
round <n>
map: selected <n>, ok <n>, not ok <n>
edits <id>
state <accepted|refused>
edit <n> in <n> file(s); shared <n>; most observers <n>
kept <n>; unresolved <n>
refused <cause> <n>                       one line per Cause with a non-zero count, in its order
declared <id>: rules <n>, exempt <n>
overlay <id>
verify: verified <n>, unchanged <n>, mismatch <n>, not_parsed <n>, unverifiable <n>, failed <n>
agreement: stale <n>, unused rules <n>
residue: dependent <n>, not_compiled <n>, not_planned <n>; mentions <n>
result <id>
state <State>
written <n> file(s)                       or: patch <path>
```

Lines for steps that did not run are omitted. Then the details, each list
in the order of its record, paths relative to `--relative-to` when given:

```
refusal <cause>[ <reason>]: <path>:<line>:<column>: <display of the subject>[ in macro <name>] (<n> observer(s), first <main file>)
refusal <cause>: <display of the subject> (<n> observer(s), first <main file>)
finding <kind> <name>: <n> observer(s), first <main file>[: <path>:<line>:<column>]
stale <display>: still declared under its old name in <main file>
exempt <display>
residue <category>: <path>:<line>:<column>
```

`finding` lines are the run report of `verify.md` §7, at most 64 groups.
Of each other kind at most 200 lines are printed; a list that is cut is
followed by `<kind>: <n> more (slip dump <file of the record>)`. The
records hold every entry.
Lines and columns are computed from the pre-image file by the command;
they are not in the records.

Exit status (`exec.md` §8.5):

| Status | `rename`, `check` | `reduce` | `apply` |
|---|---|---|---|
| 0 | state `verified`, and what `--write` or `--patch` asked was done | `accepted` | done |
| 1 | any other state; or a state that does not permit the write that was asked | `refused` | the result does not permit it; a refused write (§7.3 step 1 or 4) |
| 2 | usage | usage | usage |
| 3 | input error: as `parse`; a shard without the transform; a stale file; a store with a journal | the same | the same; `--recover` with a file it had to report |

A `verified_with_residue` result written under `--allow-residue` exits
with status 1: the command did what was asked and has something to
report.

### 10.6 The transform run record

Kind `RUNT`, version 1, not deterministic. One per `rename` or `check`
invocation, published when the command ends, also when it is interrupted
after the map phase began. One required section `run` (id 1):

```
TransformRun := tool_slip: str, tool_kiln: str
                plan: digest, round: uv, transform: digest
                edits: opt<digest>, declared: opt<digest>, overlay: opt<digest>, result: opt<digest>
                started_unix_ns: uv
                wall_ns: uv                    the whole command
                steps: list<Step>              in the order run

Step := name: StepName
        wall_ns: uv
        cpu_ns: uv                             of the coordinator process during the step; 0 if unknown
        peak_rss: uv                           of the coordinator process at the end of the step, bytes; 0 if unknown
        run: opt<digest>                       map, verify: content id of the phase's run record (model.md §9)

StepName := plan = 0 | map = 1 | reduce = 2 | apply = 3 | verify = 4 | scan = 5 | conclude = 6 | write = 7
```

The phases' own run records (`model.md` §9) are written as before, with
their `phase` and `round`. The harness reads wall time and memory of a
transform from this record and from the two it names
(`harness.md` §6.5).

## 11. What M2 leaves out

| Not in M2 | Where |
|---|---|
| Transforms written in C++, facts and guards beyond links, several transforms in one run | M3 (`spec/api.md`) |
| Created and removed files; inserted declarations; `created` and `removed` | M3 |
| Replacement text built from the pre-image (copies, moved ranges), nested edits | M3 |
| Edits in macro bodies; accepting a changed stringised argument; coverage by byte range | M4 (`spec/macros.md`, KR4) |
| Requalifying a name that would resolve differently; spelling candidates | M4 |
| Repair rounds | M4 |
| Rename of types, fields, enumerators, namespaces, concepts, parameters and locals | after Kiln's written-reference stages 3 to 5 |

## Tests

- `crates/slip_model/tests/edits.rs` — §3, §4: round trips of `TRFM` and
  of the `edits` section, binary and JSON; every invariant of §4.4 with
  its offset and path.
- `crates/slip_model/tests/editset.rs` — §6, §7.1, §8.5, §10.6: round
  trips and invariants of `EDTS`, `OVLY`, `RSLT`, `RUNT`.
- `crates/slip_reduce/tests/reduce.rs` — §5, §6.2 over shards written by
  hand: selection through links (a chain, a diamond, an unselected
  candidate); each refusal of §5.2; every row of §5.4; overlap geometry
  including empty ranges; file disagreement in `files` and in `inputs`;
  groups; unparsed observers; the declared changes of §6.2.
- `crates/slip_reduce/tests/order.rs` — §1 rule 3, §5.3, §5.6: for
  generated claim sets (fixed seed), the edit set is byte-identical under
  every tested permutation of observer completion, every `fan_in` from 2
  to 64 and every thread count; one site claimed by ten thousand shards
  with `fan_in` 2 stays within a stated memory bound.
- `crates/slip_apply/tests/splice.rs` — §7.1: splicing at the start, the
  end, adjacent ranges; CRLF, a byte-order mark, no final newline, invalid
  UTF-8, an empty file; a stale file; an edit set that changes nothing.
- `crates/slip_apply/tests/patch.rs` — §7.2: the patch applied by
  `patch(1)` gives the post-image (skipped where there is no `patch`).
- `crates/slip_apply/tests/write.rs` — §7.3: a failure injected at every
  operation of steps 1 to 5 leaves every file wholly pre or wholly post
  and `recover` returns the tree to the pre-image; a hard link, a
  read-only directory, a file changed between the checks.
- `crates/slip_refactor/tests/rename/` — §9, one directory per case over
  a small tree of files (`slip_kiln::tree`; a snippet's buffer is not a
  file and has no range to claim): each row of §9.3 and §9.4, each flag of §9.5, the
  tiers of §2.2, votes (§4.3).
- `tests/transform/rename/` — §8, §9, §10 through the executable
  (`harness.md` §6): the cases above end to end with their post-images,
  reports and exit statuses; the oracles of `harness.md` §6.4.
- `crates/slip_cli/tests/stages.rs` — §10.2 over a store of shards
  written by hand: `reduce` (ids, lines, statuses), `apply --result` with
  a result of each state and each `--allow` option, `apply --recover`, a
  store with a journal.
- `crates/slip_cli/tests/rename.rs` — §10: usage, the output lines, exit
  statuses, `check` as a stage, `rename` end to end.
- `crates/slip_cli/tests/tasks.rs` — §8.1, §8.3, §10.3: the map job of a
  transform and the verify job through an overlay, as tasks.
- `crates/slip_kiln/tests/sites.rs` — §2.2, §2.3: each tier, each reason
  and the order of reasons; the range of each kind of refusal.
- `crates/slip_kiln/tests/scan.rs` — §8.4: identifiers, mentions, raw
  strings, line splices, text in skipped groups.
- `crates/slip_kiln/tests/overlay.rs` — §7.1 "Parsing through an
  overlay": the main file, a header by two spellings, an edit that adds
  an `#include`; `inputs` shows the post-image digests.
- `crates/slip_exec/tests/objects.rs` — §10.3, §10.4: the store's new
  files, blobs and their check, the next round, the journal's refusal,
  protocol 2.
- `crates/slip_verify/tests/conclude.rs` — §8.5: every row of the state,
  in order, and what each state permits.

## Open questions

- **Exempting a pattern** (§9.4, §6.2) weakens verification where rename
  edits a dependent name. Proposed instead: a declared **spelling rule**
  `(holder, from, to)`, under which the walk writes a dependent name `to`
  inside `holder` as `from`. The pattern's digest is then equal and
  nothing is exempt; the rule is sound for the reason every rule is (it is
  applied to the post-image only, and equality is still required), at the
  price of a false mismatch when the holder already used `to` as a
  dependent name. It changes `digest.md` §3.4 and §8 and `Declared`.
- **A stringised argument.** A renamed name inside an argument that the
  macro also stringises (`EXPECT_EQ(f(x), 1)`, `assert` with assertions
  on) changes a string literal, and the unit that holds it is `changed`.
  M2 reports the mismatch and does not verify. Accepting it soundly needs
  the literal to be digested as "the spelling of these argument tokens",
  which needs the preprocessing record (KR4, M4).
- **Probing every observer** (§8.2) costs a parse per observer per run
  even for a rename confined to one file. Proposed: the map-phase walk
  writes a Bloom filter over the key ids of the observer's units into its
  shard; an observer whose filter has none of the rules' keys, and that
  read no overlay file, is `unchanged` without a job. A filter has no
  false negative, so the trusted statement "this observer does not have
  the entity" stays trusted. Size and hash count to be chosen from the
  corpus's unit counts.
- **No fixed point** (§5.5). After a group is rejected, sites that
  overlapped only its sites could be accepted. Not done: with one group
  per run it never matters, and with several the order of rescue would
  have to be specified. To be decided with M3's groups.
- **`name_taken` is cautious**: an overload of a different signature
  already named `new_name` in the scope makes a legal program and may or
  may not change overload resolution. Verification would decide; the
  flag refuses first. Whether to drop the flag and let verification
  speak.
- **Selecting one overload** (`--signature`), a member of an unnamed
  namespace (which needs the main file as part of the target), and
  matching through inline namespaces (`std::f` for `std::__1::f`).
- **Identifiers outside the basic character set** in `target` and
  `new_name`, and names written with universal-character-names
  (`not_plain`).
- **Names the linker resolves**: `__attribute__((alias("…")))`,
  `ifunc`, `asm` labels and export lists name an entity by a string that
  a rename leaves stale. The digests are equal and the link fails. A
  rename could flag a subject that any such attribute's string names
  (comparing mangled names); until then only a build finds it.
- **Residue needs the preprocessing record** to say why a token was not
  compiled (skipped group, unsubstituted argument, pasted, a directive).
  Until KR4, `not_compiled` is one category and a test name such as
  `TEST(Suite, f)` is reported beside real leftovers.
- **Ownership by pattern.** `Ownership` is prefixes. Globs and a manifest
  file are wanted for trees where generated and written files share
  directories.
- **Votes are not weighed.** One observer's `keep` against ten thousand
  `replace` rejects the site. That is the conservative reading of D5;
  whether a transform may ask for something else is an M3 question.
- **A partial selection** (`--only`) verifies a subset and can never
  show agreement for the rest. Whether `rename` should accept it at all
  outside tests.
- **Rounds are never reclaimed.** Each run adds two shard directories to
  the plan. A store command to drop rounds is wanted.
- **`first` holds 8 observers** and the default `fan_in` is 64; both are
  guesses.
