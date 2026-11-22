# model — data model and on-disk formats

Status: draft (M0). Implemented by `slip_model`. This spec defines everything
Slip's phases exchange and store. `slip_model` depends on no other crate and
does no I/O beyond reading and writing byte buffers handed to it.

Sections 2–5 are foundations used by every format. Sections 6–9 are the M0
formats. Entity keys, coverage, edits, facts and digests are added by later
milestones as new sections of the shard (§8), with ids already reserved.

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
| `UnsupportedVersion` | The container version is greater than the reader supports, or zero. |
| `UnknownSection` | A critical section id the reader does not know (§5). |
| `Corrupt` | The integrity trailer does not match. |

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

`slip_model` implements BLAKE3 itself, in safe Rust, with an incremental
interface (`new`, `update(&[u8])`, `finalize() -> Digest`) and a one-shot
function. It must pass the official BLAKE3 test vectors for the hash mode
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

### 5.2 Interface

`slip_model::container` provides a writer (`new(kind, version)`,
`section(id, payload)` in ascending order, `finish() -> Vec<u8>`) and a
reader (`open(bytes, kind, max_version) -> Result<Reader>` performing rule 5,
then `section(id) -> Option<&[u8]>`, and a check that every critical section
present was requested, so an unknown critical id cannot be missed).
`slip_model::wire` provides an `Encoder` over `Vec<u8>` and a `Decoder` over
`&[u8]` for §2, with `finish()` enforcing rule 3.

## 6. Files and paths

A `path` is a `bytes` holding an absolute file-system path, not necessarily
UTF-8.

A **real path** is a path with every symbolic link resolved, as the operating
system reports it (`realpath`). Slip identifies a file by its real path: two
spellings that reach the same file are the same file, and an edit is always
addressed to a real path.

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

## 8. The shard

A shard is what one observer produced in one phase. Kind `SHRD`, version 1.
Its content is deterministic (§1 rule 2): parsing the same inputs twice gives
the same bytes.

| Id | Section | Required | Payload |
|---|---|---|---|
| 1 | header | yes | `observer: digest`, `phase: Phase`, `round: uv` |
| 2 | outcome | yes | `Outcome` |
| 3 | inputs | no | `list<FileRef>` sorted by `path`: every file the front end read, the main file included |
| 4 | counters | yes | `list<Counter>` sorted by `name` |
| 16 | coverage | — | reserved (M2) |
| 17 | edits | — | reserved (M2) |
| 18 | facts | — | reserved (M3) |
| 19 | digests | — | reserved (M1) |
| 20 | reports | — | reserved (M2) |

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

## 9. The run record

One per invocation of a parallel phase; kind `RUNR`, version 1. Not
deterministic. All sections are required.

| Id | Section | Payload |
|---|---|---|
| 1 | run | `Run` |
| 2 | jobs | `list<Job>` in completion order |

```
Run := tool_slip:     str
       tool_kiln:     str
       plan:          digest     content id of the plan
       phase:         Phase
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

## Tests

To be listed as work packages land:

- `crates/slip_model/tests/wire.rs` — §2: each primitive, every rejection in
  §2.1, boundary values of `uv` and `sv`.
- `crates/slip_model/tests/blake3.rs` — §3: the official vectors.
- `crates/slip_model/tests/json.rs` — §4.
- `crates/slip_model/tests/container.rs` — §5: every rule, one test each.
- `crates/slip_model/tests/formats.rs` — §7–§10: round trips, the invariants,
  golden JSON.

## Open questions

- Windows paths and command lines: out of scope until a corpus needs them.
- Which environment variables enter `Command.env`. Candidates: `CPATH`,
  `C_INCLUDE_PATH`, `CPLUS_INCLUDE_PATH`, `SDKROOT`, `SOURCE_DATE_EPOCH`.
  M0 records none and Slip clears them when running the front end.
- Whether a shard should name the plan it was produced under. Today the store
  layout carries that (`spec/exec.md`).
- Rules that look like invariants and are not checked: `ok` with
  `errors > 0`, `diagnosed` with `errors = 0`, `attempt = 0`, a shard digest
  on a job that did not complete, a relative `path` or `directory`, an
  observer's main file missing from its shard's `inputs`. They are writer
  bugs rather than format errors; the M0 corpus run is where they would
  show. Decide when M0 closes which become rules.
- Property and fuzz tests for the decoders and the JSON reader (round trip,
  no panic on any input). Waiting for a fuzz step in the harness.
- `Decoder::count` bounds a list's count by the bytes remaining, which is
  sound only while every element type encodes to at least one byte. A
  zero-width element type would need its own bound.
