# KR5 — Slip's review of Kiln's WP-3280 phase 0 design

Slip's answers to the design note "WP-3280 phase 0: function-name
strings carry the declarations they print" (Kiln's
`designs/wp3280-design.md` at `b4747a3`, base Kiln `43d85be59`), which
answers item 3 of the list `docs/wp/WP-112.md` sent. Status: **sent
2026-11-23**. Kiln owns the design; this says what Slip's client needs
of it and why.

Verdict: **go ahead with phase 1, stage A**, with the five requirements
of §3. The record's shape (16-byte entry, key `(lit, function)`, eager
recording, a side table of the `Ast`) is accepted as proposed. Stage B
(PCH and image carriage) has no Slip priority before KF5.

## 1. What Slip does with these entries

One client: **the names digest** (`spec/digest.md` §9, `spec/verify.md`
§4.4). A function-name string that a unit only passes on is not in the
unit's digest; a second, informational digest holds its bytes, and the
verifier notes each one that changed. Under the motivating edit (a root
namespace moved) every function that asserts has a
`__PRETTY_FUNCTION__` whose bytes change, so the report carries one
note per such function and a string that changed for another reason (a
signature respelled, an alias retargeted, a function that now prints a
different specialisation) is one line among tens of thousands.

With the entries, the names digest is fed, per string: the bytes
outside every entry as they are, and at each entry the **key of the
entity as the declared rules map it** in place of the bytes (a
qualified chain collapsed to its last entity, whose key holds its
scope). Then a string whose bytes changed only by the declared rename
or relocation has an equal names digest and no note, and a note means a
change the declaration does not explain. Nothing here enters the
verdict: a string the program reads at compile time is written by
value as today, and an entry that is missing or wrong can only add or
drop a note. So the client fails soft, and the requirements below are
about the notes being trustworthy, not about a wrong `verified`.

## 2. The eleven questions

1. **Specialisations: as proposed.** The entity is the declaration
   whose name was printed: the specialisation in scopes and canonical
   arguments, the template or the pattern record in types as written.
   Slip keys the specialisation and reaches the template through
   `spec_info`; that accessor must answer for every such entity
   (an explicit specialisation and a member of one included).
2. **Dependent names: no entity, as proposed.** The bytes of a
   `DEPENDENT` entry are fed as bytes, so a renamed member typedef
   behind `typename W<T>::type` gives a note: right for a name Kiln did
   not resolve. Not the resolved declaration in phase 1; keep a flag
   bit free for it.
3. **Synthesised spellings: one entry, no sub-ranges.** This is the
   entry Slip needs most after plain names: `(lambda at F:L:C)` moves
   with every line an edit adds above it, and with the entity (the
   closure class, the unnamed tag, the namespace) in place of the bytes
   the string is equal across the move. Requirement: `SYNTHESIZED`
   entries always have an entity.
4. **Constructors and destructors: as proposed** (the function, with
   `CLASS_NAME`).
5. **Through copies: pointers only is enough.** A value copied out of
   the string is a constant the program computed and read; Slip writes
   it by value and a rename that changes it is a finding, which is
   right. Per-byte provenance is not asked for.
6. **Contracts: no map.** Slip does not read a contract site's function
   string today. Keep P6 going through the funnel so the record
   `(lit, f)` exists; a client that has the site has its function and
   calls `get(lit, f)`. Drop `contracts`.
7. **A separate flag** (`ParseOptions::fn_names`). Slip links Kiln as a
   library and sets it for the map parse and the post parse.
8. **Prefixes built without the flag: no records and
   `unrecorded_prefix`**, readable through an accessor. No backfill: a
   record that is exact except sometimes is worse than none.
9. **Straddling offsets: clip, and say so.** A suffix of a name is
   still bytes a rename changes; an entry dropped silently would make
   them look like plain text. `visible()` yields a straddling entry
   clipped to the view with a mark that it is partial (at the start;
   the end is the client's, by its own length). Slip feeds a partial
   entry as bytes and reads `spans` and `offset` directly where it
   needs both ends.
10. **Keywords: no entries**, and no placeholder for `auto`: nothing an
    edit declares changes those bytes.
11. **Byte offsets of the stored string: yes.**

## 3. Requirements beyond the questions

- **R-a. An accessor at the pointer, not only at a whole value.** Slip
  walks a constant structurally (a `string_view` is a pointer and a
  length inside a class value; an `__impl` object holds the pointer in
  a field). It meets `PtrBase::StringLit { lit, origin }` plus an
  offset, not a `ConstValueId` that is the pointer. Wanted:
  `Ast::fn_name_of_string(lit, origin, offset) -> Option<FnNameView>`
  (or the same over whatever the pointer's frozen form is), with
  `fn_name_of_value` built on it.
- **R-b. When `None` is the answer, stated.** With the table on and no
  unrecorded prefix, the spec section lists exactly which handles have
  no record (outside a function; a `PredefinedExpr` of a pattern not
  yet instantiated; a literal the source spells; a content-shared
  string) so that Slip can treat any other `None` as a defect and fall
  back to bytes with a counter.
- **R-c. No name outside an entry: checked.** The self-check proves
  each entry's bytes are its entity's name. The converse is what a
  notes digest rests on: that no declaration's name is printed outside
  every entry (a printer path without a hook). Wanted in the
  `KILN_FN_NAMES_VERIFY` lane: the bytes outside all entries are
  tokenised, and every identifier-shaped token is in a closed list
  (keywords, builtin type names, `operator` inside a `Name` entry
  aside, `anonymous`/`lambda`/`at` only inside `SYNTHESIZED`). A token
  not in the list fails the lane. The list is in the spec section.
- **R-d. One entity id per entity.** A namespace entry names the same
  `DeclId` that §18's written-reference entries name for that
  namespace (the first declaration), and likewise for a redeclared
  class or function, so that Slip's path from an entity to its key is
  the one it has.
- **R-e. Deterministic order.** `iter()` and the dump are in an order
  that is a function of the source alone (insertion order or sorted;
  not hash order).

## 4. Not asked

Roles are accepted as designed; Slip reads `QUALIFIED`, `depth`,
`SYNTHESIZED`, `DEPENDENT` and `CLASS_NAME` and ignores the rest
today. The `-fclang-compat` behaviour is Kiln's.
