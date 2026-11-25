# model — data model and on-disk formats

Status: draft (M0). Implemented by `slip_model`. This spec defines everything
Slip's phases exchange and store. `slip_model` depends on no other crate and
does no I/O beyond reading and writing byte buffers handed to it.

Sections 2–5 are foundations used by every format. Sections 6–9 are the M0
formats. Coverage, edits, facts and reports are added by later milestones
as new sections of the shard (§8), with ids already reserved; digests and
verdicts are already there (§8.2, §8.3).

## 1. Principles

1. **One encoding per value.** Every value has exactly one valid byte
   encoding. A reader must reject any other encoding of the same value
   (`Malformed`). Byte equality is therefore value equality, and the hash of
   an encoding is the identity of the value.
2. **Deterministic content.** A file whose content is specified as
   deterministic must be byte-identical when produced twice from the same
   inputs by the same tool version, on any host. Measurements (time, memory)
   never appear in such a file; they live in the run record (§9).
3. **Nothing is trusted.** Every reader validates everything it reads:
   lengths against the remaining input, enumeration values, ordering rules,
   UTF-8, the integrity trailer. A reader must never allocate an amount taken
   from the input without first checking it against the bytes remaining.
4. **Evolution by sections and versions.** Records have fixed fields. A format
   evolves by adding sections (§5) or by a new format version; a field is
   never added to an existing record within a version.

## 2. Primitive encodings

| Name | Encoding |
|---|---|
| `u8` | One byte. |
| `bool` | `u8`, 0 or 1. Any other value is `Malformed`. |
| `u32le` | Four bytes, little-endian. Used only in the container header. |
| `uv` | Unsigned LEB128 of a value in 0..2^64. Seven value bits per byte, least significant group first, bit 7 set on every byte but the last. At most ten bytes; the tenth byte must be 0x00 or 0x01. Must be minimal: the last byte is not 0x00 unless it is the only byte. |
| `sv` | A signed 64-bit value `n`, as `uv` of `(n << 1) ^ (n >> 63)` (zigzag). |
| `bytes` | `uv` length, then that many bytes. |
| `str` | `bytes` that must be valid UTF-8. |
| `digest` | 32 bytes (§3). |
| `opt<T>` | `u8` 0 (absent), or `u8` 1 followed by `T`. |
| `list<T>` | `uv` count, then that many `T`. |
| enumeration | `uv` holding one of the listed values. An unlisted value is `Malformed`. |
| tagged union | Written `T := a = 0: X \| b = 1 \| c = 2: Y, Z`: a `uv` tag holding one of the listed values, then the fields after that alternative's colon, as a record (nothing for an alternative without a colon). The tag is a `uv` like any other (a non-minimal one is `Malformed`); an unlisted tag is `Malformed`. `Parent` (`digest.md` §3) and `Piece` (`edits.md` §4.2) are tagged unions. |
| record | Its fields in the order listed, with no padding and no terminator. |

A list described as *sorted* must be in strictly ascending order of the stated
key (so it has no duplicates); a reader rejects any other order. Byte strings
compare lexicographically as unsigned bytes, a proper prefix first. Digests
compare as 32-byte strings.

### 2.1 Errors

Every decoding function returns one of these, with the byte offset at which
it was detected:

| Error | When |
|---|---|
| `Truncated` | The input ends inside a value. |
| `Malformed` | A rule of this spec is violated (non-minimal `uv`, bad `bool`, unlisted enumeration value, invalid UTF-8 in `str`, wrong order in a sorted list, bytes after the end of a file or payload). |
| `TooLarge` | A length or count exceeds the bytes remaining. |
| `BadMagic` | The container magic or kind is not the expected one. |
| `UnsupportedVersion` | The container version is greater than the reader supports, or zero; for a shard, 1 (§8). |
| `UnknownSection` | A critical section id the reader does not know (§5). |
| `Corrupt` | The integrity trailer does not match. |

A list's count is `TooLarge` when it is greater than the bytes remaining
after it. This bound relies on every list element encoding to at least one
byte, which holds for every type of this section.

### 2.2 Error offsets

The offset an error carries is, by error:

| Error | Offset |
|---|---|
| `Truncated` in a value | where the input ended (the length of the input decoded) |
| `Truncated` on open (§5 rule 5) | the length of the file |
| `Malformed`, non-minimal `uv` | the redundant last byte (0x00) |
| `Malformed`, `uv` over ten bytes or tenth byte above 1 | the tenth byte |
| `Malformed`, `bool` | the `bool` byte |
| `Malformed`, `str` not UTF-8 | the first byte that is not part of a valid sequence |
| `Malformed`, bytes left at the end of a payload or after `end` | the first leftover byte |
| `Malformed`, section id not ascending (§5 rule 1) | the first byte of that section's id |
| `Malformed`, unlisted enumeration value | the value's first byte |
| `Malformed`, unlisted tag of a tagged union | the tag's first byte |
| `Malformed`, `opt` tag other than 0 or 1 | the tag |
| `TooLarge` | the first byte of the length or count |
| `BadMagic` | the first byte of magic or kind that differs from the expected one |
| `UnsupportedVersion` | 12, the first byte of the version |
| `UnknownSection` | the first byte of that section's id |
| `Corrupt` | the first byte of the trailer (file length − 32) |

Offsets into a container are offsets into the whole file: a section's payload
decoder (`Reader::decoder`, or `Decoder::with_base`) reports file offsets.
The offsets of format invariants are in §7.1, §8.1, §8.2 and §8.3; that of
a missing required section in §5.4.

## 3. The hash

Slip uses one hash function everywhere: **BLAKE3** in its default hash mode
with 32 bytes of output. A `digest` is those 32 bytes. In text (JSON, logs,
file names) it is 64 lower-case hexadecimal digits.

Two ways of using it:

- **Content digest** of a byte string: `BLAKE3(bytes)`. File contents and the
  container trailer use this, so that standard tools (`b3sum`) reproduce
  them.
- **Tagged digest** of a structured value: `H(tag, v) = BLAKE3(tag ‖ 0x00 ‖
  encode(v))`, where `tag` is one of the ASCII strings registered below and
  `encode` is the encoding of §2. Tags keep identities of different kinds
  from colliding.

| Tag | Value hashed | Defined in |
|---|---|---|
| `slip.observer.v1` | `Command` | §7 |
| `slip.key.v1` | an entity's `Key` | `digest.md` §3 |
| `slip.unit.v1` | a unit's stream | `digest.md` §4 |
| `slip.type.v1` | a type's stream | `digest.md` §4.2 |
| `slip.value.v1` | a constant's stream, where a pointer names the constant | `digest.md` §7 |
| `slip.group.v1` | `list<Unit>` | `digest.md` §11 |
| `slip.tu.v1` | `list<GroupSum>` | `digest.md` §11 |
| `slip.rules.v1` | `list<Rule>` | `verify.md` §2 |
| `slip.basis.v1` | `list<Basis>`: the shards an edit set was reduced from | `edits.md` §6 |
| `slip.transform.v0` | a transform's identity: API level, command line, target, files read | `api.md` §2.2 |

`slip_model` implements BLAKE3 itself, in safe Rust, with an incremental
interface (`new`, `update(&[u8])`, `finalize() -> Digest`) and a one-shot
function, and `tagged_digest(tag, encoded)` for `H`; a tag that is not
ASCII or contains a NUL byte is a programming error and panics. It must
pass the official BLAKE3 test vectors for the hash mode
(inputs of every length in the published vector set). Keyed and
key-derivation modes and extended output are not needed. The implementation
may be replaced by a faster one at any time; the function may not change
within a format version.

## 4. JSON

`slip_model::json` is a small JSON reader and writer used for compilation
databases (`spec/cdb.md`), for the human-readable rendering of every format
(§10) and by the harness.

**Value model.** `Null`, `Bool`, `Int` (an integer in −2^63..2^64), `Float`
(any other number, as `f64`), `Str`, `Array`, `Object`. An object is an
ordered list of (key, value) pairs; order is preserved on read and write.

**Reader.** Accepts RFC 8259 JSON encoded as UTF-8, with an optional UTF-8
byte-order mark. It must reject, with the byte offset and a message: invalid
UTF-8; anything RFC 8259 does not allow (comments, trailing commas, single
quotes, leading zeros, bare control characters in strings); an unpaired
UTF-16 surrogate escape; a duplicate key in one object; nesting deeper than
128; and anything but whitespace after the top-level value. A number written
without fraction or exponent that fits `Int` is an `Int`; every other number
is a `Float`.

**Writer.** Two forms, both deterministic:

- *compact*: no whitespace;
- *pretty*: two-space indentation, one member or element per line, `": "`
  after keys, empty arrays and objects as `[]` and `{}`, a single `\n` at the
  end.

Strings are written with `\"`, `\\`, `\n`, `\r`, `\t`, `\b`, `\f` and
`\u00XX` for other characters below U+0020; every other character is written
as itself in UTF-8 (no `\u` escapes for non-ASCII, `/` not escaped). `Int` is
written in decimal. `Float` is written with Rust's shortest round-trip
formatting; a non-finite `Float` cannot be written (an error, not `null`).

Reading the writer's output must give back an equal value.

### 4.1 Reader details

- **Offsets.** `Error.offset` is a byte offset into the input as given,
  byte-order mark included. `line` is 1 plus the number of `\n` bytes before
  the offset (`\r` is not a line break); `column` is 1 plus the number of
  bytes since the last `\n` or the start. Display is
  `<line>:<column> (byte <offset>): <message>`.
- **UTF-8 first.** The whole input is validated as UTF-8 before parsing, so
  invalid UTF-8 anywhere is reported (at the first byte of the first invalid
  sequence) in preference to any syntax error.
- **Byte-order mark.** Only the three bytes `EF BB BF` at offset 0 are
  skipped, once. U+FEFF anywhere else outside a string is an unexpected
  character.
- **Where each error points.** An unexpected byte or the end of input: at
  that byte (or the input length). A bare control character: at that byte.
  A bad, truncated or unpaired `\u` escape: at the backslash of the escape
  that is wrong (for an unpaired high surrogate, its own backslash). A
  leading zero: at the zero. A duplicate key: at its opening quote. Nesting:
  at the `[` or `{` that would be at depth 129 (a top-level container is at
  depth 1). Trailing content: at its first byte.
- **Duplicate keys** are compared after unescaping (`"a"` and `"a"`
  are the same key).
- **Numbers.** `-0` has no fraction or exponent and its value fits `Int`, so
  it reads as `Int(0)` (the sign is lost; `-0.0` stays `Float(-0.0)`). A
  `Float` literal is converted with correct rounding; one whose value is
  infinite (`1e400`) is rejected with "number is too large to represent" at
  the start of the number, since it could not be written back. Underflow to
  zero is not an error.
- `\u0000` is accepted and yields U+0000.

### 4.2 Writer details

- `\u00XX` escapes use lower-case hexadecimal (`\u001f`).
- `Float` is written as Rust's `{:?}` for `f64` (shortest round-trip): plain
  decimal with at least one fractional digit when 1e-4 ≤ |x| < 1e16 or
  x = 0 (`1.0`, `0.0001`, `1000000000000000.0`), otherwise the shortest
  digits in exponent form with no `+` and no exponent padding (`1e16`,
  `1.5e300`, `1e-5`, `5e-324`). The output always contains `.` or `e`, so it
  reads back as a `Float`. `-0.0` is written `-0.0`.
- A value nested deeper than 128 cannot be written (`WriteError::TooDeep`),
  so the writer never produces text the reader rejects.
- A top-level scalar in pretty form is the scalar followed by `\n`.

### 4.3 Interface

```rust
// slip_model::json
pub enum Value { Null, Bool(bool), Int(i128), Float(f64), Str(String),
                 Array(Vec<Value>), Object(Vec<(String, Value)>) }
pub fn parse(bytes: &[u8]) -> Result<Value, Error>;
pub fn write_compact(value: &Value) -> Result<String, WriteError>;
pub fn write_pretty(value: &Value) -> Result<String, WriteError>;
pub struct Error { pub offset: usize, pub line: usize, pub column: usize, pub message: String }
pub enum WriteError { NonFiniteFloat(f64), IntOutOfRange(i128), TooDeep }
pub const MAX_DEPTH: usize = 128;
```

`Value::as_u64` and `as_i64` succeed only on an `Int` in range; an integral
`Float` such as `5.0` is not converted. `Value::get` returns the first
member with that key (the reader never produces duplicates).

## 5. The container

Every Slip file is a container:

```
file    := magic kind version section* end trailer
magic   := 53 4C 49 50 1A 0A 00 00          "SLIP", 0x1A, 0x0A, 0x00, 0x00
kind    := 4 bytes, upper-case ASCII letters      (§5.1)
version := u32le, at least 1                      format version of this kind
section := id length payload
id      := uv, at least 1
length  := uv                                     number of payload bytes
end     := 00
trailer := digest                                 content digest (§3) of every byte before it
```

Rules:

1. Sections appear in strictly ascending order of `id`; each id at most once.
2. Ids 1–63 are **critical**: a reader that does not know one fails with
   `UnknownSection`. Ids 64 and above are **ignorable**: a reader that does
   not know one skips it.
3. A section's payload must be consumed exactly by its decoder; leftover
   bytes are `Malformed`.
4. Nothing follows the trailer.
5. A reader verifies the trailer before interpreting any section. The order
   of checks on open is: magic and kind (`BadMagic`), version
   (`UnsupportedVersion`), trailer (`Corrupt`, or `Truncated` if the file is
   shorter than a header, an `end` byte and a trailer), then sections.
6. A section a format marks *required* must be present (`Malformed` if not).

The **content id** of a container file is its trailer digest.

Writers build the whole file in memory or a temporary file and publish it by
an atomic rename (`spec/exec.md`); a reader never sees a partial file under
its final name.

### 5.1 Kinds

| Kind | Format | Deterministic | Section |
|---|---|---|---|
| `PLAN` | The plan | yes | §7 |
| `SHRD` | A shard: one observer's output for one phase | yes | §8 |
| `RUNR` | A run record: what happened and what it cost | no | §9 |
| `DECL` | Declared changes | yes | `verify.md` §2 |
| `TRFM` | A transform request | yes | `edits.md` §3 |
| `EDTS` | An edit set: what reduce accepted and refused | yes | `edits.md` §6 |
| `OVLY` | An overlay: the files of a post-image and its spelling obligations | yes | `edits.md` §7.1 |
| `RSLT` | The result of one run of a transform | yes | `edits.md` §8.5 |
| `RUNT` | A transform run record | no | `edits.md` §10.6 |
| `JRNL` | The journal of a write of the working tree; never leaves the store | yes | `edits.md` §7.3 |

### 5.2 Interface

`slip_model::container` provides a writer (`new(kind, version)`,
`section(id, payload)` in ascending order, `finish() -> Vec<u8>`) and a
reader (`open(bytes, kind, max_version) -> Result<Reader>` performing rule 5,
then `section(id) -> Option<&[u8]>`, `decoder(id) -> Option<Decoder>` whose
offsets are into the whole file, `ids()`, `version()`, `content_id()`, and
`finish()`, the check that every critical section present was requested, so
an unknown critical id cannot be missed: `UnknownSection` at the first one
that was not).
`slip_model::wire` provides an `Encoder` over `Vec<u8>` and a `Decoder` over
`&[u8]` for §2 (`Decoder::with_base(data, base)` offsets every position by
`base`), with `finish()` enforcing rule 3. Errors are
`wire::Error { kind: ErrorKind, offset: usize, what: &'static str }`.

Programming errors, which panic rather than return an error: `Writer::new`
with a kind that is not four upper-case ASCII letters, or version 0;
`Writer::section` with id 0 or an id not above the previous one.

### 5.3 Opening a container

Rule 5 in detail:

1. Magic and kind are compared byte by byte as far as the input reaches; the
   first difference is `BadMagic`. A file shorter than twelve bytes whose bytes
   all match is not `BadMagic`.
2. The version is judged only if all four of its bytes are present: zero or
   above the reader's maximum is `UnsupportedVersion`.
3. A file shorter than 49 bytes (header, `end`, trailer) is then `Truncated`.
4. The last 32 bytes are the trailer; a mismatch is `Corrupt`. Bytes appended
   after a valid file therefore give `Corrupt`; bytes between `end` and a
   correct trailer give `Malformed`.
5. Sections are framed (ids strictly ascending, lengths within the bytes before
   the trailer, minimal `uv`s); a missing `end` byte is `Truncated` at the
   trailer's offset.

### 5.4 Reading a format

A format reader (`from_bytes`, `dump` and `content_id`, §5.5) validates the
whole file. The order of checks is:

1. The container (§5.3): `BadMagic`, `UnsupportedVersion`, `Truncated`,
   `Corrupt`, section framing.
2. `UnknownSection` for any critical section id the format does not define
   (for the shard this includes the reserved ids 16, 18 and 20), before
   any section is interpreted.
3. Each defined section in ascending id order: absent and required is
   `Malformed` at the offset of the `end` byte (file length − 33); otherwise
   its fields, with `Malformed` at the first leftover byte (rule 3). The
   rules an observer record obeys on its own (§7.1) are checked here, as
   each observer of the plan is decoded.
4. The format's invariants, the first violation in section and element
   order: the plan's (§7.1); the shard's (§8.1), then those of `digests`
   (§8.2), then those of `verdict` (§8.3), then those of `edits`,
   `binding`, `rule_use`, `spellings`, `negatives` and `found` (§8). Within `digests` and
   `verdict` the rules are checked in the order of their tables, **each
   rule over the whole section, in field and element order, before the
   next rule**; `edits.md` §4.7 states its own order. The edit set's
   are checked in the order of `edits.md` §6.1's table, the overlay's
   and the result's in the order §7.1 and §8.5 list them
   (`spec/landed/edits/WP-061.md` gives the order within each); the
   transform run record has none.

`dump` and `content_id` dispatch on bytes 8–11 of the file; a file whose kind
is none of those `slip_model::formats` reads (`PLAN`, `SHRD`, `RUNR`,
`DECL`, `TRFM`, `EDTS`, `OVLY`, `RSLT`, `RUNT`) is opened as a plan, which reports `BadMagic` (or
`Truncated` for a short prefix of a valid header). `content_id` returns the
trailer only once the file has been fully validated as by `from_bytes`.

A list's capacity is reserved up front only up to 1024 elements, whatever its
count says; the count itself is already bounded by the bytes remaining
(§2.1).

**Unknown ignorable sections.** The typed readers (`from_bytes`,
`from_json`) skip them (rule 2): the value has nowhere to keep them, so
`to_bytes` of that value writes the file without them. `dump` renders them
as `"#<id>": { "hex": "…" }` after the known sections, and `undump` writes
them back, so `dump` → `undump` reproduces the bytes exactly (§10).

Rules that look like invariants but are not stated in §7.1 or §8.1–§8.3 are
not checked: a reader accepts them ("Open questions").

### 5.5 Interface of the formats

```rust
impl Command { pub fn observer_id(&self) -> Digest; }        // H("slip.observer.v1", Command), §7

// For each of Plan (kind PLAN), Shard (SHRD), RunRecord (RUNR):
impl Plan {
    pub const KIND: [u8; 4];  pub const VERSION: u32;          // VERSION = 1
    pub fn to_bytes(&self) -> Vec<u8>;                         // debug-asserts `validate`
    pub fn from_bytes(bytes: &[u8]) -> Result<Plan, wire::Error>;   // every rule and invariant
    pub fn to_json(&self) -> json::Value;                      // §10
    pub fn from_json(value: &json::Value) -> Result<Plan, JsonFormError>;
    pub fn validate(&self) -> Result<(), String>;              // the invariants, for values built in memory
}

// slip_model::formats
pub fn dump(bytes: &[u8]) -> Result<String, wire::Error>;      // any kind → pretty JSON, for `slip dump`
pub fn undump(value: &json::Value) -> Result<Vec<u8>, JsonFormError>;
pub fn content_id(bytes: &[u8]) -> Result<Digest, wire::Error>;
/// (plan, round, started_unix_ns) of a RUNT file, validated as by TransformRun::from_bytes.
pub fn transform_run_names(bytes: &[u8]) -> Result<(Digest, u64, u64), wire::Error>;
pub fn encode_observer(observer: &Observer, out: &mut Encoder);
pub fn decode_observer(input: &mut Decoder<'_>) -> Result<Observer, wire::Error>;
pub fn encode_phase(phase: Phase, out: &mut Encoder);
pub fn decode_phase(input: &mut Decoder<'_>) -> Result<Phase, wire::Error>;
pub struct JsonFormError { pub path: String, pub message: String }
pub const MESSAGE_MAX: usize = 4096;
pub fn clip_message(message: &str) -> &str;                    // cut to MESSAGE_MAX at a character boundary

// The enumerations of §7–§9
impl Reason { pub const ALL: [Reason; 6]; pub fn name(self) -> &'static str; }
impl Status { pub const ALL: [Status; 4]; pub fn name(self) -> &'static str; }
impl End    { pub const ALL: [End; 5];    pub fn name(self) -> &'static str; }
```

- `undump` is the inverse of `dump`, for any of the nine kinds. It is the
  only JSON → binary path that keeps unknown ignorable sections (§5.4).
- `encode_observer`, `decode_observer`, `encode_phase` and `decode_phase`
  are the §7 `Observer` record and the §8 `Phase` enumeration on their own,
  for the worker protocol (`spec/exec.md` §5.2). The plan and shard codecs
  call these same functions. `decode_observer` checks the rules an observer
  obeys on its own (§7.1); the rules that need the plan (configuration
  index in range, uniqueness of ids and pairs, plan order) are checked only
  by the plan reader.
- `JsonFormError.path` is empty for the top-level value; `Display` is
  `at <path>: <message>` or `at the top level: <message>`. `validate`
  returns `<path>: <message>`, with the same path and message as
  `from_json` gives for the same violation.
- `Shard` has, besides the fields of §8, `digests: Option<Digests>`
  (`slip_model::digests`), `verdict: Option<Verdict>`
  (`slip_model::verdict`), `edits: Option<Edits>` (`slip_model::edits`),
  `binding: Option<Binding>` and `rule_use: Option<RuleUse>`
  (`slip_model::agreement`), `spellings: Option<Spellings>`
  (`slip_model::spellings`), `negatives: Option<Vec<Vec<u8>>>` (§8.6)
  and `found: Option<Found>` (§8.8; `Found { places: Vec<FoundPlace>,
  classes: Vec<Vec<u64>> }`, `FoundPlace { path: Vec<u8>, input: u64 }`),
  in that order; an absent section is `None`.
- `Transform` (`slip_model::transform`, kind `TRFM`, `edits.md` §3),
  `EditSet` (`slip_model::editset`, `EDTS`, §6), `Overlay`
  (`slip_model::overlay`, `OVLY`, §7.1), `TransformResult`
  (`slip_model::result`, `RSLT`, §8.5) and `TransformRun`
  (`slip_model::transform_run`, `RUNT`, §10.6) have the same interface
  as the three above. `transform_run_names` is for the store
  (`exec.md` §2.2), which may not name the type.
- `ALL` lists every value of the enumeration once, in code order; `name()`
  is its name in §7–§9, the one the JSON rendering uses (§10).

## 6. Files and paths

A `path` is a `bytes` holding an absolute file-system path, not necessarily
UTF-8.

A **real path** is a path with every symbolic link resolved, as the operating
system reports it (`realpath`), and spelled as its directories list it
(`read_dir`; `edits.md` §2.1, "One spelling"): on a volume that finds a
name in any case or normalisation, `realpath` keeps the spelling asked,
and Slip gives it the listed one. Slip identifies a file by its real path:
two spellings that reach the same file are the same file, and an edit is
always addressed to a real path.

```
FileRef := path:    path      real path
           size:    uv        bytes
           content: digest    content digest of the file's bytes
```

Paths as *spelled* by a build (in a command line, in an `#include`) are kept
as spelled where they are needed to replay the build, and are never used as
identity.

## 7. Commands, observers and the plan

```
Command := directory: bytes          working directory, absolute, as given by the database
           argv:      list<bytes>    the normalised command line (spec/cdb.md §5); argv[0] is the compiler as written
           env:       list<EnvVar>   sorted by name; environment variables the front end reads (empty in M0)
EnvVar  := name: bytes, value: bytes

ObserverId := H("slip.observer.v1", Command)
```

An **observer** is one compilation: a `Command`. Two database entries that
normalise to the same `Command` are one observer, whichever configurations
they came from. A configuration is a label on a compilation database.

The plan, kind `PLAN`, version 1. All sections are required.

| Id | Section | Payload |
|---|---|---|
| 1 | tool | `slip: str`, `kiln: str` — versions of Slip and of the Kiln front end it links |
| 2 | configurations | `list<Configuration>` in the order given on the command line |
| 3 | observers | `list<Observer>` in plan order (below) |
| 4 | excluded | `list<Excluded>` sorted by (`configuration`, `entry`) |

```
Configuration := name:     str        unique within the plan
                 database: FileRef    the compilation database file

Observer := id:        digest               ObserverId of `command`
            command:   Command
            language:  Language
            main:      FileRef              the main source file
            cost_hint: uv                   relative cost estimate; the main file's size when nothing better is known
            origins:   list<Origin>         sorted by (configuration, entry); at least one
Origin   := configuration: uv              index into section 2
            entry:         uv              zero-based index of the entry in that database

Excluded := configuration: uv
            entry:         uv
            file:          bytes            the entry's file, as spelled
            reason:        Reason
            detail:        str              human-readable; for `unsupported_flag`, the flag

Language := c = 1 | cxx = 2 | objc = 3 | objcxx = 4
Reason   := not_a_compile = 1        the command does not compile a source file (link, assemble, preprocess-only output, …)
          | unsupported_language = 2 the front end does not compile this language
          | unsupported_flag = 3     the front end rejects a flag in the command
          | missing_file = 4         the main file does not exist
          | malformed_entry = 5      the entry could not be turned into a command
          | unsupported_compiler = 6 the command line is not in the GCC/clang style (for example `cl`)
```

**Plan order.** Observers are ordered by `cost_hint` descending, then by `id`
ascending. Largest first keeps the tail of a parallel run short; the tie-break
makes the order a function of the plan's content.

**Accounting invariant.** Every (configuration, entry) pair of every database
appears exactly once: either in some observer's `origins` or in `excluded`.
A reader checks that no pair appears twice and that observer ids are unique
and equal `ObserverId(command)`.

### 7.1 Invariants

A plan reader checks the rules below. The first four are the rules an
observer record obeys on its own, checked in that order. The binary reader
checks them as each observer is decoded (§5.4 step 3), and the others after
every section is read; `from_json` and `validate` check them for each
observer in turn, before that observer's other rules.

| Rule | Offset | JSON path |
|---|---|---|
| `id = ObserverId(command)` | the observer's first byte (its id) | `sections.observers[i].id` |
| `command.env` strictly ascending by name | the first `EnvVar` out of order | `sections.observers[i].command.env[j]` |
| at least one origin | the `origins` count | `sections.observers[i].origins` |
| origins strictly ascending | that origin's first byte | `sections.observers[i].origins[j]` |
| configuration names unique | the second configuration's first byte | `sections.configurations[i].name` |
| observers in plan order | that observer's first byte | `sections.observers[i]` |
| observer ids unique | the observer's first byte (its id) | `sections.observers[i].id` |
| an origin's configuration index in range; its pair not seen before | that origin's first byte | `sections.observers[i].origins[j]` |
| `excluded` strictly ascending; its configuration index in range; its pair not seen before | that entry's first byte | `sections.excluded[k]` |

The other rules are checked in this order: configuration names; then for
each observer in turn, plan order, uniqueness of its id, and each origin
(index, then pair); then for each exclusion in turn, order, index, pair. A (configuration, entry) pair is "seen before" if it occurs in
an earlier origin (observers in plan order, origins in order) or, for an
exclusion, in any origin or an earlier exclusion. Duplicates are reported
at the later occurrence.

## 8. The shard

A shard is what one observer produced in one phase. Kind `SHRD`, version 2
(WP-112: version 2 has the `names` section, 27). A reader refuses a
version-1 shard as `UnsupportedVersion` at byte 12 (JSON path `version`):
it predates `names`, and a compared shard without it is not one without
notes (`verify.md` §4.4); a store written before is parsed again.
Its content is deterministic (§1 rule 2): parsing the same inputs twice gives
the same bytes.

| Id | Section | Required | Payload |
|---|---|---|---|
| 1 | header | yes | `observer: digest`, `phase: Phase`, `round: uv` |
| 2 | outcome | yes | `Outcome` |
| 3 | inputs | no | `list<FileRef>` sorted by `path`: every file the front end read, the main file included |
| 4 | counters | yes | `list<Counter>` sorted by `name` |
| 16 | coverage | — | reserved (M4, with the preprocessing record) |
| 17 | — | — | retired (WP-139): `edits` before `instances`; no writer writes it |
| 18 | facts | — | reserved (M3) |
| 19 | digests | no | `Digests` (`digest.md` §11; §8.2) |
| 20 | reports | — | reserved and not planned: a transform's reports are part of `edits` (32) |
| 21 | verdict | no | `Verdict` (`verify.md` §6; §8.3) |
| 24 | binding | no | `Binding` (`verify.md` §9.1): the declared changes, the overlay and the pre round a verify shard was made with; phase `verify` |
| 25 | — | — | retired (WP-111): `rule_use` before `kept_via` and `kept`; no writer writes it |
| 26 | spellings | no | `Spellings` (`verify.md` §10.4): the spelled-entity checks of this observer; phase `verify` |
| 27 | names | with `digests` | `Names` (§8.7): the names digest of each key that has one, and the notes of a verify shard (`verify.md` §4.4); phases `map` and `verify` |
| 28 | negatives | no | `list<Negative>` (§8.6): the places the map parse looked for a file and found none, sorted; phase `map` |
| 29 | rule_use | no | `RuleUse` (`verify.md` §9.1), with `kept_via` and `kept` (WP-111); phase `verify` |
| 31 | found | no | `Found` (§8.8): each place where the map parse found a file, with the input it led to, and the classes of `inputs` that were one file; phase `map` |
| 32 | edits | no | `Edits` (`edits.md` §4): what a transform's run over this observer produced (claims, groups, reports, declarations, instances, or its failure); phase `map` |

```
Phase   := map = 1 | verify = 2

Outcome := status:   Status
           errors:   uv        diagnostics of severity error or fatal
           warnings: uv
           message:  str       empty for `ok`; otherwise the first error, at most 4096 bytes,
                               cut at a character boundary

Status  := ok = 0              the front end finished with no error
         | diagnosed = 1       the front end reported errors
         | rejected = 2        the front end refused the command line
         | failed = 3          the worker could not produce a result (crash, killed, budget exceeded);
                               written by the coordinator on the worker's behalf (spec/exec.md)

Counter := name: str, value: uv
```

Counters are deterministic quantities only (tokens lexed, files read, bytes
read, declarations, and whatever else later phases count). Names are
lower-case ASCII words joined by `.` (`pp.tokens`, `inputs.files`,
`inputs.bytes`); each spec that produces a counter registers its name. The
harness gates on counters exactly (`spec/harness.md`).

A shard with status `failed` has no `inputs` section and an empty `counters`
list: nothing about a failed observer is deterministic.

A reserved id (16, 18, 20) or a retired one (17, 25) is a critical id no
reader knows: a file with one is `UnknownSection`. Ids 22, 23 and 30 are
not assigned. Sections 17, 24, 25 and 26 were added by M2, 28 by work
package WP-117, 29 by WP-111, 31 by WP-135 and 32 by WP-139, without a change of the
shard's version: a reader that predates one refuses a shard that has it
(the ids are critical), and a shard without them reads as before (a map
shard without `negatives` is then refused by the verify task's guard N1,
one without `found` by its guard N2, `verify.md` §5.3: it is made again
by `slip parse`). A
section whose payload changes takes a new id and the old one is retired,
so that a reader never meets a payload it does not know under an id it
knows (WP-111: 25 to 29; WP-139: 17 to 32, when `Edits` gained
`instances`, `edits.md` §4.5). An `edits` section
in a shard of phase `verify`, or a `binding`, `rule_use` or `spellings`
section in a shard of phase `map`, is `Malformed` at the first byte of
the section's payload, JSON path `sections.<name>`. So are `edits`,
`rule_use` and `spellings` in a shard whose status is not `ok`, and
`binding` in one whose status is `failed` (only). The phase is checked
before the status. Their own invariants are `edits.md` §4.7 and
`verify.md` §9.1 and §10.4, checked after those of §8.1 to §8.3 in the
order of the section ids, each section's phase and status first.
`negatives`'s own are §8.6's, then `found`'s, §8.8's, then `edits`'s
(32), last.

Section 27 (`names`, WP-112) changed the version to 2 (above): every
shard a reader of version 2 reads, of either phase, is one that could
have 27, 28, 29 and 31, and a version-1 shard, which predates `names`, is
refused whatever its sections (a version-1 shard with 25 or 29 included).
`names` and `rule_use` (29) are independent: a verify shard may have
either, both or neither, and neither's rows refer to the other's. It is present **exactly when** `digests`
is: a shard with `digests` and without `names` is `Malformed` at the
first byte of the `digests` payload, JSON path `sections.names` (a pre
shard of an older build fails closed, not as "no note"); its own rows
(§8.7) are checked after those of `spellings`, before `rule_use`'s and
`negatives`'s.

### 8.1 Invariants

| Rule | Offset | JSON path |
|---|---|---|
| `message` empty for `ok` | the `message` field's first byte | `sections.outcome.message` |
| `message` at most 4096 bytes | the `message` field's first byte | `sections.outcome.message` |
| no `inputs` section for `failed` | the first byte of the `inputs` payload | `sections.inputs` |
| `inputs` strictly ascending by path | that `FileRef`'s first byte | `sections.inputs[i]` |
| counter name matches `[a-z0-9_]+(\.[a-z0-9_]+)*` | that counter's first byte | `sections.counters[i].name` |
| counters strictly ascending by name | that counter's first byte | `sections.counters[i]` |
| no counters for `failed` | the first counter's first byte | `sections.counters[0]` |

Checked in this order, the two counter rules for each counter in turn.

### 8.2 The `digests` section

**Binary form.** Each record is its fields in the order of `digest.md` §11,
in the encodings of §2: `GroupClass` is an enumeration, `units` an `opt`. A
group is `class`, `file`, `count`, `indeterminate` (`uv`), `digest`,
`units`. `tu_digest` hashes the `list<GroupSum>`: the count, then per group
`uv(class)`, `bytes(file)`, `uv(count)`, `uv(indeterminate)`, `digest`.
`scheme` is `slip_model::digests::SCHEME`, 3 (WP-155; `digest.md` §11).
A unit's key is `digest.md` §3's, with the kinds and name forms of
`digest-kinds.md` §3.1 and §3.2: a reader decodes a key kind or name
form by its number and refuses one the table does not list, a
`reserved` row included (`KeyKind` 34, `Name` 16). WP-156 added
`KeyKind` 35 and `Name` 17 (`operands`, the operand unit) under the
same `scheme` (its question is the handback's, `docs/wp/WP-156.md`).

**Invariants.** The section's payload starts at offset `P` (the `scheme`
field).

| # | Rule | Offset | JSON path |
|---|---|---|---|
| 1 | present only if the outcome's status is `ok` | `P` | `sections.digests` |
| 2 | `scheme` = `digests::SCHEME` (3) | `P` | `sections.digests.scheme` |
| 3 | `groups` strictly ascending by `(class, file)` | the later group's first byte | `sections.digests.groups[i]` |
| 4 | a `tu` group's `file` is empty; a `declared` group's is not | the `file` field | `sections.digests.groups[i].file` |
| 5a | `count` ≥ 1 | the `count` field | `sections.digests.groups[i].count` |
| 5b | `indeterminate` ≤ `count` | the `indeterminate` field | `sections.digests.groups[i].indeterminate` |
| 6a | `units` strictly ascending by `(key, digest)` | the later unit's first byte | `sections.digests.groups[i].units[j]` |
| 6b | the length of `units` is `count` | the list's count (the byte after the `opt` tag) | `sections.digests.groups[i].units` |
| 6c | every `flags` has no bit but bits 0 (indeterminate) and 1 (emitted unreferenced, WP-155): 0 to 3 | the `flags` field | `sections.digests.groups[i].units[j].flags` |
| 6d | `indeterminate` is the number of `units` with flag bit 0 set | the `indeterminate` field | `sections.digests.groups[i].indeterminate` |
| 6e | `digest` = `group_digest(units)` | the group's `digest` field | `sections.digests.groups[i].digest` |
| 7 | `tu` = `tu_digest(groups)` | the `tu` field (`P` + 1 while `scheme` < 128, one byte) | `sections.digests.tu` |

Rules 5a and 5b are each checked over every group; rules 6a–6e are each
checked over every group whose `units` is present, in turn (§5.4). Rule 5b
applies to every group, and is the only check of `indeterminate` for a
group without a unit list. A `derived` group may have an empty or a
non-empty `file` (builtins have the empty path). Rules 6e and 7 are
computed by every reader: stored digests that do not match their content
are rejected. Rule 6a orders by `(key, digest)`, not by `Unit`'s full order
with `flags`: two units of one group with the same key and digest are
rejected whatever their flags (`digest.md` §11: they are one unit).

### 8.3 The `verdict` section

**Binary form.** Each record is its fields in the order of `verify.md` §6,
in the encodings of §2: `VerdictKind` and `FindingKind` are enumerations;
`key`, `pre`, `post` and `where` are `opt`; `used` is `list<digest>`.

**Invariants.** The section's payload starts at offset `V` (the `kind`
field).

| # | Rule | Offset | JSON path |
|---|---|---|---|
| 1 | present only if the phase is `verify`, then only if the status is `ok`, then only if `digests` is present (three messages, in that order) | `V` | `sections.verdict` |
| 2 | `reason`; each finding's `name`, then `where.pre.node`, `where.pre.what`, `where.post.node`, `where.post.what`: at most 4096 bytes | the field | `sections.verdict.reason`, `….findings[i].name`, `….findings[i].where.<pre\|post>.<node\|what>` |
| 3 | by `kind`, checking in field order: `verified`: `pre` not all zero, `reason` empty, `total` 0, `findings` empty; `mismatch`: `pre` not all zero, `reason` empty, `total` ≥ 1; `unverifiable`: `reason` non-empty, `total` 0, `findings` empty (only `unverifiable` may have no pre shard: `pre` all zero) | the `pre` field, the `reason` field, the `total` field, the `findings` list's count | `sections.verdict.pre`, `.reason`, `.total`, `.findings` |
| 4a | `findings` has at most `total` elements | the first finding beyond `total` | `sections.verdict.findings[total]` |
| 4b | for each finding in turn: ascending by `(rank, file, key)`, where `rank` is that of the finding's kind (`verify.md` §6: `changed` 0, `removed` 1, `added` 2, `derived_changed` 3, `derived_only_pre` 4, `derived_only_post` 5, `indeterminate` 6, `derived_indeterminate` 7; not the kind's number), an absent key first (not strictly: findings with no key may tie); then, if it has a key, no earlier finding has the same `(kind, key)` | that finding's first byte | `sections.verdict.findings[i]` |
| 5 | for each finding in turn: `key` absent only for `indeterminate`; an `added` or `derived_only_post` finding has no `pre`; a `removed` or `derived_only_pre` finding has no `post` (the two kinds since WP-155) | the `opt` tag of `key`, `pre`, `post` | `sections.verdict.findings[i].key`, `.pre`, `.post` |
| 6 | `used` strictly ascending | the later element | `sections.verdict.used[j]` |
| 7 | in each `Site` (pre, then post), `line` is 0 exactly when `column` is 0 | the `line` field | `sections.verdict.findings[i].where.<pre\|post>.line` |

`changed` and `derived_changed` have a key because only `indeterminate` may
lack one (rule 5); `derived_indeterminate` must have one too. `MESSAGE_MAX`
(4096) is the limit of rule 2, the same as `Outcome.message`'s, counted in
bytes of UTF-8.

### 8.4 The parse task

`slip_kiln::parse_observer` produces the shard of one observer by running
Kiln's front end in the calling process, through Kiln's front-end library
entry `kiln_driver::front_end::run_front_end` (`kiln/spec/driver.md`
"Front-end library entry"); `parse_observer_with` does the same and calls a
closure on the finished unit:

```rust
pub fn parse_observer(observer: &Observer, phase: Phase, round: u64) -> Result<Shard, String>;
pub fn parse_observer_with<R: Send>(
    observer: &Observer, phase: Phase, round: u64,
    options: ParseOptions,
    f: impl FnOnce(&Unit<'_>) -> R + Send,
) -> Result<(Shard, Option<R>), String>;

/// What the front end keeps beyond what the parse task needs. Every option
/// is off by default, and the shard is the same whatever they are.
#[non_exhaustive]
pub struct ParseOptions { pub pp_record: bool, pub lookups: bool }

/// What header search looked at (`ParseOptions::lookups`: Kiln's lookup log,
/// `FrontEndOptions::record_lookups`), as the front end spelled it; a
/// relative directory is relative to `Command.directory`. Names no Kiln type.
pub struct Lookups {
    pub list: Vec<String>,                 // the search list's directories
    pub tried: Vec<(String, String)>,      // (directory, name): each place a lookup tried, unique
    pub missed: Vec<(String, String)>,     // (directory, name): each place it found nothing (§8.6)
    pub read: Vec<String>,                 // each input's name as spelled when read
    pub line_names: Vec<String>,           // each name a `#line` directive gave, as written (guard G5)
}

pub struct Unit<'r> { /* … */ }        // the finished front end, valid inside the call
impl<'r> Unit<'r> {
    pub fn ast(&self) -> &'r kiln::ast::Ast;
    pub fn sm(&self) -> &'r kiln::source::SourceManager;
    pub fn oven(&self) -> &'r kiln::oven::Oven;
    pub fn pp_record(&self) -> Option<&'r kiln::pp::PpRecord>;   // when ParseOptions::pp_record asked
    pub fn lookups(&self) -> Option<&'r Lookups>;                 // when ParseOptions::lookups asked
    pub fn main_file(&self) -> Option<kiln::source::FileId>;
    pub fn real_path(&self, file: kiln::source::FileId) -> Option<&'r [u8]>;
    pub fn errors(&self) -> u32;
    pub fn stack_remaining(&self) -> Option<usize>;
    pub fn token_end(&self, at: kiln::source::Loc) -> Option<kiln::source::Loc>;
}
```

`Lookups::tried` lists, for a `#include` form (`#include`, `#import`,
`#include_next`, `__has_include`, `__has_include_next`, and `-include`,
looked up from the predefines buffer), the includer's directory where the
form tries it, then the search list from where the form starts (`"..."`:
the start; `<...>`: the angled start) or continues (after the directory
an `_next` form continues from), through the directory it was found in,
all of it when it was not; for `#embed` and `__has_embed`, the includer's
directory and `.` for `"..."`, then every `--embed-dir` directory (Kiln's
log does not say which were tried; they are taken from Kiln's driver,
`Args::embed_dirs`). A lookup of an absolute name is not in Kiln's log
and so not here (`spec/verify.md` §5.1, "What remains outside").
`Lookups::found` lists the places where a lookup found its file, unique,
in the order first seen (work package WP-135; §8.8): the place Kiln
opened, which its log names exactly (Kiln `HeaderSearch::lookup` opens
`join_path(dir, name)` for the directory the log records): the
includer's directory for a hit beside the includer (`.` when the log
names no includer, Kiln's own rule), the search list's entry for a hit
on the list, and for `#embed` and `__has_embed` `(".", path)` with the
path the log records as read. A lookup of an absolute name is `("/",
name)` once Kiln's log records one. `Lookups::read` names each buffer
read from disk and each consulted path as the front end first spelled
it, the main file included.
`Lookups::missed` lists the places where a lookup looked for its name and
found nothing, unique, in the order first seen (the negative inputs, §8.6,
item by item in `spec/landed/verify/WP-117.md`): `tried` less the place
each form found its file in, with two differences. For `#embed` and
`__has_embed` it lists the tries before the path read (the includer's
directory, `.` for `"..."`, then each `--embed-dir`, in Kiln's order), all
of them when none was. And a search directory of the command's own (`-I`,
`-iquote`, `-isystem`, `-idirafter`, `-cxx-isystem`; `-I=` under the
sysroot) that is not in the list (Kiln's driver left it out because it
was no directory, or because it was a duplicate of a listed one: another
spelling of it, or a link to it, which may be retargeted since) is a
place of each name a `#include` form searched the list for where, were
it a directory of its own, it would be searched: at its place in the
list, from where the lookup started (the angled start for `<...>`, after
the includer's directory for `#include_next`) up to and including the
place before the directory the lookup found the name in (through the end
when it found none), unless the name is a regular file in it now (a
duplicate holds its twin's files, where the lookup found them). Its
place is found by building the command's own directories into a list as
Kiln's driver does for C++ (Kiln `SearchList::build`), once as they are
and once with that directory a directory of a unique identity (which
changes no other entry: whatever Kiln dropped because of it was itself);
the toolchain's directories, all of a system group and after the
command's in it, are the one block of the log's list that the command's
list lacks, at the end of its system part (before the `-idirafter`
directories), none of them one of the command's. Two cases keep more. An absent
`-isystem` or `-idirafter` directory is a place of every lookup that
ended in an `-I` directory: created as that same directory, it would
take its place (clang `RemoveDuplicates` drops a user directory that a
later system one duplicates). And when the place cannot be told, the
directory is a place of every name searched in the list (not of a
`<...>` name when only `-iquote` names it): when the log's list is not
the command's own list with one toolchain block (a toolchain directory
that displaced one of the command's; an `-iquote` directory that is also
a toolchain one, listed twice since Kiln deduplicates the quoted part
apart; a C unit's `-cxx-isystem` directories, which the C++ build lists
and Kiln's driver leaves out for C). Kiln's driver accepts no
`-c-isystem` word (the unit is `rejected`).
`Lookups::line_names` lists the file names of the `#line` directives of
every file the unit read (Kiln's `SourceManager::line_directives`), as
written: guard G5 of `spec/verify.md` §5.3 refuses one that begins with a
prefix the post command maps.
`Lookups` is a function of the unit; the shard does not depend on it.

`token_end(at)` is Kiln's `kiln_lex::token_end` with the unit's interner
and lexer options (`Preprocessor::lex_options`): the end of the token
that starts at `at`, or `None` (Kiln `spec/lex.md` "End of a token").
It is the only holder in Slip of either; `slip_kiln::ranges` reaches the
end of a token through it (`spec/landed/api/WP-084.md`).

`stack_remaining()` is the stack left on the calling thread under Kiln's
stack guard (`kiln_sema::stack_guard::StackGuard::current().remaining()`):
`None` where no guard is installed. Inside the closure the calling thread
is the front end's.

`Err` is a failure of the worker itself, not of the subject; the
coordinator records `failed` (`spec/exec.md`). `parse_observer` is
`parse_observer_with` with a closure that does nothing. `kiln::…` is
`slip_kiln::kiln`: only the crates that may name it call `Unit`'s methods
(`spec/landed/harness/WP-021.md`); any crate may pass a `&Unit` on.

**The run.**

- A command that is not UTF-8 is `rejected` without running the front end:
  `errors` 1, `warnings` 0, `message` `` argument `<argument>` is not
  UTF-8 `` (the first such argument, decoded lossily).
- The process's current directory is set to `Command.directory` (failing
  that is `Err`) and not restored: Kiln has no working-directory parameter.
  Callers that parse on several threads of one process must serialise.
- The command line is the one of `spec/cdb.md` §6.1, `argv[0]
  -fsyntax-only -fkiln-reproducible-time argv[1..]`, given to
  `run_front_end` with
  `FrontEndOptions::default()`: a thread of the front end's own, whose stack
  is the command's `-fkiln-stack-size=`, else Kiln's default, with Kiln's
  stack guard installed; no lookup log; nothing rendered. The entry writes
  no file whatever the command asks (`-o`, `-MD`, `-MF`, `-MJ`,
  `-fconstexpr-report=`, …) and reads no standard input.
- `Command.env` is not applied: the front end sees the worker's environment.
- `-fkiln-reproducible-time` fixes `__DATE__`, `__TIME__` and
  `__TIMESTAMP__` at the Unix epoch whatever the clock and whatever
  `SOURCE_DATE_EPOCH` holds, so that a constant computed from one of them
  is the same in every parse and a shard's content is a function of its
  inputs (§8). The digest writes the macros themselves symbolically
  (`digest.md` §9).
- When a parse happened, the entry calls the closure once the AST is final,
  with errors or without (a fatal error included): the parse task first
  computes the inputs (below) from the unit, then calls `f` on a `Unit`
  over the same state. `real_path(file)` is the `path` of the `inputs`
  entry that buffer `file` gave (the real path), `None` for any other
  buffer; `errors()` is the job engine's error count when `f` is called
  (the driver's not included). A failure computing the inputs is `Err` and
  `f` is not called. No parse, no call: `f`'s result is `None`.
- `FrontEndError::Panicked(text)` (a panic of the front end or of `f`) is
  `Err("the front end panicked: <text>")`; `FrontEndError::Thread` is `Err`
  with the error's text. The worker answers `Failed`; the coordinator
  retries once (`spec/exec.md` §5.4).

**Outcome**, from the entry's `FrontEndReport`:

- `errors` and `warnings` are the report's: the driver's and the job's
  engines together, after the command's `-ferror-limit=` (Kiln's default,
  as clang's) and `-Wfatal-errors`, with `-Werror` applied.
- `ok`: `errors` is 0 and a parse happened. `message` is empty. No error
  and no parse is `Err` ("the front end did not parse and reported no
  error").
- Otherwise `rejected` if the first error (`FrontEndReport.first_error`,
  the first diagnostic of level error or fatal in the order emitted) has an
  id whose name (`DiagId::name`) starts with `drv_`: the driver refused the
  command line, or a job's command-line check such as `-std=` failed. Else
  `diagnosed` (a free-text first error, without an id, included).
- `message` is the first error as `<file>:<line>:<col>: <message>` from
  `FirstError.at` (the presumed file name and line, the column in bytes, as
  Kiln's text output prints them) and `FirstError.message` (the text after
  `error: `, with the ` [<option>]` suffix the text output prints when the
  command shows options, as it does by default); `<message>` alone when
  the error has no position. Cut to 4096 bytes at a character boundary. An
  error count with no first error kept (not expected) is `diagnosed` with
  the message `the front end reported <N> errors and kept none`.
- `FrontEndError::NotSyntaxOnly` and `FrontEndError::NotOneInput` (what the
  entry refuses without running: an action other than `-fsyntax-only`,
  `--help`, `-###`, a `-print-*` or `-dump*` probe, `-M`/`-MM`, an unbuilt
  header unit; not exactly one input, standard input, an assembler input)
  are `rejected`, `errors` 1, `warnings` 0, `message` the error's text
  (`the command line is not -fsyntax-only`, `the command line has not one
  source input`). What the entry refuses with `drv_front_end_unsupported`
  (`-verify`, `-include-pch`, written or made by the PCH probe of
  `-include`) is a driver error, so `rejected` by the rule above.

**Inputs.** Present exactly when a parse happened, whatever the status
(so also for a `diagnosed` unit whose `#include` was not found); absent
otherwise. The list is made from the unit, without a second read of a file
the front end read:

- every buffer of the source manager (`SourceManager::file_ids`) whose
  origin (`file_origin`) is `Disk`: its size and content digest are those
  of `original_bytes` (the bytes as read, byte-order mark and invalid UTF-8
  included);
- every buffer whose origin is `Lazy` (from a module file or a precompiled
  header) that has `original_bytes`; one without them (never read) is left
  out;
- every path of `FrontEnd.consulted` (`__has_include` and `__has_embed`
  hits, `#embed` resources; Kiln lists each once and leaves out the files
  the preprocessor entered) whose real path is not that of a buffer above:
  read here, once, for its size and digest; failing that is `Err`. (A
  `__has_include` hit is a buffer; an `#embed` resource is not: Kiln keeps
  no bytes of it.)

`Virtual` buffers (the predefines, the command line's macros, token-paste
results) are not inputs. Each name (`file_name`, the path as first loaded,
or the consulted path) is resolved against `Command.directory` and to its
real path, spelled on disk (failing that is `Err`). The list is sorted by path bytes and
made unique; two entries with one real path and different sizes or digests
are `Err` (`<path> changed during the parse: two buffers of it differ`).

For a unit the parse task of WP-010 gave inputs for (Kiln's `-MD`: the main
file, every header entered or skipped by an include guard or `#pragma
once`, every file found by `__has_include`, every `#embed` resource, system
headers included), this is the same list
(`crates/slip_kiln/tests/parse.rs` "parity").

**Counters**, when a parse happened; none otherwise:

- `inputs.bytes`: the sum of the inputs' sizes;
- `inputs.files`: their number;
- `inputs.respelled`: the inputs whose name, resolved, reached the file
  under another spelling than its own on disk (`edits.md` §2.1, "One
  spelling"; an `#include` in another case); present only when not 0, so
  that a shard on a volume that does not fold is what it was without it;
- `pp.tokens`: the tokens the preprocessor delivered to the parser, the end
  of file not counted (`FrontEnd.tokens`). After a fatal error, Kiln's
  count: the tokens before it and the one returned with it.

The parse task writes neither `digests` nor `verdict`.

### 8.5 The map task

The task of phase `map` is the parse task with the walk of `digest.md` run
on the finished unit. It is what `slip parse` runs (`spec/exec.md` §8.2).

```rust
// slip_digest::map
/// The map task's walk: `None` when `unit.errors()` is not 0 (the walk is
/// not run), else `Some(digest_unit(unit, options, subject))`. The worker
/// calls this and no method of `Unit` (§8.4).
pub fn map_unit(unit: &slip_kiln::Unit<'_>, options: &Options<'_>,
                subject: &dyn Fn(&[u8]) -> Vec<u8>) -> Option<Result<Output, String>>;
/// The walk over a finished unit. `subject` maps the real path of an
/// input to its subject path (`spec/verify.md` §5.1); the identity for
/// the map phase.
pub fn digest_unit(unit: &slip_kiln::Unit<'_>, options: &Options<'_>,
                   subject: &dyn Fn(&[u8]) -> Vec<u8>) -> Result<Output, String>;
/// The same checks and the same `Input`, for another use of the walk
/// (`spec/verify.md` §2.3 records, §6.1 traces): `digest_unit` is
/// `with_input(unit, subject, |input| digest(input, options))`.
pub fn with_input<R>(unit: &slip_kiln::Unit<'_>, subject: &dyn Fn(&[u8]) -> Vec<u8>,
                     f: impl FnOnce(&Input<'_>) -> R) -> Result<R, String>;
/// The counters of a walk as a shard carries them: `output.counters` and
/// the cause counters below, sorted by name.
pub fn counters(output: &Output) -> Vec<slip_model::Counter>;
/// The counter word of a cause's text.
pub fn cause_word(text: &str) -> String;
```

**The walk** runs inside the closure of `parse_observer_with`, on the
front end's thread, when `unit.errors()` is 0 (`map_unit`); with errors
it does not run, so neither do its checks, and the shard is the parse
task's (`diagnosed`, no `digests`). Rules:
none; detail: `none`; names: not asked for.

- **The stack.** The walk bounds its own recursion and needs
  `slip_digest::STACK_SIZE` bytes of stack for it (`digest.md`; the walk
  marks a unit indeterminate rather than go deeper). `digest_unit`
  compares `unit.stack_remaining()` with it first: less is
  `Err("the front end's thread has <n> bytes of stack left; the digest
  needs <STACK_SIZE>")`, and so is `None`
  (`Err("no stack guard on the front end's thread")`). The front end's
  thread has the command's `-fkiln-stack-size=`, else Kiln's default of
  1 GiB; a command that asks for less than the walk needs fails its job
  instead of overflowing. That `STACK_SIZE` covers the walk's bound is
  shown by a test built in the release profile (`spec/harness.md` §9.3).
- **Subject paths.** The subject path of a buffer that is one of the
  shard's `inputs` is `subject(unit.real_path(file))`. A buffer that is
  not an input and whose name in the source manager starts with `<` (the
  predefines `<built-in>`, the command line's macros) has that name as
  its subject path. Any other buffer is `Err("a declaration lies in
  <name>, which is not an input")`: nothing can be said about a file
  that was not read.
- **`Err`** is returned from the closure and makes the job fail
  (`spec/exec.md` §5.4), as any failure of the worker does. A shard with
  `digests` is never written from a walk that reported one.
- **The shard.** If the parse task's shard is `ok`, it gets the walk's
  `digests` and counters. If it is not `ok` although the unit had no
  error when the closure ran (an error the driver reported, a count
  raised by `-Werror` after it), the walk's result is dropped: `digests`
  are for `ok` shards only (§8.2 rule 1).
- A shard that breaks §8.2 fails the job through the worker's
  validation (`spec/exec.md` §5.5). The walk gives no declared unit the
  empty path for want of a location (`digest.md` §11: the file of the
  earliest declaration that has one; an entity with none is derived; a
  declared unit that should still have none is written in the derived
  group, `Unaccounted("unit/file")`). A declared group with the empty
  path can still come from a caller's `subject` that answers the empty
  path for an input; that shard is refused, not repaired by dropping
  the unit or inventing a file.

**Counters.** Those of the parse task; those of `digest.md` §12; the
walk's count of indeterminate keys by cause, `digest.indeterminate.<cause>`
(`error`, `unsupported`, `unaccounted`, `collision`, `undeclared_local`,
`visibility_by_use`, and while the walk has areas not yet written `pending` and
`digest.indeterminate.pending.<area>`), each present only when not 0;
the operand units' `digest.operands.<name>` (WP-156, `digest.md` §12),
each present only when not 0; and
for the causes that carry a text (`unsupported`, `unaccounted`):

```
digest.cause.<cause>.<word>          indeterminate keys whose first cause is this one
```

`<word>` is the cause's text (`ExprKind/CXXReflectExpr`, `walk/depth`)
with ASCII upper-case letters made lower-case, `/` made `.`, every other
byte outside `[a-z0-9_]` made `_`, and an empty word between two `.`, or
at either end, made `_`: `digest.cause.unsupported.exprkind.cxxreflectexpr`,
`digest.cause.unaccounted.walk.depth`. Two texts with one word add up.
`cargo xtask perf summarize` sums every counter over a run
(`spec/harness.md` §7.2), so a corpus run lists, per cause, how many keys
it makes indeterminate; `digest.cause.unaccounted.walk.depth` is the
number of keys cut off by the walk's depth bound.

**Readers.** A shard of phase `map` that is `ok` and has no `digests` was
written before this section (or by the parse task alone); verification
treats it as an unusable pre shard (`spec/verify.md` §5.3, P3). The
format still does not require the section ("Open questions").

**Negative inputs.** The map task parses with `ParseOptions::lookups` on,
and a shard that has `inputs` gets the section `negatives` (§8.6) from
the unit's `Lookups::missed`, and the section `found` (§8.8) from its
`Lookups::found` and `Lookups::read`, whatever its status. `slip parse` (through
the map task) is the only command that writes it; the verify task, `slip
declare` and `slip units` write no map shard.

### 8.6 The `negatives` section

The **negative inputs** of a map shard: each place the map parse looked
for a file and found none, so that the verify task can tell that none has
appeared there since (guard N1, `verify.md` §5.3; work package WP-117).
What a place is, form by form, is `Lookups::missed` (§8.4); what Kiln's
log does not show is not one (`verify.md` §5.1, "What remains outside").

```
negatives := list<Negative>
Negative  := shared: uv        bytes in common with the path before (none for the first)
             rest:   bytes     the path's bytes after them
```

Each path is the first `shared` bytes of the one before, then `rest`;
`shared` is the length of the two paths' longest common prefix (one list
has one encoding). The JSON form (§10) is the list of whole paths.

**The path of a place.** A place `(d, n)` is the path
`Command.directory/d/n` (an absolute `d` or `n` replaces what is before
it) as written, with its `.` components and repeated `/` dropped: nothing
is resolved, no symbolic link is followed, `..` is kept. Paths are bytes,
absolute, as `inputs` writes them (§3). Guard N1 asks the kernel about the
path when it looks, so it reaches what the front end would reach then,
wherever the links on the way lead by then.

So two hosts with the same tree at the same paths write the same bytes:
the path is text of the command and of the source, and the tree decides
only which places there are (where a lookup found its file). In
particular:

- **A directory that does not exist** is written as any other:
  `-I/b/later` writes `/b/later/h.h`; `-I/b/x/../y` writes
  `/b/x/../y/h.h`, which exists only once `/b/x` does, as for the front
  end (the kernel takes `..` from the directory reached).
- **A directory reached through a symbolic link** is written as spelled,
  not as the link's target: `-I/b/lnk` with `lnk -> inc` writes
  `/b/lnk/h.h`. A link retargeted after the map phase is then followed
  to its new target when N1 looks, as the post parse follows it. Two
  spellings that differ only by `.` or `/` are one place; others
  (`/b/inc`, `/b/lnk`) are two.

The real path is not used: it would depend on the links' targets at the
map phase only, and a link retargeted since would move the place the post
parse looks at away from the one recorded.

**Invariants**, checked after the other sections' (§8), at the section's
first byte for the first two and at the entry's for the rest; JSON path
`sections.negatives` or `sections.negatives[<i>]`:

| Rule | Text |
|---|---|
| N-1 | the shard's phase is `map`: "a negatives section in a shard whose phase is not map" |
| N-2 | the shard has `inputs` (and so its status is not `failed`): "a negatives section in a shard without inputs" |
| N-3 | each path starts with `/`: "a negative input is not an absolute path" |
| N-4 | the paths are strictly ascending by bytes: "negatives are not strictly ascending" |

The encoding's own rules are `Malformed` at the entry's `shared` field
while decoding: "a shared prefix longer than the path before", "a shared
prefix that is not the longest".

**Determinism.** The section is a function of the tree and the command,
as `inputs` is: the parse's lookups are, and a place's path is text. A
place is written once whatever the number of lookups that missed it, in
path order.

### 8.7 The `names` section

What a unit's names digest is: `digest.md` §9 "The names digest". The map
task writes one entry per key that has one, from the walk of the map
(`names` empty when no unit writes a name-derived string symbolically);
the verify task writes its post-image's entries and its notes.

```
Names     := units: list<NamesEntry>, notes: list<NameNote>
NamesEntry:= key: digest, names: digest
NameNote  := key: digest, file: bytes, name: str
```

`names` of an entry is the key's names digest: its one unit's, or for a
key with several units that have one `H("slip.names.key.v1", count,
digests)` in the units' order. A note is a key whose names digest differs
between the pre shard and the post-image (`verify.md` §4.4), with the
file and name of the post-image's first unit of the key that has one.
JSON: `{"units": [{"key", "names"}], "notes": [{"key", "file", "name"}]}`
(digests as hex, `file` as `bytes` in §2's JSON form).

| Row | Rule | Offset | JSON path |
|---|---|---|---|
| 1 | present exactly when `digests` is | absent: the first byte of the `digests` payload; present without `digests`: the first byte of the section's payload, checked after row 2 | `sections.names` |
| 2 | status `ok` | the first byte of the section's payload | `sections.names` |
| 3 | `units` strictly ascending by `key` | that entry's first byte | `sections.names.units[i]` |
| 4 | no note in a shard of phase `map`, nor in one whose verdict is not `verified` or `mismatch` | the first note's first byte | `sections.names.notes[0]` |
| 5 | `notes` strictly ascending by `key` | that note's first byte | `sections.names.notes[i]` |
| 6 | every note's `key` has an entry in `units` | that note's first byte | `sections.names.notes[i]` |
| 7 | every note's `name` at most 4096 bytes | the `name` field's first byte | `sections.names.notes[i].name` |

Checked in this order, each over its whole list before the next. Row 1 is
the shard's (it names `digests`); rows 2 to 7 the section's. Tests:
`crates/slip_model/tests/sections.rs` `names_1_*` to `names_7_*` and
`names_rules_in_order`.

### 8.8 The `found` section

The **found lookups** of a map shard: each place where the map parse
found a file, with the input the place led to, and which of the shard's
`inputs` were one file at the map phase, so that the verify task can tell
that every place still leads there and that the inputs are still the
same files (guard N2, `verify.md` §5.3; work package WP-135). It is the
positive counterpart of `negatives` (§8.6): N1 sees a search that now
ends earlier, N2 a spelling that now leads elsewhere, through a link
retargeted, and two files made one or one made two by hard links.

```
Found      := places:  list<FoundPlace>
              classes: list<list<uv>>
FoundPlace := shared: uv        bytes in common with the path before (none for the first)
              rest:   bytes     the path's bytes after them
              input:  uv        the index in `inputs` of the file the place led to
```

**The places** are the paths (§8.6, "The path of a place", from the
command's `directory`) of each place of `Lookups::found` and of each name
of `Lookups::read` (the main file, and every file as the front end first
spelled it), unique, in path order, front-coded as `negatives` is. A
place is written once whatever the number of lookups that found a file
there. Kiln's log names the directory and the name it opened for every
found lookup it records, so a place is never recomputed; what it does not
record (a lookup of an absolute name, Kiln WP-2620) is covered by
`Lookups::read` when that spelling is the file's first, and is not
otherwise (`verify.md` §5.1).

**The input of a place** is found when the map task makes the shard,
right after the parse: the input whose path is the place's real path, or
else the one that is the same file as the place (same device and file
number, links followed): a hard link of an input that the front end read
as that input's buffer (Kiln identifies a file by its device and file
number, `SourceManager::load`). A place that leads to none fails the map
task (the tree changed during the parse, the assumption `inputs` already
makes, §8.4): no shard records a place it cannot tie to a file read.

**The classes** partition the inputs that are one file at the map phase
(the same device and file number, links followed): each class lists two
inputs or more, by index, ascending; the classes ascend by their first
input; an input in no class is a file of its own. The device and file
numbers themselves are never written: they mean nothing on another host,
or after a copy. Two inputs are one file at the map phase only when two
consulted paths (an `#embed` resource, a `__has_include` or `__has_embed`
hit the preprocessor did not enter) are hard links of one another: each
real path is an input of its own (§8.4). A hard link of a file the
preprocessor entered is that file's buffer (Kiln's source manager finds
it by device and file number) and no input of its own: its place leads
to the buffer's input.

The JSON form (§10) is `{ "places": [{ "path", "input" }…], "classes":
[[…]…] }`, the paths whole.

**Invariants**, checked after `negatives`'s, at the section's first byte
for the first two, at the place's first byte for F-3 and F-4, at its
`input` for F-5, at the class's first byte (its count) for F-6 and F-7,
and at the member for the rest; JSON path `sections.found`,
`sections.found.places[<i>]`, `sections.found.places[<i>].input`,
`sections.found.classes[<j>]` or `sections.found.classes[<j>][<k>]`:

| Rule | Text |
|---|---|
| F-1 | the shard's phase is `map`: "a found section in a shard whose phase is not map" |
| F-2 | the shard has `inputs`: "a found section in a shard without inputs" |
| F-3 | each path starts with `/`: "a found place is not an absolute path" |
| F-4 | the paths are strictly ascending by bytes: "found places are not strictly ascending" |
| F-5 | each place's input is below the number of inputs: "a found place names no input" |
| F-6 | each class has two inputs or more: "a class of fewer than two inputs" |
| F-7 | the classes are strictly ascending by their first input: "classes are not ascending by their first input" |
| F-8 | each member is below the number of inputs: "a class names no input" |
| F-9 | each class's members are strictly ascending: "a class is not strictly ascending" |
| F-10 | no input is in two classes: "an input in two classes" |

They are checked place by place (F-3 to F-5 for each in turn), then class
by class (F-6, F-7, then F-8 to F-10 for each member in turn). The
encoding's own rules are `negatives`'s, at the place's `shared` field.

**Determinism.** The places are text of the command and the source, as
`negatives`'s; the inputs they lead to and the classes are a function of
the tree, as `inputs` is.

## 9. The run record

One per invocation of a parallel phase; kind `RUNR`, version 1. Not
deterministic. All sections are required. A record names its plan, phase,
round and start time itself, so a reader never takes them from its file
name (`spec/exec.md` §2, `spec/harness.md` §7).

| Id | Section | Payload |
|---|---|---|
| 1 | run | `Run` |
| 2 | jobs | `list<Job>` in completion order |

```
Run := tool_slip:     str
       tool_kiln:     str
       plan:          digest     content id of the plan
       phase:         Phase
       round:         uv         the round of the phase run, 0 for the first (spec/exec.md §2)
       started_unix_ns: uv
       wall_ns:       uv         whole phase
       cores:         uv         cores available to the process
       workers_max:   uv         most workers running at once
       memory_budget: uv         bytes (spec/exec.md); 0 if unlimited
       peak_rss_sum:  uv         largest observed sum of the running workers' resident sizes, bytes; 0 if unknown

Job := observer:   digest
       attempt:    uv           1 for the first try
       end:        End
       shard:      opt<digest>  content id of the shard written, if any
       queued_ns:  uv           from phase start to the job being started
       wall_ns:    uv
       cpu_ns:     uv           user + system of the worker for this job; 0 if unknown
       peak_rss:   uv           bytes; 0 if unknown
       estimate:   uv           bytes the admission controller reserved for it

End := completed = 0            the worker returned a shard (whatever its status)
     | crashed = 1              the worker died or panicked
     | killed_memory = 2        the worker exceeded its memory cap
     | killed_time = 3          the worker exceeded its time limit
     | cancelled = 4            the run was interrupted
```

## 10. JSON rendering

Every container has a JSON rendering, produced by `slip dump <file>` and used
for golden files and hand-written test inputs. `slip_model` converts in both
directions; binary → JSON → binary must reproduce the bytes.

```json
{
  "kind": "PLAN",
  "version": 1,
  "sections": {
    "tool": { "slip": "0.0.0", "kiln": "1.3.0" },
    "configurations": [ … ],
    "observers": [ … ],
    "excluded": [ … ]
  }
}
```

Rules:

- Pretty form (§4). `sections` members are in ascending id order and named as
  in the tables of §7–§9; record members are in field order and named as the
  fields.
- `uv`/`sv`: a number when below 2^53, otherwise a decimal string.
- `digest`: a 64-digit hexadecimal string.
- `bytes` and `path`: a string when the bytes are valid UTF-8, otherwise
  `{ "hex": "…" }` with lower-case hexadecimal digits.
- Enumerations: the names used in this spec (`"cxx"`, `"diagnosed"`).
- `opt`: `null` when absent.
- The trailer is not rendered; converting to binary recomputes it.
- An unknown ignorable section is rendered under the name `"#<id>"` as
  `{ "hex": "…" }`.

When reading JSON: an unknown member, a missing member, or a value of the
wrong type is an error naming the JSON path (`sections.observers[3].main`).

### 10.1 Writing

- An optional section that is absent (the shard's `inputs`, `digests`,
  `verdict`) is not rendered; present and empty is `[]`.
- `"#<id>"` payloads are always in hex form, even when they are valid UTF-8.
- `kind` is the four-letter kind; `version` the number 1.
- The run record's `"run"` has `"round"` right after `"phase"` (§9 field
  order).
- The shard's `"digests"` and `"verdict"` come after `"counters"`. Their
  members are named as the fields of `digest.md` §11 and `verify.md` §6
  (`Finding.where` is `"where"`), in field order, all required. A group's
  members are `"class"`, `"file"`, `"count"`, `"indeterminate"`,
  `"digest"`, `"units"`.

### 10.2 Reading

Only the rendering the writer produces is accepted, so a value has one JSON
form as it has one binary form (§1 rule 1):

- Every object has exactly the members of its record, in field order. An
  unknown member, a missing member and a member out of order (or repeated, in a
  hand-built `Value`) are errors at the member's path. `sections` has its
  members in ascending id order, required sections present.
- `"#<id>"` is accepted only for an id of 64 or above written in canonical
  decimal (no leading zero); any other key starting with `#` is an unknown
  member. Its value must be `{ "hex": "…" }`.
- `uv`: an integer number below 2^53, or a string of decimal digits without a
  leading zero whose value is at least 2^53 and fits 64 bits. A number at or
  above 2^53, a string below 2^53, a float (`1.0`) and a negative number are
  errors.
- `bytes`/`path`: a string, or `{ "hex": "…" }` (exactly that one member) with
  an even number of lower-case hexadecimal digits whose bytes are not valid
  UTF-8. Hex holding valid UTF-8 is an error at `….hex`.
- `digest`: exactly 64 lower-case hexadecimal digits.
- Enumerations: exactly the names of §7–§9, `digest.md` §11 and
  `verify.md` §6.
- `kind` must be the reader's kind (`undump`: one of the three); `version`
  must be 1 (0 or above 1 is an error at `version`).
- Invariant violations are errors at the JSON paths of §7.1 and §8.1–§8.3,
  with the same messages `validate` gives.

## Tests

- `crates/slip_model/tests/wire.rs` — §2: each primitive, every rejection in
  §2.1, the offsets of §2.2, boundary values of `uv` and `sv`.
- `crates/slip_model/tests/blake3.rs` — §3: the official vectors (fixture
  `crates/slip_model/tests/blake3_vectors.txt`).
- `crates/slip_model/tests/json.rs` — §4, §4.1–§4.3.
- `crates/slip_model/tests/container.rs` — §5, §5.3: every rule, one test
  each.
- `crates/slip_model/tests/formats.rs` — §5.4, §5.5, §7–§10: round trips,
  the invariants, golden JSON `crates/slip_model/tests/golden/plan.json`,
  `shard.json` and `run.json` (the `dump` of fixed values).
- `crates/slip_model/tests/sections.rs` — §8.2, §8.3 and their JSON form:
  every invariant with its offset and path, goldens
  `crates/slip_model/tests/golden/shard-digests.json` and
  `shard-verdict.json`.
- `crates/slip_kiln/tests/parse.rs` and the unit tests of
  `crates/slip_kiln/src/parse.rs` — §8.4; the parity test's oracle (the
  parse task of WP-010) is `crates/slip_kiln/tests/support/`. Also
  `stack_remaining` inside the closure, and `__DATE__` and `__TIME__`
  fixed with `SOURCE_DATE_EPOCH` set in the test's environment.
- `crates/slip_digest/tests/map.rs` — §8.5 on observers the test writes
  to a directory of its own: subject paths of inputs, of the predefines, and through a `subject`
  that maps a root; too little stack (`-fkiln-stack-size=`) as `Err`;
  the cause counters and `cause_word` on every cause text of the crate;
  the same shard bytes from two runs; the group files of the
  declarations Kiln makes without a location (`digest.md` §11).
- The unit tests of `crates/slip_cli/src/worker.rs` — the map task: an
  `ok` shard gets `digests` and counters; the walk skipped on a unit with
  errors (also with a stack too small for the walk: `diagnosed`, not
  `Err`); too little stack fails the job; the jobs it refuses.
- `crates/slip_cli/tests/cli.rs` — §8.5 through the executable: `slip
  parse` gives every `ok` shard a `digests` section and the counters; a
  `diagnosed` shard has neither.

Running the tests with `SLIP_BLESS=1` rewrites the golden files (until
`cargo xtask` owns blessing, `spec/harness.md` §6).

## Open questions

- The run record does not forbid two jobs with the same observer and
  attempt, and its reader does not reject them.
- Windows paths and command lines: out of scope until a corpus needs them.
- Which environment variables enter `Command.env`. Candidates: `CPATH`,
  `C_INCLUDE_PATH`, `CPLUS_INCLUDE_PATH`, `SDKROOT`, `SOURCE_DATE_EPOCH`.
  M0 records none and does not apply `Command.env`: the front end sees the
  worker's environment (§8.4), and Kiln reads none of the include-path
  variables (`spec/cdb.md` "Open questions").
- Whether a shard should name the plan it was produced under. Today the store
  layout carries that (`spec/exec.md`).
- Rules that look like invariants and are not checked: `ok` with
  `errors > 0`, `diagnosed` with `errors = 0`, `attempt = 0`, a shard digest
  on a job that did not complete, a relative `path` or `directory`, an
  `entry` beyond its database, an observer's main file missing from its
  shard's `inputs`. They are writer bugs rather than format errors; the M0
  corpus run is where they would show. Decide when M0 closes which become
  rules.
- Property and fuzz tests for the decoders and the JSON reader (round trip,
  no panic on any input). Waiting for a fuzz step in the harness.
- `Decoder::count` bounds a list's count by the bytes remaining, which is
  sound only while every element type encodes to at least one byte. A
  zero-width element type would need its own bound.
- Whether a section must be present: `digests` on every `ok` shard
  (`digest.md` §10) and `verdict` on every `ok` verify shard (`verify.md`
  §6). The map task (§8.5) and the verify job (`verify.md` §5.4) write
  them; the format does not require them, because a verify shard of an
  observer whose post-image does not parse has neither and the readers
  that matter treat absence as "not usable" or `failed` (`verify.md`
  §5.3, §7). Making `digests` required on an `ok` map shard would reject
  every store written before §8.5; decide when the store gets a version
  2. The package that writes verdicts decides the
  consistency rules of a verdict that the format does not check:
  `unverifiable` with `only_pre`, `only_post` or `used` non-zero, or with a
  non-zero `pre`; `verified` with `only_pre`/`only_post` non-zero; `pre` or
  `post` on an unnamed `indeterminate` finding; a finding's `file` or
  `Site.file` empty or not absolute; a `where` on a finding whose kind
  cannot have one; the `digest.*` counters against the section.
- `findings` longer than the default limit of 64, or more than 16 findings
  with a `where` (`verify.md` §6.1), are not rejected: the limit is a
  parameter of the job, not of the format.
- **For Kiln.** `-fkiln-reproducible-time` is on every parse (§8.4), and
  nobody has verified it together with precompiled headers and module
  files. Kiln records the flag in a precompiled header's and a module
  file's snapshot (`kiln_driver/src/pch_snapshot.rs`), so a file the
  build made without it may be refused, or accepted with the build's
  `__DATE__` and `__TIME__` inside it. The flag stays: the LLVM corpus
  uses neither. The question for Kiln, before the first corpus that does:
  what a parse with the flag does with a snapshot made without it, and
  whether a snapshot can carry time-dependent tokens at all.
- The counter namespace `digest.cause.*` (§8.5) is as wide as the walk's
  list of cause texts (about 170 today). It shrinks as rows are decided;
  a baseline that gates on counters exactly (`spec/harness.md` §7.3)
  lists every one that occurs.
