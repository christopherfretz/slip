# edits — claims, reduce, apply, the verify loop, native rename

Status: draft (M2; reconciled with `spec/api.md` §7 for M3). Implemented
by `slip_model` (the formats), `slip_reduce` (§5, §6), `slip_apply` (§7),
`slip_refactor` (§9), `slip_kiln` (§2.3, §8.4), `slip_script` (§4.8) and
`slip_cli` (§8, §10). Design rationale:
[02-pipeline.md](../docs/design/02-pipeline.md),
[04-macros-and-configurations.md](../docs/design/04-macros-and-configurations.md).
Formats use the encodings and the container of [model.md](model.md); keys,
units and rules are those of [digest.md](digest.md); what "verified" means
is [verify.md](verify.md); what a transform written in C++ may ask for is
[api.md](api.md).

Two producers write what this file specifies: the native rename (§9) and
a programmable transform (`api.md`, M3). Both meet in one format, the
`edits` section of a map shard (§4), and everything after it (reduce,
apply, the verify loop) does not know which of them ran. Where `api.md`
has a name for a thing this file uses that name.

Nothing in this file is trusted. A transform, the reducer and the applier
can each be wrong; what catches them is verification, which takes from them
only the post-image, the declared changes and the spelling obligations
(`verify.md` §2, §10). No rule below makes a wrong edit safe. Each one
either makes a wrong edit less likely, or makes it be reported earlier and
more precisely than verification would report it.

## 1. Principles

1. **Edits are claims.** An observer does not edit. It claims that certain
   bytes of a file should be replaced, or must stay, or cannot be edited.
   Reduce decides from all claims; apply performs; verification judges.
2. **A claim is addressed to bytes of a file of known content.** The file is
   named by real path and content digest (`model.md` §6). No edit is ever
   applied to a file whose bytes do not have that digest.
3. **Determinism.** The edit set, the declared changes, the overlay, the
   post-image and the result are functions of the plan, the transform
   request, the options this file says they record, and the contents of
   the files read. They do not depend on the number of workers, on the
   order in which observers finish, on the time, or on the store's path.
4. **All or nothing per group.** An edit may belong to an atomic group. A
   group is applied whole or not at all. A request can make the whole run
   one group (`indivisible`, §3); a native rename does.
5. **Conservative.** Reduce never applies part of a group, and never
   iterates to rescue what remains after a rejection. Where it repeats a
   step (§5.5) the repetition only rejects ("Open questions").
6. **Byte exact.** Apply changes the claimed bytes and no others. It does
   not reformat, re-encode, or normalise line endings.
7. **The working tree is not touched** until a result permits it and the
   user asks (§7.3).
8. **Identity is content.** A claim, a group, a report and a declaration
   are identified by what they say, never by a number a producer assigned
   or by the order in which it worked (`api.md` §7.6 rule 1).

## 2. Files, ownership and tiers

### 2.1 Ownership

A transform request carries the **owned set**: the files Slip may edit.

```
Ownership := owned:    list<path>     canonical real paths of directories or files; sorted, no two equal
             readonly: list<path>     the same; sorted, no two equal; none also in `owned`
```

Every entry is a **canonical absolute path**: it starts with `/`, has no
empty, `.` or `..` component, has no NUL byte (no file has one), and has
no trailing `/` unless it is `/` itself. A reader refuses any other entry
(§3). `/` is canonical, an entry that covers every file, but it is not a
file: where a path names a file (an edit set's `files`, an overlay's
`files`, a residue's `file`), a reader refuses `/` too. The entry `/` **covers**
every absolute path; any other entry `e` covers a real path `p` iff `p`
equals `e` or `p` starts with `e` followed by `/` (so `/a/b` covers
`/a/b/x` and not `/a/bc/x`). A file is **owned** if some entry covers it
and the longest entry covering it is in `owned`. Every other file is
**read-only**: system headers, dependencies, generated files in a build
tree, and whatever `readonly` carves out of an owned directory.

`owned` must not be empty. Entries are made real paths when the request is
built (§10.1); an entry that does not exist is an `Input` error.

This is the ownership `api.md` §6.6 rule 6 cites: `is_editable(r)` is
true exactly when `r` is not null and its file is owned.

### 2.2 Tiers

The **tier** of a token the parser saw
([04](../docs/design/04-macros-and-configurations.md) §2):

| Tier | The token's bytes are | M2 |
|---|---|---|
| 0 | written in a file, outside every macro argument | editable |
| 1 | written in a file, inside one argument of one macro invocation, and reached the parser only by parameter substitution | editable; every substitution is a vote (§4.3) |
| 2 | written in the replacement list of a macro definition | refused (`macro_body`) |
| 3 | made by `##` or `#`, or by a builtin macro | refused (`pasted`) |
| X | not one token of one of the above: a name whose tokens come from different expansions, or from a buffer that is not a file (command-line definitions, predefines) | refused (`crossing`, `not_a_file`) |

Tiers 0 and 1 are those of `api.md` §5.4, and one function gives their
ranges to both producers: `slip_kiln::ranges` (`docs/wp/WP-084.md`),
which gives the file range of a token span, or none. Tiers 2, 3 and X
are this file's names for *why* a name is not edited; `slip_kiln::sites`
finds them (§2.3).

**The range is Kiln's.** Which bytes of a file an edit must replace to
replace a range of tokens, and where a token ends, are answered by
Kiln's own functions (Kiln WP-2150: `SourceManager::editable_file_range`,
Kiln `spec/source.md` "Editable file ranges"; `kiln_lex::token_end`,
Kiln `spec/lex.md` "End of a token"), the ones Kiln's fix-its use.
`slip_kiln::ranges` calls them and Slip has no rule of its own for
either. They need Slip's Kiln revision at or after `913a490bb`.

**The tier is where the token is written**, which Kiln has no function
for and Slip reads from Kiln's expansion entries: the token's location
is followed to its immediate spelling while it is in a macro-argument
entry; what is reached is a location in a file (tier 0 if no step was
taken, tier 1 otherwise), in a replacement list (2), in scratch space
(3), or in a buffer that is not a file (X).

A name is edited only when both agree: it is written at tier 0 or 1,
Kiln gives a range for its span, and the bytes of that range are the
name (§2.3). The two rules differ in one place, a name that is a whole
macro expansion, for which Kiln gives the invocation
([KF3](../docs/requests/KF3-location-utilities.md) §2.1):

- `ID(foo)` with `#define ID(x) x`: the token is in tier 1, Kiln's
  range is the seven bytes of the invocation, and they are not the
  name. It is refused (`not_plain`). Slip does not go to the argument's
  bytes by a rule of its own; a range for them is a question to Kiln
  (KF3 §2.2), and until Kiln answers such a name is not edited.
- `NAME` with `#define NAME foo`, and a use of `foo` with `#define foo
  foo`: the token is written in a replacement list. It is refused
  (`macro_body`), whatever Kiln's range is.

A token is tier 1 only if every expansion between its bytes and the parser
is the substitution of a parameter by an argument. An argument that is
itself the name of a macro whose body holds the token gives a tier 2
token.

Tier 0 and tier 1 are properties of an observer, not of the bytes: the
same bytes are an argument in an observer where the name before `(` is a
macro and plain text where it is a function. A claim therefore carries
the set of tiers it was derived at, and reduce records every tier a site
was claimed at (§5.3).

### 2.3 File ranges

A claim's range is `[lo, hi)` in byte offsets of the file's bytes: the
bytes its content digest is computed over. `slip_kiln::ranges` maps a
Kiln token span to a file range (§2.2). Kiln's offsets are offsets in
its text of the file, which are offsets in the file's bytes exactly when
Kiln changed nothing on loading (`file_recoded` below). Where it gives
none, or the range may not be edited, `slip_kiln::sites` gives the
reason, and the native rename claims a refusal with it (§4.2); a
programmable transform sees a null range or `is_editable` false
(`api.md` §5.4, §6.6) and requests nothing.

| Reason | Condition |
|---|---|
| `macro_body`, `pasted`, `crossing`, `not_a_file` | the tier (§2.2); `crossing` also when the token is written at tier 0 or 1 and Kiln gives no range for it |
| `not_owned` | the file is read-only (§2.1) |
| `file_recoded` | the front end's text of the file is not the file's bytes: Kiln replaced invalid UTF-8 when it loaded the file (its load report is not clean), so offsets in the text are not offsets in the file |
| `not_plain` | the bytes of the range are not the token's spelling: the name is written with a universal-character-name, a line splice or a trigraph, or the range is a macro invocation that expands to the name (§2.2) |

When several conditions hold, the reason is the first in the order of the
table. A byte-order mark is part of Kiln's text of a file and shifts no
offset.

## 3. The transform request

Kind `TRFM`, version 1, deterministic. One required section `transform`
(id 1):

```
Transform := kind:        TransformKind
             ownership:   Ownership           §2.1
             indivisible: bool                every edit of the run is one atomic group (§5.5)
             rename:      opt<Rename>         present iff kind = rename
             script:      opt<Script>         present iff kind = script

TransformKind := rename = 1 | script = 2

Rename := target:   list<bytes>     §9.1: the qualified name, outermost component first; at least one
          kind:     TargetKind
          new_name: bytes           an identifier

TargetKind := any = 0 | function = 1 | variable = 2

Script := id:      digest           the `TransformId` of api.md §2.2
          file:    path             the transform's source file, as given
          options: list<bytes>      the author's options (api.md §2.1 rule 2), in the order given
          steps:   uv               the budgets of api.md §2.5
          bytes:   uv
          depth:   uv
```

Every entry of `ownership` is a canonical absolute path (§2.1); an entry
that is not is `Malformed` at that entry, path
`sections.transform.ownership.owned[i]` or `….readonly[i]`, checked
before the list's order (`spec/landed/edits/WP-060.md` §3).

For a rename: every component of `target` and `new_name` matches
`[A-Za-z_][A-Za-z0-9_]*`, is not a keyword of any C++ standard Kiln
implements, and `new_name` differs from the last component; `indivisible`
is true. (Identifiers outside the basic character set: "Open questions".)

For a script: the coordinator compiles the transform unit once before any
worker starts (`docs/wp/WP-092.md`) and puts the `TransformId` it
computed in the request. A worker whose own compilation of the transform
unit has another `TransformId` (a file of the transform changed during
the run) fails its jobs (`exec.md` §5.4): no shard of that worker is
written under this request. `indivisible` is false unless the command
asks for it (§10.1).

The **request id** is the content id of the container. A map job names
it, and the map shard repeats it (§4), so shards of different requests
are never reduced together. Since the request holds the `TransformId`,
this is the refusal of `api.md` §2.2, which is stated by request id.

## 4. Claims: the `edits` section

Section 17 of a shard of phase `map` (`model.md` §8). Present iff the
job carried a transform and the outcome is `ok`. It is what one run of
the transform over this observer produced: its claims and what goes with
them, or the record that the run failed.

```
Edits := scheme:    uv               1
         transform: digest           the request id (§3)
         failure:   opt<Failure>     §4.6: the run failed. Then every list below except `files` is empty
         files:     list<FileRef>    the files this section names; sorted by path, no two equal
         subjects:  list<Subject>    §4.1; sorted by `id`, no two equal
         links:     list<Link>       §4.1; sorted by (a, b), no two equal
         claims:    list<Claim>      §4.2; sorted as there, no two equal in the sort key
         groups:    list<list<uv>>   §4.4
         reports:   list<Report>     §4.5
         changed:   list<Changed>    §4.5
         renamed:   list<Rule>       §4.5
```

Every entry of `files` is also in the shard's `inputs` with the same size
and content, or is the main file. A **place** is a range of one of them:

```
Place := file: uv, lo: uv, hi: uv        `file` an index into `files`; lo <= hi <= the file's size
```

The counters of a run (`api.md` §7.6 rule 6, and §6.3 below) are in the
shard's `counters`, not in this section.

### 4.1 Subjects

An observer may list the entities the transform is about, as far as it
can see them. The native rename does; a programmable transform has no
operation that makes one (§4.8).

```
Subject := id:      KeyId            the entity's key id in this observer, with no rules (digest.md §3)
           rule:    opt<Rule>        the rule that undoes what the transform does to the entity's key
                                     (verify.md §2.1); absent if its key does not change
           role:    Role
           of:      list<KeyId>      for a follower: the entities it follows; sorted, not empty. Else empty
           flags:   uv               SubjectFlags
           display: str              the entity as a person would write it; for reports only

Role := seed = 0          the request names it (§9.1)
      | candidate = 1     it must change if an entity it is linked to does (§9.2)
      | follower = 2      its key changes because of the entities in `of` (§9.2)

SubjectFlags := bit 0  read_only      some declaration of the entity is in a read-only file
                bit 1  unsupported    the transform cannot change an entity of this kind (§9.5)
                bit 2  name_taken     the scope already declares the new name (§9.5)
                bit 3  unreachable    the entity is referenced where the transform cannot find the name (§9.5)
```

```
Link := a: KeyId, b: KeyId           a < b; both are subjects of this shard
```

A link says the two entities change together or not at all. A subject's
`rule` is in force only if reduce **selects** the subject (§5.2); that is
how a rule differs from an entry of `renamed` (§4.5), which is in force
whatever else happens.

The producer gets `id` and `rule` from `slip_digest::query::Keys`
(`verify.md` §9.5): they are the key the walk gives the entity and the
rule the walk will match, not a second computation of either.

### 4.2 Claims

```
Claim := file:     uv                index into `files`
         lo:       uv
         hi:       uv                lo <= hi <= the file's size
         stance:   Stance
         pieces:   list<Piece>       the replacement; empty unless stance = replace
         subjects: list<KeyId>       the subjects the bytes denote here; sorted, no two equal
         reason:   Reason            `none` unless stance = refuse
         tiers:    uv                bit t set: the claim was derived at tier t (t = 0, 1, 2, 3; bit 4: X)
         macro:    opt<MacroSite>    absent if `tiers` is bit 0 alone
         votes:    uv                times this observer derived the claim; at least 1
         notes:    list<bytes>       shown beside the edit in reports; sorted, no two equal

Stance := replace = 0        these bytes become the text of `pieces`
        | keep = 1           these bytes denote something the transform must not change
        | refuse = 2         these bytes would have to change and cannot
        | unresolved = 3     these bytes may denote a subject; this observer cannot tell

Piece := literal = 0: bytes                         not empty
       | copy    = 1: Place                         lo < hi
       | spelled = 2: (key: KeyId, text: bytes)     `text` not empty

Reason := none = 0 | macro_body = 1 | pasted = 2 | crossing = 3 | not_a_file = 4 | not_owned = 5
        | file_recoded = 6 | not_plain = 7 | mixed_set = 8 | mixed_instances = 9

MacroSite := name_start: uv          offset, in the claim's file, of the name token of the outermost
                                      macro invocation the bytes are an argument of or were produced by
             name:       bytes       that macro's name
```

**Pieces** are those of `api.md` §7.1, and mean what it says:

| Piece | Means |
|---|---|
| literal | its bytes |
| copy | the text of the range in the result of the run: its bytes with every accepted edit that lies properly inside the range applied (§5.4, §7.1). Copy is by reference, not by value |
| spelled | `text`, written as a name that is meant to denote the entity `key`. Verification checks that it does (`verify.md` §10); nothing else in this file reads `key` |

No two literal pieces are adjacent. A `replace` with no piece deletes the
bytes. A `KeyId` in a piece is in the key space of the pre-image
(`verify.md` §2).

The **site** of a claim is `(file path, lo, hi)`. Claims are sorted by
`(file, lo, hi, stance, pieces)`: `pieces` as a sequence, a piece by its
tag and then its fields, bytes and ids by their bytes. Two claims equal
in that key are one claim (§4.3). A `replace` claim is an **edit**, and
its identity is `(file path, lo, hi, pieces)`, with the file of a copy
piece taken as its path: this is the edit of `api.md` §7.6.

- A `replace` claim has `lo < hi` (insertion is not in version 0), tiers
  0 and 1 only, and is in an owned file. Its `pieces` may be empty.
- `keep` and `unresolved` claims have tiers 0 and 1 only, and their range
  is a token's file range (§2.3).
- A `refuse` claim's range is where a person must look: for `macro_body`
  the token's bytes in the macro definition; for `not_owned` and
  `not_plain` the range Kiln gave for the name (§2.2), in whatever file
  it is; for every other reason the name token of the outermost
  invocation, or `[0, 0)` of the main file if there is none. `subjects`
  is not empty.
- `keep` is claimed for every name token whose bytes equal those a
  `replace` of the transform would replace elsewhere and that this
  observer resolved to an entity that is not a subject (rename: every
  name spelled like the old name that denotes something else). `subjects`
  is then empty.
- `subjects` names subjects of the shard, of any role. A claim of a
  producer without subjects has none.
- `notes` and `subjects` are no part of a claim's identity.

Only the native rename makes `keep`, `refuse` and `unresolved` claims. A
programmable transform has no operation for them; what it does not
request, it is silent about (`api.md` §7.3 rule 7).

### 4.3 Votes

One range of file bytes can be seen by the parser several times in one
observer: an argument substituted twice, a header included twice. Each
time is one derivation. Derivations that agree in the sort key are one
claim, with `votes` their number, `tiers` the union, `macro` the least
`MacroSite` (by `name_start`, then `name`), `reason` the least number
other than `none`, and `subjects` and `notes` the unions. Derivations
that disagree are several claims at one site, and reduce sees the
disagreement. A producer that does not count derivations writes 1.

### 4.4 Groups

```
groups: list<list<uv>>     each a strictly ascending list of two or more indices of `replace` claims;
                           the lists sorted as sequences; no index in two lists
```

The members of a list are applied all or none (`api.md` §7.3 rule 4). A
group's identity is the set of its members' identities, so it does not
depend on the indices. Groups of one run that share a member are one
group, and a group of one member is no group (`api.md` §7.6 rule 2): a
producer writes neither. Under an `indivisible` request the whole run is
one group whatever this list says, and the native rename writes none.

### 4.5 Reports and declarations

```
Report := place:    opt<Place>       absent: the subject unit as a whole
          severity: Severity
          message:  bytes
          attached: bool             the place is the nearest the producer could name, not the place
                                     meant (api.md §7.4 rule 3)

Severity := note = 0 | review = 1 | error = 2

Changed := key:    KeyId             a unit whose digest is meant to differ
          place:  Place             lo < hi: the bytes whose edit is the reason
          reason: bytes
```

A note, a report's message and a declaration's reason are `bytes`, not
`str`: a programmable transform makes them from character arrays, and
`api.md` §7 does not require them to be UTF-8. They are compared and
sorted as bytes, and nothing of a run output is lost or refused for its
encoding. They are text only where they are printed (§10.5). A failed
run's texts (§4.6) are `str`: the host and Kiln's renderer make them.

- `reports` is sorted by `(place, severity, message, attached)`, an
  absent place first, and has no two equal entries.
- `changed` is sorted by `(key, place, reason)` and has no two equal
  entries. An entry is **in force** only if reduce accepts an edit that
  shares a byte with its place (§6.2; `api.md` §7.5 rule 7). There is no
  entry without a place.
- `renamed` is a rule table (`verify.md` §2): sorted by
  `(parent, kind, name, disc)`, no two entries with the same. Its rules
  are in force whether or not any edit is accepted. A rule cannot make
  two different programs compare equal (`verify.md` §2.1), so a rule
  whose edits were rejected costs a reported difference and nothing
  else.

A report of severity `error` says the transform could not do its job in
this observer. §8.5 says what it does to the result.

### 4.6 A failed run

A run of a programmable transform can fail (`api.md` §2.8). The shard's
outcome is still `ok`: the observer parsed, and its `digests` are those
of any map shard. `failure` records the run:

```
Failure := kind:     FailureKind
           message:  str                  the diagnostic's first line
           place:    opt<SourcePlace>     where in the transform's source it arose
           rendered: str                  the diagnostics as Kiln renders them, the call stack included
           refusal:  opt<HostRefusal>     for `uncaught` with a `slip::error`

FailureKind := uncaught = 0 | not_constant = 1 | step_budget = 2 | memory_budget = 3
             | depth_limit = 4 | host_failure = 5

SourcePlace := file: path, line: uv, column: uv
HostRefusal := function: str, errc: str, message: str, subject: opt<Place>
```

`errc` is the enumerator's name (`api.md` §4). A failed run hands over
nothing else (`api.md` §7.6 rule 5): `subjects`, `links`, `claims`,
`groups`, `reports`, `changed` and `renamed` are empty, and `files` holds
at most the file of `refusal.subject`. An observer whose run failed was
not examined by the transform; it is still verified like every other
(§8.2). The native rename does not fail this way: an error in it fails
the job.

### 4.7 Invariants

| Rule | Offset | JSON path |
|---|---|---|
| `scheme` is 1 | the field | `sections.edits.scheme` |
| with a `failure`: the seven lists are empty and `files` has at most one entry | the first byte of the first list that is not, in field order | that list |
| with a `failure`: `refusal.subject`, if present, is within its file (`file` an index of `files`, `lo <= hi <= size`); `files` is empty unless `refusal.subject` is present | the `subject` field's opt tag; the `files` count | `sections.edits.failure.refusal.subject`; `sections.edits.files` |
| `files` strictly ascending by path | that `FileRef`'s first byte | `sections.edits.files[i]` |
| `subjects` strictly ascending by `id` | that subject's first byte | `sections.edits.subjects[i]` |
| `of` not empty iff role is `follower`; strictly ascending | the `of` field | `sections.edits.subjects[i].of` |
| flag bits above 3 are 0 | the `flags` field | `sections.edits.subjects[i].flags` |
| `links` strictly ascending; `a < b` | that link's first byte | `sections.edits.links[i]` |
| a claim's `file` is an index of `files`; `lo <= hi <= size`; `lo < hi` for `replace` | the claim's first byte | `sections.edits.claims[i]` |
| `pieces` empty unless `replace`; no empty literal or spelled text; no two literals adjacent; a copy's place is within its file and not empty | that piece's first byte, or the `pieces` field | `sections.edits.claims[i].pieces[j]` |
| `reason` is `none` iff the stance is not `refuse` | the `reason` field | `sections.edits.claims[i].reason` |
| `subjects` and `notes` strictly ascending | the field | `sections.edits.claims[i].subjects`, `sections.edits.claims[i].notes` |
| `tiers` not 0, bits above 4 are 0, bits 2 to 4 only for `refuse`; `macro` absent if `tiers` is 1 | the `tiers` field | `sections.edits.claims[i].tiers` |
| `votes` at least 1 | the `votes` field | `sections.edits.claims[i].votes` |
| `claims` strictly ascending in the sort key | that claim's first byte | `sections.edits.claims[i]` |
| a group has two or more members, strictly ascending, each the index of a `replace` claim; no index in two groups; `groups` strictly ascending | that group's first byte | `sections.edits.groups[i]` |
| `reports` strictly ascending; a place within its file; `severity` at most 2 | that report's first byte | `sections.edits.reports[i]` |
| `changed` strictly ascending; a place within its file and not empty | that entry's first byte | `sections.edits.changed[i]` |
| `renamed` strictly ascending by the bytes of the encoded `(parent, kind, name, disc)` | that rule's first byte | `sections.edits.renamed[i]` |

Checked in this order, the claim rules for each claim in turn. That a
subject of a claim or a link names a subject of the shard, that a
subject's `id` and `rule` belong together, that `files` agrees with
`inputs`, and that a `replace` is in an owned file, are checked by reduce
(§5.1), which reports them as `Input` errors naming the shard.

### 4.8 From a run output

`api.md` §7.6 fixes what a run of a programmable transform hands over;
this is how `slip_script` writes it (`docs/wp/WP-092.md`):

| Run output (`api.md` §7.6) | In the shard |
|---|---|
| `RunOutput.transform`, `RunFailure.transform` | not written: it must equal `Script.id` of the request the job names, else the job fails (§3). `Edits.transform` is the request id |
| an `Edit` | a `replace` claim: `file`, `lo`, `hi`, `pieces` and `notes` as they are; `tiers` the edit's set, bit t for tier t; `subjects` empty; `reason` `none`; `votes` 1; `macro` given when the host knows the invocation |
| `groups` | `groups`: the indices are the claims' indices, since a run output has only `replace` claims and the same sort order |
| a `Report`, a `Changed` | the same record |
| `renamed` | `renamed` |
| `counters` | the shard's `counters` |
| a `RunFailure` | `failure`; its counters in the shard's `counters` |

Two requests of one run with one file, range and pieces are one `Edit`
there and one claim here; if they were made at different tiers, the
edit's `tiers` has both and so has the claim (`api.md` §7.6 rule 1).
Nothing of a run output is lost in the shard, and a
shard of a programmable transform has nothing a run output lacks except
`votes` and `macro`.

What a native producer has and `api.md` does not: subjects, links, and
the stances `keep`, `refuse` and `unresolved`. A list of what `api.md`
would need to offer them is in "Open questions".

## 5. Reduce

Reduce is a function

```
reduce(request, plan, round, the map shards of that round for the selected observers)
    -> (EditSet, opt<Declared>)
```

It reads the request and shards and nothing else: no source file, no
front end. The selected observers are in plan order, and **the index** of
an observer is its position in the plan.

### 5.1 Inputs

- A selected observer with no shard, or whose shard is not `ok`, is
  **unparsed**. Reduce counts it and goes on; it contributes nothing.
- Every `ok` shard must have an `edits` section with `scheme` 1 and
  `transform` the request's id. Otherwise reduce fails with an `Input`
  error naming the shard. A shard that breaks a rule of §4.7's last
  paragraph fails the same way.
- An `ok` shard with a `failure` is a **failed run**. Reduce records the
  failure (§6) and takes nothing else from the shard.
- If no shard is `ok` and without a failure, the edit set is refused with
  `no_observer`.

### 5.2 Subjects

1. **Union.** The subjects of all shards are joined by `id`. Two records
   of one `id` must have equal `rule` (anything else is an `Input` error:
   a key determines its rule). The joined record is a `seed` if any
   record is; else a `candidate` if any is; else a `follower`. `flags`
   are or-ed; `of` is the union; `display` is that of the least observer
   index. Links are joined as a set.
2. **Selection.** The **selected** subjects are the least set that holds
   every seed and, with one end of a link, the other. A follower is
   selected if every entity in its `of` is selected.
3. **Refusals of subjects**, each one refusal of the edit set (§6):

| Cause | Condition |
|---|---|
| `no_subject` | the request is a rename and there is no seed |
| `subject_read_only` | a selected subject or follower has `read_only` |
| `subject_unsupported` | a selected subject or follower has `unsupported` |
| `subject_unreachable` | a selected subject or follower has `unreachable` |
| `name_taken` | a selected subject or follower has `name_taken` |
| `mixed_follower` | a follower has some of its `of` selected and some not |

A candidate that is not selected is not a subject. A `replace` claim
whose `subjects` is not empty and holds no selected subject is treated as
`keep`; a `refuse` claim all of whose `subjects` are unselected is
dropped. A `replace` or `refuse` claim with some subjects selected and
some not is the refusal `mixed_set` at its site. A claim with no subject
is not touched by selection.

These rules are applied to collapsed claims (§5.3), whose `subjects` is
the union over the observers that made the claim: selection needs every
shard's subjects and links, and the collapse does not. A claim so turned
into a `keep` loses its pieces, leaves any group it was in, and is joined
with the site's `keep` claim if there is one.

### 5.3 Collapse

Claims are joined by site, then within a site by `(stance, pieces)`, a
copy piece compared by its file's path and its range. The join of claims
with one site, stance and pieces is one **collapsed claim**:

| Field | Join |
|---|---|
| `subjects` | union |
| `tiers` | bitwise or |
| `macro` | least, as in §4.3; absent if none has one |
| `reason` | least number other than `none` |
| `notes` | the least 8 of the union, by bytes |
| `observers` | the number of observers that made the claim |
| `votes` | the sum |
| `first` | the least 8 observer indices |

"Identical claims" are claims equal in site, stance and pieces. Ten
thousand observers that claim the same replacement of the same bytes are
one collapsed claim with `observers` 10000 and eight sample observers.

**Identity is strict.** The key of a spelled piece is part of it. Two
observers that replace one range with the same text and mean different
entities by it have made two different edits of one range (§5.4,
`divergent`). So have one that writes a spelled piece and one that writes
the same bytes as a literal.

This is decided, for M2 and for a script alike: `divergent` refuses, and
reduce does not accept a site on equal text alone. The case it costs is
an overload set seen differently: two observers that resolve one call to
two different overloads of the renamed name (a type that differs between
configurations) write spelled pieces with different keys, and the rename
is refused where a text-only identity would have accepted it. The
refusal names the site and both observers. A spelled piece that carries
a set of keys, joined across observers by union when the text is equal,
with the obligation holding when the name denotes one of them, would
accept it by weakening the check to "one of the entities some observer
meant"; it is not in this specification, and it would need the trusted
base's agreement (`verify.md` §10) and `api.md` §7.6 to say so.

The join is associative and commutative, so the collapsed claims do not
depend on the order or the grouping in which shards are read. Reports,
`changed` entries and failures are joined the same way, each by its
whole record with places by path, to a record with `observers` and
`first` (§6).

### 5.4 Outcome of a site

For each site, from its collapsed claims after §5.2's treatment of
unselected subjects:

| The site has | Outcome |
|---|---|
| `replace` claims with one `pieces`, and nothing else | **live**: a candidate edit |
| `replace` claims with one `pieces`, and `unresolved` | **live** |
| `replace` claims with two or more different `pieces` | rejected: `divergent` (`api.md` §7.1 rule 3a) |
| `replace` and `keep` | rejected: `contested` |
| `refuse` (whatever else) | rejected: `site_refused` with the reason |
| `keep` only, or `keep` and `unresolved` | **kept**: listed, not an edit |
| `unresolved` only | **unresolved**: residue of category `dependent` (§8.4) |

A live site whose claims were made at tier 1 in some observers and at
tier 0 in others is live; both bits are in the edit.

**Geometry.** Live sites are not empty ranges. Two ranges of one file
**cross** if they share a byte and neither is within the other; `a` is
**properly inside** `b` if it is within `b` and is not `b`. The ranges a
live edit **copies** are the places of its copy pieces. Each rule below
is applied to the live sites that the rules before it left:

| Cause | Rejected |
|---|---|
| `contested` | a live site, when a kept or unresolved range of another site shares a byte with it |
| `overlap` | two live sites that cross: both. A live site that crosses a range another live edit, or itself, copies: both sites (`api.md` §7.1 rule 3) |
| `cyclic` | say an edit **draws on** another when the other is properly inside a range the first copies. Every live edit on a cycle of that relation, a cycle of one included: `replace(x, copy(r))` with `x` properly inside `r` (`api.md` §7.1 rule 3b) |

What is left is a family of ranges in which any two are disjoint or
nested, no copied range is crossed, and copies are well-founded. An edit
within a copied range and an edit that contains one are not conflicts;
§7.1 says what they mean. An edit of exactly a copied range is not
applied to the copy (`api.md` §7.1 rule 3).

**File agreement.** For every file named by a live site or copied by a
live edit, every `ok` shard that has the path in `inputs` or in `files`
must give it the same size and content digest. Otherwise every live site
of the file, and every live edit that copies from it, is rejected with
`file_differs`: the file changed while the phase ran, or two observers
read different files through one real path.

### 5.5 Groups

Two edits are **tied** if some shard has them in one group (§4.4). The
**atomic groups** of the run are the classes of live and rejected edits
under the transitive closure of "tied"; an edit tied to nothing is a
group of its own. Under an `indivisible` request all edits are one
group. So two observers that see one edit in different groups have made
one larger group: all or none is transitive (`api.md` §7.6 rule 2).

A group is **rejected** if any edit of the group is at a rejected site,
or any refusal of §5.2 applies to a subject one of its claims names. All
rejections of §5.4 are computed first, over the live sites of every
group; then groups are rejected. A site that was rejected for crossing a
site of a group that is itself rejected stays rejected.

**Effect.** Among the edits that remain, an edit is **outermost** if it
is within no other, and **effective** if it is outermost or is drawn on,
through a copy in which no other remaining edit properly inside the
copied range contains it, by an effective edit. An edit that is not
effective would be accepted and change nothing: its bytes are replaced
by an edit around it, and nothing carries it elsewhere. Reduce publishes
no such edit. Every remaining edit that is not effective is rejected
with `buried`, together with the innermost remaining edit that contains
it, and with the groups of both (`api.md` §7.1 rule 3c: an edit that
replaces a range without copying it, over an edit some observer asked
for inside it, is a disagreement between two requests, and both are
rejected); this is repeated until every remaining edit is effective. A repetition only rejects, and it ends because the
set shrinks.

The edits of the edit set are the edits that remain. If there are none
the edit set's `state` is `refused`: with the refusals that explain it,
or with `no_edit` if no observer claimed a `replace`.

### 5.6 Cost

Reduce must run in memory that does not grow with the number of
observers or of claims:

- Shards are read in runs of at most `fan_in` (default 64) in plan order.
  Each run is merged by site into a temporary file of collapsed claims
  under `tmp/` of the store; temporary files are merged `fan_in` at a time
  until one remains. Collapsed claims have bounded size apart from their
  pieces (`first` holds at most 8 indices, `notes` at most 8 entries;
  `subjects` is bounded by the distinct entities at one token), so a site
  claimed by every observer costs one record at every level. Reports,
  `changed` entries and failures are merged the same way.
- At any time reduce holds at most `fan_in` shards' `edits` sections, the
  joined subjects, links and rules, and one buffer per open temporary
  file.
- File agreement reads each shard's `inputs` once, in a second pass over
  the shards, keeping only the paths of live sites and of copied files.
- The first level may run on several threads (one run of shards each).
  By §5.3 the result is the same.

Three things grow with the transform and not with the observers, and are
held in memory, one record of bounded size each: each distinct edit that
is a member of a group, unless the request is `indivisible` (§5.5); each
copy piece of a live edit; and each live edit that lies within another
live edit or within a copied range (§5.4, §5.5). A native rename has none
of the three.

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

pub fn reduce(request: &Transform, request_id: Digest, plan: Digest, round: u64,
              shards: &dyn Shards, options: &Options) -> Result<Reduced, ReduceError>;

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
           transform:  digest                the request id
           basis:      digest                H("slip.basis.v1", list<Basis>): the shards reduced
           state:      SetState
           files:      list<FileRef>         canonical real paths of files, sorted by path; exactly the files the records below name
           subjects:   list<Subject>         the selected subjects and followers, joined (§5.2); sorted by id
           rules:      list<Rule>            the `renamed` rules of the shards, joined (§6.2); a rule table
           edits:      list<Edit>            sorted by (file, lo, hi), no two with one range; empty unless accepted
           groups:     list<list<uv>>        the atomic groups of two or more of `edits`, as §4.4; empty under
                                             an `indivisible` request, where all of `edits` is one group
           changed:    list<Exempt>          sorted by (key, place, reason), no two equal
           kept:       list<Place>           sorted by (file, lo, hi), no two equal
           unresolved: list<Place>           the same
           refusals:   list<Refusal>         sorted as below
           reports:    list<Reported>        sorted as §4.5 sorts reports, no two equal in that key
           failures:   list<Failed>          sorted by (kind, message, place), no two equal in that key
           unparsed:   list<digest>          observers without an `ok` shard; in plan order
           counters:   list<Counter>         §6.3; sorted by name

Basis    := observer: digest, shard: opt<digest>      in plan order; `shard` the content id, absent if unparsed
SetState := accepted = 0 | refused = 1

Edit := file: uv, lo: uv, hi: uv
        pieces:    list<Piece>       a copy's file is an index of this set's `files`
        subjects:  list<KeyId>       sorted, no two equal
        tiers:     uv                the tier rules of a `replace` claim (§4.7)
        macro:     opt<MacroSite>
        notes:     list<bytes>       sorted, no two equal; at most 8
        observers: uv
        votes:     uv
        first:     list<digest>      the observers of the least 8 indices that made the claim, in plan order

Exempt   := key: KeyId, place: Place, reason: bytes
            in_force:  bool          an accepted edit shares a byte with `place` (§6.2)
            observers: uv
            first:     list<digest>

Reported := report: Report           §4.5, its place in this set's `files`
            observers: uv
            first:     list<digest>

Failed   := failure: Failure         §4.6; `rendered` and `refusal` those of the first observer
            observers: uv
            first:     list<digest>

Refusal := cause:     Cause
           file:      opt<uv>         present iff the cause is of a site (`Cause` below)
           lo:        uv              0 when `file` is absent
           hi:        uv              0 when `file` is absent
           reason:    Reason          for `site_refused`; else `none`
           subject:   opt<KeyId>      the subject, for a refusal of a subject (absent for `no_subject`); else the least
                                      subject of the site, if any (absent for `no_observer`, `rule_conflict`, `no_edit`)
           macro:     opt<MacroSite>  absent unless of a site
           other:     opt<Place>      for `contested` by another range, `overlap` and `buried`: the least other range
                                      involved; absent for every other cause
           observers: uv
           first:     list<digest>

Cause := no_observer = 0 | no_subject = 1 | subject_read_only = 2 | subject_unsupported = 3
       | subject_unreachable = 4 | name_taken = 5 | mixed_follower = 6 | mixed_set = 7
       | site_refused = 8 | contested = 9 | divergent = 10 | overlap = 11 | file_differs = 12
       | cyclic = 13 | buried = 14 | rule_conflict = 15 | no_edit = 16
```

The causes **of a site** are `mixed_set`, `site_refused`, `contested`,
`divergent`, `overlap`, `file_differs`, `cyclic` and `buried` (§5.2,
§5.4, §5.5). The others are of the set: the refusals of a subject (§5.2),
`no_observer` (§5.1), `rule_conflict` (§6.2) and `no_edit` (§5.5).
`no_edit` stands only in a set with no edit and beside no refusal that
needs a `replace` claim (`contested`, `divergent`, `overlap`,
`file_differs`, `cyclic`, `buried`): it says no observer claimed one. A
refused set has at least one refusal.

Refusals are sorted by `(cause, file, lo, hi, subject)`, an absent `file`
or `subject` before any present one; no two are equal in that key. For a
refusal of a subject, `observers` and `first` count the observers whose
shard set the flag (for `no_subject`, `no_observer`, `no_edit` and
`rule_conflict`: 0 and empty). For `contested`, they count the observers
that claimed `keep`; for the others, the observers of the rejected
claims.

Failures are joined by `(kind, message, place)`: the same failure in many
observers is one record with a count (`api.md` §2.8 rule 3).

A refused edit set keeps every list but `edits` and `groups`: they are
the report.

### 6.1 Invariants

The rows are checked in order. Within a row, its lists in field order,
the elements of each in order, and every rule of the row for one element
before the next element (`spec/landed/edits/WP-061.md` §2 lists each
row's rules and messages).

| Rule | Offset | JSON path |
|---|---|---|
| `scheme` is 1 | the field | `sections.set.scheme` |
| `basis` is the tagged digest of the list implied by `unparsed` and the reader's shards | not checked by the format | — |
| every `files` path canonical (`transform::is_canonical`) and not `/` (§2.1) | that `FileRef` | `sections.set.files[i]` |
| `files` strictly ascending by path | that `FileRef` | `sections.set.files[i]` |
| the subject rules of §4.7: ascending by id, then `of`, then the flags, each over the whole list | that subject, its `of`, its `flags` | `sections.set.subjects[i]`, `….of`, `….flags` |
| `rules` strictly ascending as a rule table | that rule | `sections.set.rules[i]` |
| `edits` and `groups` empty if `state` is `refused` | the first edit, else the first group | `sections.set.edits[0]`, `sections.set.groups[0]` |
| `edits` strictly ascending by `(file, lo, hi)`; `lo < hi`; no two of one file cross, and none crosses a range an edit copies (§5.4) | that edit | `sections.set.edits[i]` |
| the piece rules of §4.7 | that piece | `sections.set.edits[i].pieces[j]` |
| an edit's `subjects` and `notes` strictly ascending; its `tiers` obey the tier rules of a `replace` claim (§4.7) | the field | `sections.set.edits[i].subjects`, `….notes`, `….tiers` |
| the group rules of §4.7, over `edits` | that group | `sections.set.groups[i]` |
| every place within its file's size, and a `changed` place not empty; a refusal without `file` has `lo` and `hi` 0 | that record | its path |
| every file is named by a record: an edit, a copy piece, a `changed`, `kept` or `unresolved` place, a refusal's `file` or `other`, a report's place, a failure's `refusal.subject` | that `FileRef` | `sections.set.files[i]` |
| a `changed` entry's `in_force` is true iff an edit shares a byte with its place (§6.2) | the field | `sections.set.changed[i].in_force` |
| `first` has at most 8 entries and at most `observers`; `notes` at most 8 | the field | its path |
| `changed`, `kept`, `unresolved`, `refusals`, `reports`, `failures` strictly ascending in their keys | that record | its path |
| a refusal's `reason` is not `none` iff its cause is `site_refused` | the field | `sections.set.refusals[i].reason` |
| a refusal's fields against its cause (§6): `file` present iff of a site; `subject` present for a refusal of a subject but `no_subject`, absent for `no_subject`, `no_observer`, `no_edit`, `rule_conflict`; `macro` absent unless of a site; `other` only for `contested`, `overlap`, `buried`; `observers` 0 for `no_subject`, `no_observer`, `no_edit`, `rule_conflict`; `no_edit` only with no edit and beside no refusal that needs a `replace` claim | the field, or the refusal for `no_edit` | `sections.set.refusals[i].file`, `….subject`, `….macro`, `….other`, `….observers`, `sections.set.refusals[i]` |
| the counter rules of `model.md` §8.1: the name form, strictly ascending by name | that counter's name, that counter | `sections.set.counters[i].name`, `sections.set.counters[i]` |
| `state` is `refused` if there is a refusal of a subject, `no_observer`, `rule_conflict`, or no edit; a refused set has a refusal | the `state` field | `sections.set.state` |

Not checked by the format: the order of `unparsed` and of a `first`
list (plan order, which needs the plan), the values of `observers` and
`votes` beyond the rows above (which need the shards), and that
`groups` is empty under an `indivisible` request (which needs the
request).

That copies are well-founded and every edit effective (§5.5) is not
checked by the format: apply checks both and refuses a set that breaks
either (§7.1).

### 6.2 Declared changes of an edit set

Reduce derives `Declared` (`verify.md` §2) from an accepted edit set. A
refused one has none.

- **`rules`**: the union of
  - the `renamed` rules of every shard that is `ok` and has no failure,
    which are also the set's `rules`; and
  - the `rule` of every selected subject and follower that has one.

  Two rules with one `(parent, kind, name, disc)` and different targets,
  or two rules of a kind other than `namespace` with one target
  `(to_parent, kind, to_name, disc)` and different `(parent, name)`,
  refuse the edit set with `rule_conflict`: two observers disagree about
  what an entity is renamed to. (Namespaces may merge, `verify.md` §2.1.)
- **`changed`**: the keys of the `changed` entries that are in force: an
  edit of the set shares a byte with the entry's place.
- **`created`, `removed`**: empty.

That `Declared` is derived by untrusted code is the design: a rule that is
wrong or missing cannot make two different programs compare equal
(`verify.md` §2.1). An exemption can: `changed` withdraws a unit from the
comparison. The edit set lists every entry with its place, its reason and
whether it is in force; the report prints them; §9.4 says which units a
rename exempts and why; and a spelled piece inside an exempt unit is
still checked (`verify.md` §10).

### 6.3 Counters

In `EditSet.counters`:

| Counter | Meaning |
|---|---|
| `reduce.shards` | `ok` shards read |
| `reduce.unparsed` | selected observers without one |
| `reduce.failed_runs` | `ok` shards with a failure |
| `reduce.claims` | claims read |
| `reduce.claims.replace`, `.keep`, `.refuse`, `.unresolved` | by stance |
| `reduce.sites` | distinct sites |
| `reduce.sites.live`, `.kept`, `.unresolved`, `.rejected` | by outcome (§5.4) |
| `reduce.subjects`, `reduce.subjects.selected` | joined subjects; selected ones, followers included |
| `reduce.groups`, `reduce.groups.rejected` | atomic groups (§5.5), edits tied to nothing included |
| `reduce.edits` | edits in the set |
| `reduce.edits.shared` | edits claimed by more than one observer |
| `reduce.edits.max_observers` | the largest `observers` of an edit |
| `reduce.edits.nested` | edits that are not outermost |
| `reduce.files` | files with at least one edit |
| `reduce.reports.note`, `.review`, `.error` | joined reports by severity |
| `reduce.changed`, `reduce.changed.in_force` | joined `changed` entries |
| `reduce.rules` | rules declared (§6.2) |
| `reduce.runs` | temporary files written (§5.6) |

Reduce also sums, over the `ok` shards, each shard counter whose name
starts with `api.` or `edits.`, and writes the sum under the same name;
for `api.peak_bytes` it writes the largest. A shard that lacks a counter
adds nothing to it.

Map shards that carry a transform add to their `counters`:
`edits.claims`, `edits.claims.replace`, `edits.claims.keep`,
`edits.claims.refuse`, `edits.claims.unresolved`, `edits.subjects`,
`edits.links`, `edits.groups`, `edits.reports`, `edits.changed`,
`edits.files`; and a programmable transform's run adds those of `api.md`
§7.6 rule 6.

## 7. Apply

### 7.1 The overlay

`slip_apply` turns an accepted edit set into an **overlay**: for each file
with an edit, the post-image bytes, and for each spelled piece, where its
text landed.

**What the edits mean.** The edits of one file are disjoint or nested
(§5.4). For a range `r` of a file, the **text of `r`** is the bytes of
`r` in which each edit that is properly inside `r`, and within no other
edit properly inside `r`, is replaced by its **expansion**. The expansion
of an edit is its pieces in order: a literal's bytes; a spelled piece's
`text`; for a copy, the text of the copied range. The **post-image** of a
file is its bytes in which each outermost edit (§5.5) is replaced by its
expansion.

So an edit properly inside a copied range is applied to the copy; an
edit of exactly the copied range is not, which is what lets two ranges be
exchanged; and an edit inside another edit's range is not applied at its
own place: it appears wherever a copy carries it.

1. Read every file that has an edit or is copied from, at its real path.
   If one cannot be read, or its size or content digest is not the
   `FileRef`'s, apply fails with `stale_file` naming the path. Nothing is
   published.
2. Check the set: the invariants of §6.1; no edit draws on itself
   through any chain (§5.4); every edit is effective (§5.5). A set that
   fails is `Invalid`: reduce made none such, and an edit set given by
   hand (§10.2) is not trusted to be one.
3. Compute each file's post-image as above. Bytes outside the outermost
   edits are copied. No byte is added, removed or recoded: a byte-order
   mark, the line endings, a missing final newline and invalid UTF-8 all
   pass through. The length of every expansion is computed before any is
   built; a post-image of more than 2^31 bytes fails apply with
   `too_large` naming the path (copies can multiply text). Expansions are
   built in an order in which each is built after those it draws on, with
   no recursion on the call stack.
4. Each post-image is published as a **blob**: a file of exactly those
   bytes, named by their content digest (§10.4).
5. **Obligations.** Each time the text of a spelled piece is written into
   a post-image, at its edit's own place or in a copy that carries the
   edit, that is one obligation: the file, the byte range of the text in
   the post-image, and the piece's `key` (`api.md` §7.6 rule 4). The same
   piece carried to three places gives three.

```
Overlay := edits:     digest                  the edit set id
           files:     list<OverlayFile>       sorted by path, no two equal; not empty
           spellings: list<Obligation>        verify.md §10.1; sorted by (file, lo, hi, key), no two equal

OverlayFile := path: path                     real path: canonical, not `/` (§2.1)
               pre:  Content
               post: Content

Content := size: uv, content: digest
```

Kind `OVLY`, version 1, deterministic, one required section `overlay`
(id 1). The **overlay id** is the container's content id. Invariants,
checked in this order, each over its whole list: `files` not empty;
every `path` canonical (`transform::is_canonical`); `files` strictly
ascending by path; `spellings` strictly ascending; each `file` an index
of `files`; `lo < hi <= post.size`.

An overlay replaces the content of existing files. It creates and removes
none (later versions). A file whose edits leave its bytes as they were is
an entry all the same, with `post` equal to `pre`: an obligation may name
it. The patch and the write skip such a file.

**Parsing through an overlay.** The front end is given the overlay's
files and blobs ([KF2](../docs/requests/KF2-overlay-and-file-identity.md)).
Every read of a file the overlay names yields the post-image bytes, by
whatever spelling the file is reached; every other read goes to the file
system. The post-image has the pre-image's paths, so a subject path
(`digest.md` §11) is the file's real path in both images. The verify job
through an overlay is `verify.md` §5.1, "The overlay form".

### 7.2 The patch

`--patch <file>` writes the overlay as a unified diff, a function of the
overlay and the pre-image bytes:

- One file section per overlay file whose `post` differs from its `pre`,
  in path order: `--- <path>` and
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

1. **Check.** For every overlay file whose `post` differs from its `pre`
   (the others take no part in the write): it is owned (§2.1), it is a regular
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
pub enum ApplyError { StaleFile(Vec<u8>), TooLarge(Vec<u8>), NotAccepted, Invalid(String), Io(Vec<u8>, String) }

/// Disjoint replacements of one buffer, ascending: the last step of §7.1 for one file.
pub fn splice(pre: &[u8], edits: &[(u64, u64, &[u8])]) -> Result<Vec<u8>, ApplyError>;
/// §7.1 steps 2 and 3 without the files' bytes: for each edit of the set, the length of its
/// expansion, or `Invalid` (a cycle, an edit that is not effective) or `TooLarge`.
pub fn plan(set: &EditSet) -> Result<Vec<u64>, ApplyError>;
pub fn build(set: &EditSet, set_id: Digest, files: &dyn Files) -> Result<Built, ApplyError>;
pub fn patch(overlay: &Overlay, pre: &dyn Files, post: &dyn Fn(Digest) -> std::io::Result<Vec<u8>>,
             relative_to: Option<&[u8]>, out: &mut dyn std::io::Write) -> Result<(), ApplyError>;
```

`build` holds, at a time, one file's pre- and post-image and the
pre-images of the files that file's edits copy from, when given a sink
instead of collecting (`build_into`, same contract). An edit set without
copies costs what a splice costs.

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
writes `shards/<plan>/map.<r>/`, the verify phase `verify.<r>/`. `r` is one
more than the greatest round the plan has taken, and at least 1: a
round is taken by a shard directory of either phase or a transform run
record (§10.6; `exec.md` §2 says which names count). Round 0 is never a
transform's: it is `slip parse`'s, and that of the pre shards of `slip
verify`. A round number is not reused while anything of a later round
remains, and `slip verify` does not take a round of a transform's run
(`exec.md` §8.7). (A round is one application of edits, `verify.md` §3; a
run that applies edits more than once is M4.)

### 8.1 Steps

| Step | Does | Parallel |
|---|---|---|
| map | per observer: parse, digest (`digest.md`), run the transform (§9, or a run of `api.md` §2.4); shard with `inputs`, `digests`, `edits` | yes |
| reduce | §5: the edit set and the declared changes | first level |
| apply | §7.1: the overlay and its obligations | no |
| verify | per observer: parse through the overlay with the declared rules, compare, count rule use, check spellings (`verify.md` §5, §9, §10) | yes |
| scan | §8.4: residue | per file |
| conclude | §8.5: the result | no |

The digests of a map shard are the pre-image's and do not depend on the
transform: the walk runs first and its output is complete before the
transform is entered.

A refused edit set ends the run after reduce: there is no overlay, no
verify phase, and the result's state is `refused`.

### 8.2 Which observers are parsed again

The verify phase **selects** the run's selected observers whose map
shard of the round is `ok`; those are the observers `verify.md` §7 gives
a state. (An unparsed observer is counted by the result, §8.5, and has no
state there.) An observer whose run of the transform failed (§4.6) is
selected like any other.

Every one of them gets a verify job when the declared rules are not
empty. The post-image of an observer that read no overlay file is its
pre-image, and its own verdict is not in doubt; the job is there for the
agreement check (`verify.md` §9), which needs to know, from trusted code,
whether that observer has an entity a rule renames. With no rules, an
observer that read no overlay file is `unchanged` and gets no job
(`verify.md` §5.5).

An observer **read** an overlay file if the file's path is in the
`inputs` of its map shard.

("Open questions": a trusted filter of keys in the map shard would let
most observers that read no overlay file skip the job.)

### 8.3 The verify job

The job of `verify.md` §5 with an image of the overlay form. Its `post`
(`exec.md` §5.2) is

```
Post { pre: r, declared: <id of the DECL container>, image: overlay(<overlay id>) }
```

so the pre shard is the map shard of the run's own round, and the worker
reads the declared changes, the overlay and its blobs from the store.
`verify.md` says the rest: the loading of blobs and guards O1 and O2
(§5.1, §5.3), the `binding` and `rule_use` sections (§9.1), the
`spellings` section (§10.4). The phase's report is `verify.md` §7.1 with
the additions of §9.4 and §10.5.

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
Residue := category: Category, file: path, lo: uv, hi: uv, line: uv, column: uv
Category := dependent = 0 | not_compiled = 1 | not_planned = 2 | mention = 3
```

sorted by `(file, lo)`, no two equal in that key; `file` a canonical
real path (`transform::is_canonical`, not `/`); line and column are 1-based, the
column in bytes.

The scan is made for a request of kind `rename`. A script has no old
name to scan for: its result has no residue, and what its run did not
examine is counted by `api.unresolved_spelling_matches` (`api.md` §6.5
rule 6), which the edit set sums (§6.3).

### 8.5 The result

Kind `RSLT`, version 1, deterministic. One required section `result`
(id 1). The **result id** is the content id.

```
Result := scheme:    uv               1
          plan:      digest
          round:     uv
          transform: digest           the request id
          edits:     digest           the edit set id
          declared:  opt<digest>      content id of the DECL container
          overlay:   opt<digest>
          state:     State
          partial:   bool             the run selected fewer observers than the plan has
          allow_unparsed: bool        the run was given --allow-unparsed
          verdict:   opt<RunVerdict>  the verify phase's verdict (verify.md §7, §9.3); absent if the
                                      phase did not run or did not finish
          tally:     Tally
          agreement: list<Stale>      verify.md §9.2; sorted by (rule, observer)
          unused:    list<uv>         indices of rules no observer reached an entity through
          spellings: Spellings        verify.md §10.3
          residue:   list<Residue>
          exempt:    list<KeyId>      Declared.changed, repeated here for the report

Tally := selected: uv, unparsed: uv, failed_runs: uv, error_reports: uv,
         verified: uv, unchanged: uv, mismatch: uv, not_parsed: uv, unverifiable: uv,
         never_parsed: uv, failed: uv

RunVerdict := verified = 0 | not_verified = 1 | incomplete = 2

Stale := rule: uv, observer: digest, direct: uv

Spellings := obligations: uv, passed: uv
             failed: list<FailedSpelling>     in the obligations' order
FailedSpelling := obligation: uv              its index in the overlay's `spellings`
                  why:        denotes_other = 0 | unseen = 1
                  observers:  uv
                  first:      opt<digest>     absent for `unseen`
                  found:      opt<KeyId>
                  name:       str

State := verified = 0 | verified_with_residue = 1 | not_verified = 2 | incomplete = 3 | refused = 4
```

The seven states of `Tally` are those of `verify.md` §7, over the
observers the verify phase selected (§8.2); they add up to `selected`
less `unparsed`. `failed_runs` and `error_reports` are the counts of
`ok` shards with a failure and of joined reports of severity `error`
(§6). `verdict`, `agreement`, `unused` and `spellings` are what the
trusted run report computed (`slip_verify::run::Report`, `verify.md`
§9.5, §10.6); conclude copies them and decides none.

Invariants, checked in this order: `scheme` is 1; `agreement` strictly
ascending by `(rule, observer)`;
`unused` and `exempt` strictly ascending; `spellings.failed` strictly
ascending by `obligation`, and `passed` plus its length is `obligations`;
for each failed spelling, `observers` is 0 and `first` and `found` are
absent if it is `unseen`, and `observers` is not 0 and `first` is
present if it is `denotes_other`; every `residue` file canonical, then
`residue` strictly ascending by `(file, lo)`, then each entry's `lo <
hi`, `line` and `column` at least 1; `overlay`, then `declared`, absent
iff `state` is `refused`, and a `refused` result has no `verdict` and a
tally of 0 in every field but `selected`, `unparsed`, `failed_runs` and
`error_reports`; `unparsed` at most `selected`; with a `verdict`, the
seven states add up to `selected` less `unparsed`, and without one each
of them is 0 (no verify phase finished, so no observer has a state).
The format checks nothing else: not a residue's range against its
file, nor which state applies, which is conclude's (below). (The rows
on failed spellings, on a residue's range, line and column and on the
tally are `spec/landed/verify/WP-070.md` §1.)

The state is the first that applies:

1. `refused`: the edit set is refused.
2. `not_verified`: the verdict is `not_verified`: some observer is
   `mismatch` or `not_parsed`, or is stale for a rule, or some spelling
   obligation failed or was seen by no observer (`verify.md` §9.3).
3. `incomplete`: the verdict is `incomplete` or absent (the run was
   cancelled); or `unparsed` is not 0 and the run was not given
   `--allow-unparsed`.
4. `verified_with_residue`: everything that was applied is verified, and
   something that was asked or found was not done: the residue has an
   entry that is not a `mention`; or the edit set has a refusal (a group
   was rejected); or `failed_runs` or `error_reports` is not 0.
5. `verified`.

`--write` and `--patch` act only on `verified`, and on
`verified_with_residue` when every reason for it is permitted:
`--allow-residue` permits residue and refusals, `--allow-errors` permits
failed runs and `error` reports (`api.md` §2.8 rule 4, §7.4 rule 2).
Neither is implied by the other or by anything else: by default a run
with a failed run or an `error` report writes nothing, however good its
verification. They
never act on a `partial` result unless `--allow-partial` is given. Under
`--allow-unparsed` the main file of every unparsed observer is scanned as
a file no observer read.

A state other than `verified` withdraws nothing from the store: the
overlay and the shards stay, for the report and for diagnosis. **Repair**
(changing the edit set in response to findings and verifying again) is
M4; its hook is that a result names everything a second round needs.

## 9. Native rename

`slip::refactor::rename` at Level A
([01](../docs/design/01-transform-api.md) §6): implemented in Rust in
`slip_refactor`, configured by `Rename` (§3), run in the map phase after
the digest walk. It renames **functions and variables**. Its request is
`indivisible`: the rename is applied everywhere or nowhere.

### 9.1 The target

`target` is a qualified name from the global namespace: `a::b::f` is
`[a, b, f]`. An entity **matches** if its key (`digest.md` §3; read
with `Keys::key`, §9.7) has the name `identifier(f)`, its parent chain
to `root` is, innermost first, entities named `b` then `a` of kind
`namespace`, `class` or `class_template`, and its kind is allowed:

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
with the flag `unsupported`. So is an entity with C language linkage
that is declared with the target's name in the target's scope: its key
has the parent `c_linkage` whatever namespace declares it (`digest.md`
§3), so it never matches, and it is recognised by the scopes that
enclose its declaration.

### 9.2 Subjects

- **Seeds**: §9.1.
- **Candidates**: every virtual member function whose name is the old
  identifier, in every observer, whatever class it is in. With each, a
  **link** to every function it overrides. Overriding ties functions
  across classes that no single observer need see together; reduce closes
  the seeds under the links (§5.2), so a rename of one virtual function
  renames its whole family and no other.
- **Followers**, each with `of` the subjects it follows:
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

The `rule` of a subject of any role is the rule under which the entity,
declared with the new identifier in the same scope, has the key it has
now: `Keys::rename_rule(entity, new_name)`. For the one follower whose
key does not hold the identifier as its name (an unnamed class's name is
`unnamed_by` the variable's), the rule is made from the entity's `Key`
(`Keys::key`, §9.7) with the inner name replaced: found as the key with
`unnamed_by(new_name)`, to be called by the name it has now. A follower
for which `Keys::key` refuses has the flag `unsupported` (§9.5).

### 9.3 Written references

For each subject of the observer, rename claims `replace` at every token
that is the subject's name. Where the name is found, and which piece the
replacement is:

| Reference | Found from | Piece |
|---|---|---|
| Every declaration and the definition of the entity: its declarator-id's last name. Redeclarations in other files, friend declarations, block-scope `extern` declarations, out-of-line definitions (`void N::f() {}`: `f` only), a template's declarations, explicit specialisations and instantiations | Kiln's declaration extents (`kiln/spec/ast.md` §19) | literal |
| An id-expression: a call, `&f`, `&C::f`, a use of a variable; in evaluated and unevaluated operands, default arguments, default member initialisers, template arguments. The function overload resolution selected is the subject; a name that selects a non-subject overload of another scope is a `keep` | the written-reference table, role expression name | spelled |
| A class member access `a.f`, `p->f`, `a.C::f` | role member name | spelled |
| A qualified name: only the last component is the name. Qualifiers are not edited: a rename moves nothing | the same entries | spelled |
| A template name with arguments: `f<int>(x)`, `v<int>` | role template name | spelled |
| A using-declaration's name. The using-declaration is a follower (§9.2) | declaration extents | literal |
| An overriding function. It is a subject by a link (§9.2) | as the first row | literal |
| A non-dependent name in a template pattern, resolved at the definition | the table | spelled |
| A dependent name in a template pattern | §9.4 | literal |
| A name in a macro argument (tier 1). Each substitution votes | as its row | as its row |
| A name in a macro body, pasted, or crossing (tiers 2, 3, X) | `slip_kiln::sites` | none: `refuse` |

**The pieces.** A *literal* replacement is `literal(new_name)`. A
*spelled* one is `spelled(K, new_name)`, with `K` the key id, with no
rules, of the origin of the entry's referent (`api.md` §5.5: for a
specialisation or a member of an instantiated class, the template or the
member of the template), when every derivation of the token in this
observer has that one origin; a token whose derivations have different
origins, all subjects, is written as a literal. `K` is then a subject of
the shard.

Why two kinds. A spelled piece is checked directly by the trusted base:
where it lands, the post-image must resolve the name to `K`
(`verify.md` §10). That check does not depend on the unit that holds the
name being compared, so it also covers a reference inside a unit that
§9.4 exempts. A declaration's own name is not a reference the front end
records, so an obligation there could never be seen; what checks it is
the entity's rule: a declaration left under the old name, or given
another, is an entity whose key does not line up (`removed` and `added`,
or `stale`, `verify.md` §9). The same holds for a using-declaration's
name until Kiln records it as a reference.

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
| the unqualified callee of a dependent call | the declarations `S` found at the definition | `S` not empty and all subjects: `replace`. `S` not empty and none a subject: `keep`. Mixed: `refuse(mixed_set)`. `S` empty: next row |
| any other dependent name (`T::f`, `x.f`, `x->f`, a call found only by argument-dependent lookup) | per instantiated specialisation, the entity the name resolved to | every one a subject: `replace`. None a subject: `keep`. Mixed: `refuse(mixed_instances)`. Not instantiated in this observer, or the front end has no such record: `unresolved` |

A `replace` here has a literal piece, and with it the shard has one
`changed` entry (§4.5): `key` the unit whose stream holds the name (the
innermost enclosing unit of the pattern, `Keys::unit_of`), `place` the
name token, `reason` `dependent name <old name>`. Reduce puts the key in
`Declared.changed` when the edit is accepted (§6.2). A holder that has no
key to exempt (it is keyed by content, or its key is indeterminate) gets
no entry: the edit is claimed, and verification reports the unit.

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
  makes, and every other unit; what is still checked inside the holder is
  every spelled piece (§9.3). An error in a dependent name of an exempt
  pattern that no observer instantiates is not detected. The report lists
  every exempt unit with the place and the reason.

### 9.5 Refusals

Rename sets a subject's flags (§4.1), which refuse the run (§5.2):

| Flag | When |
|---|---|
| `read_only` | a declaration of the entity is in a read-only file. A function that overrides one of a library is refused this way, through its link |
| `unsupported` | another kind (§9.1); a constructor, destructor, operator, conversion function, literal operator or deduction guide reached through a link; an entity with C language linkage; `main`; a member of a class template, until the package that adds followers of specialisations lands; a follower whose rule cannot be made (§9.2) |
| `name_taken` | the scope that declares the entity (for a namespace, every part of it the observer sees; for a class, the class and its bases) declares an entity named `new_name` |
| `unreachable` | the subject is referenced where the front end records no name token: an attribute argument, a designator |

and claims `refuse` (§4.2) at a reference it found and cannot edit (§2.3).

Not refused, and left to verification: a use that would resolve to
another entity once renamed (the new name hides, or is hidden by, a
declaration in another scope), which a spelled piece reports as
`denotes_other` and a digest as a difference; a reference without a token
(§9.3); a string that names the entity for the linker (`alias`, `ifunc`:
"Open questions").

### 9.6 What a rename declares

By §6.2: the rule of every selected subject and follower, and `changed`
for the holders of the dependent names that were edited. `renamed` is
empty (every rule of a rename is a subject's, in force only if the
subject is selected); nothing is `created` or `removed`; there is no
group and no report.

The claim verified is that of `verify.md` §1: in every selected observer
the post-image is the same program as the pre-image except that the
entities the rules name have the new name, and except for the exempt
units; no observer still has one of those entities under its old name
(`verify.md` §9); and every reference written as a spelled piece denotes
the entity it was written for (`verify.md` §10).

### 9.7 Interface

`slip_refactor` (depends on `slip_model`, `slip_kiln`, `slip_digest`; it
may name `slip_kiln::kiln`, `spec/harness.md` §3):

```rust
pub fn run(transform: &Transform, unit: &slip_kiln::Unit<'_>, keys: &mut slip_digest::query::Keys<'_>,
           files: &dyn Fn(kiln::source::FileId) -> Option<FileRef>) -> Result<Edits, String>;
```

It reads the subject through four things and nothing else of the front
end's tables:

| From | What |
|---|---|
| `slip_digest::query::Keys` (`docs/wp/WP-087.md`) | key ids and keys with no rules, the unit that holds a declaration, `rename_rule` |
| `slip_kiln::written` (`docs/wp/WP-067.md`) | the written references of the unit, deduplicated, with role, name span and referent; the names of declarations from the declaration extents |
| `slip_kiln::ranges` (`docs/wp/WP-084.md`) | the file range of a name's span, or none: Kiln's `editable_file_range` (§2.2), which needs Kiln at or after `913a490bb` |
| `slip_kiln::sites` (`docs/wp/WP-068.md`) | why there is none, and where a person must look (§2.3) |

`run` is a function of the unit: two parses of the same inputs give equal
`Edits`.

## 10. Commands

The usage lines are in `exec.md` §8; this section says what the commands
do. An `<id>` is a prefix of 4 to 64 hexadecimal digits that matches
exactly one file of its kind in the store, as for `--plan` (`exec.md`
§8.2).

### 10.1 `map` and `rename`

`slip map` runs the map phase of a transform:

1. Opens the store exclusively; builds or finds the plan as `parse` does.
2. Builds the transform request (§3) and publishes it.
   - `--from`, `--to`, `--kind`: a rename. `--from` is split at `::` (a
     leading `::` is allowed and ignored).
   - `--transform <file>` with `--transform-arg <option>` repeated: a
     script. The command compiles the transform unit once, stops with the
     error of `api.md` §12 if it does not compile, and puts its
     `TransformId` in the request (`docs/wp/WP-092.md`). `--indivisible`
     sets the request's flag. The budgets are the defaults of `api.md`
     §2.5.
   - `--owned` and `--readonly` are made real paths.
3. Takes a new round (§8) and runs the phase of `exec.md` §5 with jobs
   that carry the request's id (`Job.transform`).
4. Publishes the transform run record (§10.6) and prints the first lines
   of §10.5.

`slip rename` is the whole run of a native rename: steps 1 to 3 with a
rename request, then `reduce`, then `check`, then, with `--write` or
`--patch` and a state that permits it (§8.5), what `apply` does. It
publishes one transform run record for all of it and prints the report
(§10.5). `rename` without `--write` or `--patch` changes nothing outside
the store.

The whole run of a script as one command is M3's
(`docs/demos/demo-3.md`); until it exists a script is run as `map`,
`reduce`, `check`, `apply`.

### 10.2 `reduce`, `check`, `apply`

- `reduce` runs §5 over the map shards of a round, with the request the
  shards name, and publishes the edit set and the declared changes. It
  prints `edits <id>`, `state <state>`, and `declared <id>` if there is
  one.
- `check` takes an edit set, which need not be the one reduce made, builds
  the overlay, runs the verify phase (§8.2, §8.3), scans and concludes.
  `--declared` defaults to the declared changes of the edit set (§6.2),
  derived again. It is `rename` from the apply step on, and it is how a
  test gives the trusted base an edit set that is wrong on purpose
  (`harness.md` §6.4). The edit set's `plan`, `round` and `transform` must
  be those of the round's map shards. `--report <file>` writes the verify
  phase's report (`verify.md` §7.1, §9.4, §10.5); `--findings` and
  `--locate` are `verify`'s (`exec.md` §8.7).
- `apply --result` performs §7.2 or §7.3 for a published result.
- `apply --recover` is §7.3's recovery. It is the only command that runs
  on a store with a journal.

`slip parse` and `slip verify` are not part of a transform's run: a map
shard made by `parse` has no `edits` section, and `verify` (`verify.md`
§5.1, the tree form) checks a post-image that someone else made.

### 10.3 Protocol

The messages are `exec.md` §5.2, protocol 2: a map job of a transform
carries `transform`, the request id; a verify job carries `post` (§8.3).
A worker reads the request, the overlay, its blobs and the declared
changes from the store, which it opens shared. How a verify job loads an
overlay's blobs is `verify.md` §5.1, "Loading".

### 10.4 The store

Added to `exec.md` §2 (`declared/` is there since M1):

```
<store>/
  transforms/<request id>.trfm
  edits/<edit set id>.edts
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
transform <request id>
round <n>
map: selected <n>, ok <n>, not ok <n>, failed runs <n>
edits <id>
state <accepted|refused>
edit <n> in <n> file(s); shared <n>; most observers <n>; nested <n>
groups <n>, rejected <n>                  omitted for an indivisible request
kept <n>; unresolved <n>
reports: note <n>, review <n>, error <n>  omitted when all are 0
refused <cause> <n>                       one line per Cause with a non-zero count, in its order
declared <id>: rules <n>, exempt <n>
overlay <id>: files <n>, spellings <n>
verify: verified <n>, unchanged <n>, mismatch <n>, not_parsed <n>, unverifiable <n>, never_parsed <n>, failed <n>
agreement: stale <n>, unused rules <n>
spellings: passed <n>, failed <n>
residue: dependent <n>, not_compiled <n>, not_planned <n>; mentions <n>
result <id>
state <State>
written <n> file(s)                       or: patch <path>
```

`map` prints the first three lines. Lines for steps that did not run are
omitted. Then the details, each list in the order of its record, paths
relative to `--relative-to` when given:

```
refusal <cause>[ <reason>]: <path>:<line>:<column>: <display of the subject>[ in macro <name>] (<n> observer(s), first <main file>)
refusal <cause>: <display of the subject> (<n> observer(s), first <main file>)
report <severity>: [<path>:<line>:<column>: ]<message> (<n> observer(s), first <main file>)
failed run <kind>: <message> (<n> observer(s), first <main file>)
finding <kind> <name>: <n> observer(s), first <main file>[: <path>:<line>:<column>]
stale <display>: still declared under its old name in <main file>
spelling <why>: <path>:<line>:<column>: meant <key, first 16 digits>[, found <name>] (<n> observer(s))
exempt <key, first 16 digits> <reason>: <path>:<line>:<column>[ (not in force)]
residue <category>: <path>:<line>:<column>
```

`finding` lines are the run report of `verify.md` §7, at most 64 groups.
Of each other kind at most 200 lines are printed; a list that is cut is
followed by `<kind>: <n> more (slip dump <file of the record>)`. The
records hold every entry. Lines and columns are computed by the command,
from the pre-image file, and for a `spelling` line from the post-image
blob; they are not in the records. A place a report was only attached to
(§4.5) is followed by ` (nearest place)`. A report's message and a
declaration's reason are bytes (§4.5): they are printed with every
invalid UTF-8 sequence replaced by U+FFFD and every control character
below U+0020 by a space, so that a line stays a line; the record keeps
the bytes.

Exit status (`exec.md` §8.5):

| Status | `rename`, `check` | `map` | `reduce` | `apply` |
|---|---|---|---|---|
| 0 | state `verified`, and what `--write` or `--patch` asked was done | every selected observer's shard is `ok` and no run failed | `accepted` | done |
| 1 | `not_verified`, `verified_with_residue` or `refused`; or a state that does not permit the write that was asked | a shard is not `ok`, or a run failed | `refused` | the result does not permit it; a refused write (§7.3 step 1 or 4) |
| 2 | usage | usage | usage | usage |
| 3 | input error: as `parse`; a shard without the request; a stale file; a store with a journal; a transform that does not compile | the same | the same | the same; `--recover` with a file it had to report |
| 4 | `incomplete` | — | — | — |

A `verified_with_residue` result written under the options that permit
it exits with status 1: the command did what was asked and has something
to report. `map`'s status is `parse`'s (`exec.md` §8.5), with a failed
run counted like a shard that is not `ok`; `reduce` runs after either.

### 10.6 The transform run record

Kind `RUNT`, version 1, not deterministic. One per `map`, `rename` or
`check` invocation, published when the command ends, also when it is
interrupted after the map phase began. One required section `run` (id 1):

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
          | load = 8                           compiling the transform unit in the coordinator
```

The phases' own run records (`model.md` §9) are written as before, with
their `phase` and `round`. The harness reads wall time and memory of a
transform from this record and from the two it names
(`harness.md` §6.5). A round is taken from the moment its first record or
shard directory exists (§8).

## 11. What M2 leaves out

| Not in M2 | Where |
|---|---|
| Running a transform written in C++: the host, the subject over Kiln, the map task with a script | M3 (`api.md`, `docs/wp/WP-080.md` to `WP-096.md`). The formats and steps of this file already carry what it produces |
| Insertion (an empty range); created and removed files; inserted declarations; `created` and `removed` | later versions of `api.md` |
| Edits in macro bodies; accepting a changed stringised argument; coverage by byte range | M4 (`spec/macros.md`, KR4) |
| Requalifying a name that would resolve differently; spelling candidates | M4 |
| Repair rounds | M4 |
| Rename of types, fields, enumerators, namespaces, concepts, parameters and locals | after Kiln's written-reference stages 3 to 5 |

M2's own producer, the native rename, makes no copy piece, no nested
edit, no group and no report. Reduce and apply implement them in M2 all
the same, tested with shards written by hand, so that M3 adds a producer
and nothing under it.

## Tests

- `crates/slip_model/tests/edits.rs` — §3, §4: round trips of `TRFM` and
  of the `edits` section, binary and JSON, with every kind of piece, a
  group, a report with and without a place, a `changed` entry, a rule, a
  failure; every invariant of §4.7 with its offset and path.
- `crates/slip_model/tests/editset.rs` — §6, §7.1, §8.5, §10.6: round
  trips and invariants of `EDTS`, `OVLY` (with obligations), `RSLT`,
  `RUNT`.
- `crates/slip_reduce/tests/reduce.rs` — §5, §6.2 over shards written by
  hand: selection through links (a chain, a diamond, an unselected
  candidate); each refusal of §5.2; every row of §5.4's first table; file
  disagreement in `files`, in `inputs` and in a copied file; unparsed
  observers; a failed run; the declared changes of §6.2, with a `changed`
  entry in force and one whose edit was rejected; `rule_conflict` both
  ways and the namespace exception; `no_edit`.
- `crates/slip_reduce/tests/pieces.rs` — §5.3, §5.4, §5.5: strict
  identity (the same text as a literal, as a spelled piece, with another
  key: `divergent`); crossing edits; an edit crossing a copied range; an
  edit within, equal to, and around a copied range; the exchange of two
  ranges with an edit inside one; a wrap with an edit inside; `cyclic`
  with one edit and with two; `buried`, alone and repeated (an edit
  carried only by an edit that is itself rejected).
- `crates/slip_reduce/tests/groups.rs` — §4.4, §5.5: a group seen whole
  by one observer and in part by another; two groups joined by a third
  observer; a rejected member rejects the group and not its neighbours;
  an `indivisible` request; the `groups` list of the set.
- `crates/slip_reduce/tests/order.rs` — §1 rule 3, §5.3, §5.6: for
  generated claim sets (fixed seed), the edit set is byte-identical under
  every tested permutation of observer completion, every `fan_in` from 2
  to 64 and every thread count; one site claimed by ten thousand shards
  with `fan_in` 2 stays within a stated memory bound.
- `crates/slip_apply/tests/splice.rs` — §7.1: splicing at the start, the
  end, adjacent ranges; CRLF, a byte-order mark, no final newline, invalid
  UTF-8, an empty file; a stale file; an edit set that changes nothing
  (an entry with `post` equal to `pre`).
- `crates/slip_apply/tests/compose.rs` — §7.1: the text of a range with
  edits inside it; the exchange; the wrap; a copy from another file; one
  edit carried to three places and its three obligations; an obligation's
  range for a spelled piece after a copy of unknown length; a chain of
  copies a thousand deep (no stack overflow); a doubling chain that is
  `too_large` without being built; a cycle and a buried edit given by
  hand are `Invalid`.
- `crates/slip_apply/tests/patch.rs` — §7.2: the patch applied by
  `patch(1)` gives the post-image (skipped where there is no `patch`).
- `crates/slip_apply/tests/write.rs` — §7.3: a failure injected at every
  operation of steps 1 to 5 leaves every file wholly pre or wholly post
  and `recover` returns the tree to the pre-image; a hard link, a
  read-only directory, a file changed between the checks; an entry with
  `post` equal to `pre` is not touched.
- `crates/slip_refactor/tests/rename/` — §9, one directory per case over
  a small tree of files (`slip_kiln::tree`; a snippet's buffer is not a
  file and has no range to claim): each row of §9.3 with its piece, each
  row of §9.4 with its `changed` entry, each flag of §9.5, the tiers of
  §2.2, votes (§4.3), a token whose two derivations have different
  origins.
- `tests/transform/rename/` — §8, §9, §10 through the executable
  (`harness.md` §6): the cases above end to end with their post-images,
  reports and exit statuses; the oracles of `harness.md` §6.4.
- `crates/slip_cli/tests/stages.rs` — §10.2 over a store of shards
  written by hand: `reduce` (ids, lines, statuses), `apply --result` with
  a result of each state and each `--allow` option, `apply --recover`, a
  store with a journal.
- `crates/slip_cli/tests/rename.rs` — §10: usage, the output lines, exit
  statuses including 4, `map` and `check` as stages, `rename` end to end.
- `crates/slip_cli/tests/tasks.rs` — §8.1, §8.3, §10.3: the map job of a
  transform and the verify job through an overlay, as tasks.
- `crates/slip_kiln/tests/sites.rs` — §2.2, §2.3: each tier, each reason
  and the order of reasons; the range of each kind of refusal.
- `crates/slip_kiln/tests/written.rs` — §9.7: the written references and
  the declaration names of a unit (`docs/wp/WP-067.md`).
- `crates/slip_kiln/tests/scan.rs` — §8.4: identifiers, mentions, raw
  strings, line splices, text in skipped groups.
- `crates/slip_kiln/tests/overlay.rs` — §7.1 "Parsing through an
  overlay": the main file, a header by two spellings, an edit that adds
  an `#include`; `inputs` shows the post-image digests.
- `crates/slip_exec/tests/objects.rs` — §8, §10.4: the store's new
  files, blobs and their check, the next round, the journal's refusal.
- `crates/slip_verify/tests/conclude.rs` — §8.5: every row of the state,
  in order, and what each state permits under each `--allow` option.
- `crates/slip_script/tests/shard.rs` — §4.8 (M3, `docs/wp/WP-092.md`):
  a run output to the section and back loses nothing.

## Open questions

- **What `api.md` lacks that a native producer has** (§4.8): a way to
  say `keep` (`api.md` §7.3 rule 7), to say `refuse` with a
  reason at a place the transform could not edit, and to tie entities
  together across observers (subjects, links, selection). A rename
  written against `api.md` version 0 is therefore weaker than the native
  one in exactly the cases reduce decides: a virtual family seen in
  parts, a name that one observer resolves to something else.
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
  crossed only its sites could be accepted, and an edit whose container
  was rejected for `buried` may or may not have been worth keeping. Not
  done: the order of rescue would have to be specified, and a rejected
  group is reported, not lost.
- **A whole run that produces no edit** is `refused` with `no_edit`
  (§5.5), and exits with status 1. For a script whose job is to report,
  not to edit, that is the wrong word.
- **Rules of a script are unconditional** (§4.5): `expect_renamed` has
  no place, so nothing ties it to an accepted edit. If its edits are
  rejected the run is not verified, which is safe and unhelpful.
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
  `TEST(Suite, f)` is reported beside real leftovers. A script has no
  residue scan at all (§8.4).
- **Ownership by pattern.** `Ownership` is prefixes. Globs and a manifest
  file are wanted for trees where generated and written files share
  directories.
- **Votes are not weighed.** One observer's `keep` against ten thousand
  `replace` rejects the site. That is the conservative reading of D5;
  whether a transform may ask for something else is open.
- **A partial selection** (`--only`) verifies a subset and can never
  show agreement for the rest. Whether `rename` should accept it at all
  outside tests.
- **Rounds are never reclaimed.** Each run adds two shard directories to
  the plan. A store command to drop rounds is wanted.
- **`first` holds 8 observers, `notes` 8 entries** and the default
  `fan_in` is 64; all three are guesses.
- **Budgets of a script** are the defaults of `api.md` §2.5; an option
  to change them, and where it is recorded beyond the request, wait for
  the first transform that needs it.
