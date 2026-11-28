# cdb — compilation databases and the plan

Status: draft (M0). Implemented by `slip_cdb`. Depends on `slip_model` only;
the front end is reached through the trait of §6, so everything here is
tested without Kiln.

This spec defines how a build description becomes the plan
(`spec/model.md` §7): reading a database (§2), splitting command lines (§3),
expanding response files (§4), normalising a command (§5), checking it
against the front end (§6) and building the plan (§7).

## 1. Principles

1. **The build is the authority.** Slip compiles each translation unit as its
   build does. Normalisation removes only what cannot affect the meaning of
   the source: where outputs go, and how the compiler was launched. Every
   other argument is kept, in order, as spelled.
2. **Every entry is accounted for.** An entry becomes an observer or is
   excluded with a reason. Nothing is dropped silently, and one bad entry
   never fails the database.
3. **Fail in the plan, not in the run.** A command the front end will refuse
   is found here.

## 2. Reading a JSON compilation database

The file must be JSON (`spec/model.md` §4) whose top-level value is an array.
Anything else is the error `Database` (with the file and the reason) and no
plan is produced. The reason is `not JSON: <error>` (the error of
`spec/model.md` §4.1, with line, column and byte; invalid UTF-8 is one of
these) or `the top level is <type>, not an array`. `<type>`, here and in the
details below, is one of `null`, `a boolean`, `a number`, `a string`,
`an array`, `an object`.

Each element is an *entry*, numbered from zero. An entry is well formed when
it is an object with:

| Key | Type | Meaning |
|---|---|---|
| `directory` | string, an absolute path | Working directory of the command |
| `file` | string | The main source file, absolute or relative to `directory` |
| `arguments` | array of strings, not empty | The command line, already split |
| `command` | string | The command line as one string, split by §3 |
| `output` | string | Ignored |

`directory` and `file` are required, and at least one of `arguments` and
`command`; when both are present `arguments` is used. Other keys are ignored.
An entry that is not well formed, or whose `command` cannot be split, is
excluded with reason `malformed_entry` and a `detail` saying why.

- **Order of checks** for one entry, the first failure decides: not an
  object; `directory` missing or not a string; `directory` not absolute
  (`Path::is_absolute`, so the empty string is relative); `file` missing or
  not a string; then the command line.
- **`arguments` wins strictly.** When `arguments` is present it alone is
  used: if it is malformed (not an array, empty, a non-string element) the
  entry is excluded even when `command` would split. When `arguments` is
  absent, `command` must be a string and is split by §3.
- `output` and every unknown key are ignored whatever their type.
- The exclusion's `file` is the entry's `file` whenever that is a string,
  whatever the defect (so a bad `directory` still reports the file); empty
  otherwise.
- Detail texts: `` the entry is <type>, not an object ``,
  `` `<key>` is missing ``, `` `<key>` is <type>, not a string ``,
  `` `directory` `<d>` is not an absolute path ``,
  `` `arguments` is <type>, not an array ``, `` `arguments` is empty ``,
  `` `arguments[<i>]` is <type>, not a string ``,
  `` neither `arguments` nor `command` is present ``,
  `` `command` cannot be split: <message> (byte <offset>) `` (§3).

Readers for other formats implement the same interface (a sequence of
entries, each well formed or carrying its defect) and share everything from
§4 on.

### 2.1 Interface

```rust
// slip_cdb::database
pub fn read_json(bytes: &[u8]) -> Result<Vec<Result<Entry, Defect>>, DatabaseError>;
pub struct Entry { pub directory: String, pub file: String, pub arguments: Vec<String> } // arguments never empty
pub struct Defect { pub file: String, pub detail: String }   // file: the entry's `file`, or empty
pub struct DatabaseError { pub detail: String }
```

One element per entry, in order; an entry's number is its index.

## 3. Splitting a command string

A `command` string is split into words as follows. This is the behaviour of
clang's JSON compilation database for the cases build systems produce, made
strict.

- Words are separated by one or more of space, tab, newline, carriage return.
  Other whitespace is an ordinary character.
- Outside quotes, a backslash makes the next character literal.
- `"` starts a double-quoted part, which runs to the next unescaped `"`.
  Inside it a backslash makes the next character literal; every other
  character is itself.
- `'` starts a single-quoted part, which runs to the next `'`. Every
  character inside is itself.
- Quoted and unquoted parts that touch form one word (`-D"A B"'C'` is the
  word `-DA BC`). An empty quoted part makes an empty word (`""`).

Errors (the entry is `malformed_entry`): a quote that is never closed; a
backslash as the last character; a string with no words.

Each error carries a byte offset into the input and a message:

| Error | Offset | Message |
|---|---|---|
| a double quote never closed (a backslash immediately before the end of a quoted part leaves it unclosed) | the opening quote | `double quote is never closed` |
| a single quote never closed | the opening quote | `single quote is never closed` |
| a trailing backslash | the backslash | `backslash is the last character` |
| no words | 0 | `command has no words` |

`slip_cdb::split` provides `split_command(&str)` (this section) and
`split_response(&str)` (§4), each returning `Result<Vec<String>, SplitError>`
with `SplitError { offset: usize, message: &'static str }`.

## 4. Response files

An argument of the form `@path`, other than `argv[0]`, is replaced by the
words of the file `path` (relative paths are relative to the entry's
`directory`). The file is split with the GNU response-file rules: as §3,
except that inside double quotes a backslash is literal unless followed by
`"` or `\`. Replacement is recursive to a depth of 16.

- `@` alone names no file and is kept as an ordinary argument.
- Relative paths at every depth are relative to the entry's `directory`, not
  to the response file naming them (clang's default differs).
- An empty response file (no words) is not an error: it contributes no
  words. §3's "no words" error applies to a `command` string only.
- Depth: a response file named on the command line is at depth 1; one named
  inside it at depth 2. Depth 16 is accepted, 17 is `malformed_entry`. A
  cycle therefore ends as "nested deeper than 16".

Errors (`malformed_entry`): the file cannot be read; it is not valid UTF-8;
its quoting is malformed; nesting exceeds 16.

## 5. Normalisation

Input: `directory`, the argument list after §4, and `file`. Output: a
`Command` (`spec/model.md` §7) and the real path of the main file, or an
exclusion.

**Step 1: launchers.** While the base name of the first argument is one of
`ccache`, `sccache`, `distcc`, `icecc`, `gomacc`, and at least one argument
follows, remove the first argument. The first remaining argument is the
compiler, `argv[0]`, kept as spelled. An empty argument list (after §4) is
`malformed_entry`.

**Step 2: compiler kind.** If the base name of `argv[0]`, lower-cased and
without a trailing `.exe`, is `cl` or `clang-cl`, the entry is excluded with
`unsupported_compiler`.

**Step 3: scan.** The arguments after `argv[0]` are classified left to right
using the tables below. An argument that is a *separate-value option* takes
the next argument as its value, whatever that argument looks like; if there
is none the entry is `malformed_entry`. An argument that does not begin with
`-`, and `-` alone, is an *input* unless taken as a value.

Removed (they say where output goes, or only shape diagnostics output):

| Form | Options |
|---|---|
| alone | `-c` `-S` `-emit-llvm` `-MD` `-MMD` `-MP` `-MG` `-MV` `-pipe` `-fcolor-diagnostics` `-fno-color-diagnostics` |
| with a separate value, removed together | `-o` `-MF` `-MT` `-MQ` `-MJ` `--serialize-diagnostics` `-dependency-file` |
| joined | `-o<x>` `-MF<x>` `-MT<x>` `-MQ<x>` `-MJ<x>` (any non-empty `<x>`), `--output=<x>`, `-fdiagnostics-color=<x>` (any `<x>`, empty included), `-fdiagnostics-color` |

Kept, and known to take a separate value (so the value is not mistaken for an
input or an option):

`-I` `-D` `-U` `-F` `-L` `-l` `-B` `-x` `-u` `-T` `-z` `-G` `-arch` `-target`
`-include` `-imacros` `-isystem` `-isysroot` `-iquote` `-idirafter` `-iprefix`
`-iwithprefix` `-iwithprefixbefore` `-iframework` `-isystem-after`
`-cxx-isystem` `-internal-isystem` `-internal-externc-isystem`
`--sysroot` `--param` `--include` `--include-directory` `--define-macro`
`--undefine-macro` `--language` `--target` `--gcc-toolchain`
`-Xclang` `-Xpreprocessor` `-Xassembler` `-Xlinker` `-Xanalyzer` `-mllvm`
`-working-directory` `-std` `-fmodule-file` `-fmodule-map-file`
`-ivfsoverlay` `-include-pch` `-stdlib++-isystem` `-MACHINE`

An option in this list written with its value attached (`-Iinc`,
`--sysroot=/x`, `-std=c++17`) is one argument and takes nothing. An option
takes a separate value only when the argument is exactly the option's
spelling.

Every other argument beginning with `-` is kept as one argument.

One argument is classified by the first of these that applies: it is exactly
an option removed with a separate value; it is exactly a kept option with a
separate value; it is removed alone or joined; it is an input; otherwise it
is a kept option.

**Step 4: the action.** If any of `-E`, `-M`, `-MM`, `--version`, `-v` alone
with no input, `--help`, `-###`, `-print-…`/`--print-…` (any argument with
either prefix) is present, the entry is excluded with `not_a_compile`. The
kept arguments contain no action option; the front-end adapter adds its own.
Only kept options are inspected, never values.

**Step 5: inputs.** These checks run in this order; the first that fails
decides.

1. If there is no input, the entry is `not_a_compile`. If there is more than
   one, it is `malformed_entry` ("several inputs").
2. If the input's extension, as spelled and compared exactly, is one of `.o`
   `.obj` `.a` `.so` `.dylib` `.lib` `.s` `.S` `.asm` `.ll` `.bc` `.rc`, the
   entry is `not_a_compile`. (Before the file is looked for: a link step is
   not a compile whether or not its object file has been built.)
3. The input, resolved against `directory`, must exist (`missing_file`
   otherwise). Any failure to resolve its real path is `missing_file`, with
   a detail saying "does not exist" when the file is not found and giving
   the OS error otherwise. The real path is spelled as its directories
   list it (`edits.md` §2.1, "One spelling"), whatever case `directory`
   and the input are written in.
4. Its real path must equal the real path of `file` resolved against
   `directory` (`malformed_entry`, "file does not match the command",
   otherwise), both spelled on disk. A `file` whose real path cannot be
   resolved does not match.

**Result.** `Command.directory` is the entry's `directory` as given;
`Command.argv` is `argv[0]` followed by the kept arguments in their original
order (options, values and the input, each exactly as spelled);
`Command.env` is empty.

Normalisation is idempotent: normalising `Command.argv` again returns it
unchanged.

### 5.1 Interface

```rust
// slip_cdb::normalise: §4, then §5
pub fn normalise(directory: &str, arguments: &[String], file: &str) -> Result<Normalised, Exclusion>;
pub struct Normalised { pub command: Command, pub main: PathBuf }   // main: real path of the main file
pub struct Exclusion { pub reason: Reason, pub detail: String }
```

The option tables of this section are constants of `slip_cdb::normalise`.

## 6. The front-end check

```rust
pub trait FrontEndCheck {
    /// Would the front end accept this command, and as what language?
    /// `command` is normalised (§5); `main` is the real path of its main file.
    fn check(&self, command: &Command, main: &[u8]) -> Result<Language, Rejection>;
}
pub enum Rejection {
    UnsupportedLanguage(String),   // detail: the language
    UnsupportedFlag(String),       // detail: the first flag refused
}
```

`slip_kiln` implements it with Kiln's own command-line parser, which is the
authority on both questions. `slip_cdb` provides `ByExtension`, used when no
front end is linked and in tests: it accepts every flag and determines the
language from the last `-x`/`--language` value before the input if there is
one, otherwise from the input's extension. The values are those of the
compiler driver:

| Language | `-x` values |
|---|---|
| `c` | `c` `c-header` `cpp-output` |
| `cxx` | `c++` `c++-header` `c++-cpp-output` |
| `objc` | `objective-c` `objective-c-header` `objective-c-cpp-output` |
| `objcxx` | `objective-c++` `objective-c++-header` `objective-c++-cpp-output` |

Any other value (including `none`) is `UnsupportedLanguage`. The extensions:

| Language | Extensions |
|---|---|
| `c` | `.c` |
| `cxx` | `.cc` `.cp` `.cpp` `.cxx` `.c++` `.C` `.CPP` `.cppm` `.ixx` `.ii` |
| `objc` | `.m` |
| `objcxx` | `.mm` `.M` |

and treats `.c` as `cxx` when the base name of `argv[0]`, after removing a
trailing `-<digits and dots>`, ends in `++`. Any other extension is
`UnsupportedLanguage` (detail `.<ext>`, or `` input `<path>` has no
extension ``).

Further rules of `ByExtension`:

- `main` is not read. The input is found with the §5 step 3 scan, and the
  extension is that of the input as spelled in `argv`.
- The language values are `-x <v>`, `-x<v>`, `--language <v>` and
  `--language=<v>`; one that is the value of another option is not a
  language.
- A command with no input, or with several inputs, is `UnsupportedLanguage`
  (`the command has no input`, `the command has several inputs`); one whose
  scan fails (a separate-value option with no value) is
  `UnsupportedFlag(<option>)`.

### 6.1 `KilnCheck`: the command line checked

`slip_kiln::KilnCheck` parses, with `kiln_driver::args::Args::parse_as`,
exactly the command line the parse task gives Kiln's front-end library
entry (`spec/model.md` §8.4):

```
argv[0]  -fsyntax-only  -fkiln-reproducible-time  argv[1..]
```

`-fsyntax-only` and `-fkiln-reproducible-time` (`spec/model.md` §8.4)
come first, where no argument of the command can take either as its
value. The entry writes nothing and renders nothing, so the
command's own `-MD`, `-MF`, `-MT`, `-MP`, `-MJ`, `-o` and
`-fdiagnostics-format=` do not matter to the parse and are not checked; a
later phase option (`-c`, `-S`, `--precompile`) leaves the action
`-fsyntax-only`, as in clang.

`argv[0]` is the program name, except that a name Kiln's driver would run as
its dependency scanner (`kiln_driver::scan_deps::is_scanner_name` of its
base name: `kiln-scan-deps`, `clang-scan-deps`, `clang-scan-deps-<v>`) is
replaced by `kiln`, which has the driver mode such a name has (C mode).

Response files are not expanded: §4 already did. `main` is not read and not
compared with Kiln's input.

### 6.2 `KilnCheck`: rejections

In this order; the first that applies decides.

1. An argument that is not UTF-8 (Kiln's command line is text):
   `UnsupportedFlag`, detail the argument decoded lossily.
2. The first `ArgError` Kiln reports as an error (every variant except
   `DeprecatedArg`, which Kiln reports as a warning):
   - `-x <v>` with a value Kiln does not compile (`language not recognized`,
     including `objective-c`, `objective-c++`): `UnsupportedLanguage`,
     detail `<v>`;
   - any other: `UnsupportedFlag`, detail the argument of the command as
     written: the first argument equal to the option Kiln names (option and
     value joined when the option ends in `=`), else the first one that
     starts with it, else that reconstructed spelling. (An option taking a
     separate value, left without one at the end of the command, is
     `MissingValue` here.)
3. The front-end library entry would refuse the command, decided from
   Kiln's public `Args` fields as the entry's own checks read them:
   - what it refuses without running (`NotSyntaxOnly`): the action is not
     `-fsyntax-only` (`-E`, `-fsyntax-only` being overridden only by an
     earlier phase), `--help`, `-###` (`print_commands`),
     `-print-resource-dir`, a toolchain probe (`-dumpversion`,
     `-dumpmachine`, `-print-*`: `ProbeArgs::any`), `-M`/`-MM`
     (`deps.only`), or a `-fmodule-header` input that is not built
     (`Args::module_header_job` other than `Build`);
   - standard input as the one input (`-`; `NotOneInput`);
   - what it refuses with `drv_front_end_unsupported` when written on the
     command line: `-verify` (`Args::verify`), `-include-pch`
     (`pch.include`).
   - Slip's own refusal (not the entry's): `-working-directory`
     (`Args::working_directory`, WP-148: changing the front end's working
     directory from the command line would make relative lookup entries
     disagree with `Command.directory`); a flag that reads a file or
     names a search directory Slip's lookup guards do not model
     (`spec/verify.md` §5.3): `-imacros` (`Args::imacros`), `-iprefix`
     (`Args::iprefix`), `-iwithprefix`, `-iwithprefixbefore`
     (`Args::prefixed_dirs`), each with its aliases; and Kiln's
     tooling-client recording, dump and verification flags
     (`-fkiln-written-ref-keys`, `-fkiln-static-init-facts`,
     `-fkiln-fn-names`, `-fkiln-dump-fn-names`, `-fkiln-builtin-literals`,
     `-fkiln-verify-subst-failed`, `-fkiln-dump-friend-forms`;
     `slip_kiln::check::not_modelled`). Kiln's driver refused them as
     unknown arguments at earlier pins; Slip keeps refusing them (WP-163,
     WP-148, WP-173), and its parse refuses them too (`rejected`, before
     Kiln runs).

   `UnsupportedFlag`, detail the argument responsible: the last argument
   of the shortest prefix `argv[1..=k]` of the command that is refused (in
   a prefix with no single input yet, a `-fmodule-header` counts as
   refused), or, when the prefix before it reports `MissingValue` for the
   argument before it, that option (`-include-pch <f>` names
   `-include-pch`). The check and the parse agree: a command `KilnCheck`
   accepts is never `rejected` by the parse for one of these reasons. Not
   covered, since the check reads no file: an `-include <h>` that Kiln's PCH
   probe turns into `-include-pch <h>.pch` because that file exists
   (`rejected` by the parse with `drv_front_end_unsupported`).
4. Kiln finds no input, or more than one: `UnsupportedLanguage`
   (`Kiln finds no input in the command`, `Kiln finds N inputs in the
   command`).
5. The input's language:
   - an assembler input (`.s`, `.S`, `.sx`, `-x assembler`,
     `-x assembler-with-cpp`): `UnsupportedLanguage`, detail `assembler` or
     `assembler-with-cpp`;
   - with no `-x` applying to it, an extension Kiln does not type as C or
     C++ (`kiln/spec/driver.md` "Input languages": `.c` `.h` `.i` `.cc` `.cp`
     `.cpp` `.CPP` `.cxx` `.c++` `.C` `.ii` `.cppm` `.ixx` `.hh` `.hpp` `.hxx`
     `.H` `.inc`): `UnsupportedLanguage`, detail `objective-c` for `.m`,
     `objective-c++` for `.mm` and `.M`, `` input `<path>` has no
     extension ``, else `.<ext>`. Kiln itself compiles any other extension
     as C++ under `-fsyntax-only`, where the build's compiler would compile
     Objective-C or treat the file as a linker input; such a parse would not
     observe the build.
6. Otherwise the language is Kiln's (`Args::input_lang`: the last `-x`
   before the input, else the extension and the driver mode): `c` or
   `cxx`. The driver mode is Kiln's (`DriverMode::from_program_name`): the
   base name without `.exe`, then without a trailing `-<digits and dots>`,
   ending in `++` is C++ mode, where `.c`, `.h` and `.i` are C++; a
   `--driver-mode=` overrides it.

## 7. Building the plan

Input: an ordered list of (configuration name, database path), a
`FrontEndCheck`, and the tool versions. Configuration names must be unique
and non-empty (`Usage` error otherwise). `Usage` is checked before any
database is read: no database at all, an empty configuration name, or a name
given twice.

1. Read each database (§2). Record its `FileRef`.
2. For each entry in order: §4, §5, then §6. A failure at any step excludes
   the entry with the corresponding reason.
3. Compute `ObserverId`. Entries with the same id are one observer; each
   adds an `Origin`.
4. For each observer record the main file's `FileRef` (size and content
   digest read once per real path) and set `cost_hint` to the main file's
   size.
5. Order the observers and the exclusions as `spec/model.md` §7 requires,
   check the accounting invariant, and return the plan.

Reading and hashing the main files is the only part worth parallelising; it
may use threads. The result must not depend on how many.

- **Database errors.** A database file is read whole, then its real path is
  resolved; failure of either is the error `Database` (`cannot be read:
  <os error>`, `its real path cannot be resolved: <os error>`), as are the
  two §2 errors (their detail unchanged). The error carries the database
  path as given. The first failing database, in the order given, is
  reported. The configuration's `FileRef` is the real path (spelled on
  disk, `edits.md` §2.1), the number of
  bytes read and the content digest of those bytes.
- **Merging.** Entries are keyed by `ObserverId`. The first entry's
  normalised main path and language are kept for the observer.
- **Hashing.** Each distinct real main path is read once, in pieces of
  `READ_CHUNK` (64 KiB), and its size is the number of bytes read (not file
  metadata). A main file that cannot be opened or read (a directory, say)
  makes every origin of every observer naming it an exclusion with reason
  `missing_file`, each keeping its own entry's `file` as spelled, with
  detail `` main file `<real path>` cannot be read: <os error> ``.
- **Threads.** `min(threads, distinct main files)` scoped threads claim files
  by an atomic counter; results are stored by index, so the plan is the same for every
  `threads`. 0 and 1 hash on the calling thread.
- `Excluded.detail` is, for a rejection by the check, the rejection's text
  unchanged (`-fbad`, `.f90`); for a normalisation exclusion, its detail.

### 7.1 Interface

```rust
// slip_cdb::plan
pub fn build(sources: &[Source], check: &dyn FrontEndCheck, tool: &Tool, threads: usize)
    -> Result<Plan, PlanError>;
pub struct Source { pub name: String, pub path: PathBuf }   // configuration name, database file
pub struct Tool { pub slip: String, pub kiln: String }
pub enum PlanError {
    Usage(String),
    Database { path: PathBuf, detail: String },   // path as given
}
pub const READ_CHUNK: usize = 64 * 1024;
```

The plan is a function of: the database files' contents, the contents of the
response files they name, the existence, real paths and contents of the main
files, the front-end check, and the tool versions.

## Tests

- `crates/slip_cdb/tests/split.rs` — §3 and the GNU variant of §4: one case
  per rule and per error.
- `crates/slip_cdb/tests/normalise.rs` — §4 expansion and §5: each table
  row, each step, each exclusion, idempotence; commands taken from real
  CMake output for LLVM.
- `crates/slip_cdb/tests/check.rs` — §6: `ByExtension`, each table row.
- `crates/slip_cdb/tests/database.rs` — §2: well-formed entries, each defect.
- `crates/slip_cdb/tests/plan.rs` — §7: the accounting invariant, merging of
  equal commands across configurations, order, determinism across thread
  counts; golden JSON plans (`spec/model.md` §10)
  `crates/slip_cdb/tests/golden/plan_mixed.json` and `plan_merged.json`
  (`SLIP_BLESS=1` rewrites them). The goldens replace observer ids by
  `"<observer id>"` (after checking each against `Command::observer_id`),
  each database's `size` and `content` by `"<size>"` and `"<digest>"`, and
  the fixture root by `$ROOT`.
- `crates/slip_kiln/tests/check.rs` — §6.1, §6.2: `KilnCheck`.

## Open questions

- clang's database reader tolerates trailing commas and non-string scalars;
  this reader is strict. Relax only if a real build system needs it.
- The compiler named by `argv[0]` decides predefined macros and default
  include directories in the real build. Kiln finds a toolchain its own way.
  How Slip detects and reports a mismatch between the two belongs to the
  front-end adapter and is not specified yet.
- Kiln reads no include-path environment variable (`CPATH`,
  `C_INCLUDE_PATH`, `CPLUS_INCLUDE_PATH`, `SDKROOT`), on purpose. A
  database does not record the environment of the build, so a build that
  depends on one of them parses differently under Slip and nothing says
  so. A warning, or a refusal, when one is set where `slip plan` runs is
  the candidate rule.
- Entries with several inputs, and `-x` appearing more than once.
- Windows command lines (`cl`, and the Windows splitting rules).
- The joined form `-o<x>` also matches clang's `-objcmt-…` migration options
  and `-object`. Neither affects the meaning of a translation unit, so
  removing them is harmless; a kept-options exception list is needed only if
  a driver option beginning `-o` that matters turns up.
- `ByExtension` reads `argv` as text, lossily for bytes that are not UTF-8.
  Paths in a JSON database are always text, so this matters only to a future
  database format.
- A launcher written as `env VAR=value compiler …` is not recognised; the
  entry is excluded as `malformed_entry` (several inputs).
- Kiln's front-end library entry writes no side output
  (`-ftime-trace`, `-fconstexpr-report=`, `-MD`, …: `spec/model.md` §8.4):
  its job runs with `no_side_outputs` (Kiln `kiln_driver/src/front_end.rs:547`,
  `args.rs:747-751`), under which the `std` module cache is only read, a
  valid entry used and nothing built or pruned. The parse of a unit that
  does `import std` therefore depends on whether a cache entry already
  exists on the host, which is not an input of the shard. `KilnCheck` does
  not refuse `import std`; no corpus of M1 uses modules.
- Entries with the same observer id whose main file or language differ can
  only arise if the file system changes while the plan is built. The first
  entry's are kept; the change is not detected.
- A database is read whole and parsed to a tree before any entry is used,
  and entries are normalised and checked on one thread. A database of
  hundreds of megabytes needs a streaming reader and a parallel check; the
  plan format does not change.
