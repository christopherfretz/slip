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
plan is produced.

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

Readers for other formats implement the same interface (a sequence of
entries, each well formed or carrying its defect) and share everything from
§4 on.

## 3. Splitting a command string

A `command` string is split into words as follows. This is the behaviour of
clang's JSON compilation database for the cases build systems produce, made
strict.

- Words are separated by one or more of space, tab, newline, carriage return.
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

## 4. Response files

An argument of the form `@path`, other than `argv[0]`, is replaced by the
words of the file `path` (relative paths are relative to the entry's
`directory`). The file is split with the GNU response-file rules: as §3,
except that inside double quotes a backslash is literal unless followed by
`"` or `\`. Replacement is recursive to a depth of 16.

Errors (`malformed_entry`): the file cannot be read; it is not valid UTF-8;
its quoting is malformed; nesting exceeds 16.

## 5. Normalisation

Input: `directory`, the argument list after §4, and `file`. Output: a
`Command` (`spec/model.md` §7) and the real path of the main file, or an
exclusion.

**Step 1: launchers.** While the base name of the first argument is one of
`ccache`, `sccache`, `distcc`, `icecc`, `gomacc`, and at least one argument
follows, remove the first argument. The first remaining argument is the
compiler, `argv[0]`, kept as spelled.

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
| joined | `-o<x>` `-MF<x>` `-MT<x>` `-MQ<x>` `-MJ<x>` (any non-empty `<x>`), `--output=<x>`, `-fdiagnostics-color=<x>`, `-fdiagnostics-color` |

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

**Step 4: the action.** If any of `-E`, `-M`, `-MM`, `--version`, `-v` alone
with no input, `--help`, `-###`, `-print-…`/`--print-…` (any argument with
either prefix) is present, the entry is excluded with `not_a_compile`. The
kept arguments contain no action option; the front-end adapter adds its own.

**Step 5: inputs.** If there is no input, the entry is `not_a_compile`. If
there is more than one, it is `malformed_entry` ("several inputs"). The one
input, resolved against `directory`, must exist (`missing_file` otherwise),
and its real path must equal the real path of `file` resolved against
`directory` (`malformed_entry`, "file does not match the command",
otherwise). If the input's extension, compared exactly, is one of `.o`
`.obj` `.a` `.so` `.dylib` `.lib` `.s` `.S` `.asm` `.ll` `.bc` `.rc`, the
entry is `not_a_compile`.

**Result.** `Command.directory` is the entry's `directory` as given;
`Command.argv` is `argv[0]` followed by the kept arguments in their original
order (options, values and the input, each exactly as spelled);
`Command.env` is empty.

Normalisation is idempotent: normalising `Command.argv` again returns it
unchanged.

## 6. The front-end check

```rust
pub trait FrontEndCheck {
    /// Would the front end accept this command, and as what language?
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
one (`c`, `c++`, `objective-c`, `objective-c++`, and the `-header` and
`-cpp-output` variants of each map to the base language; anything else is
`UnsupportedLanguage`), otherwise from the input's extension:

| Language | Extensions |
|---|---|
| `c` | `.c` |
| `cxx` | `.cc` `.cp` `.cpp` `.cxx` `.c++` `.C` `.CPP` `.cppm` `.ixx` `.ii` |
| `objc` | `.m` |
| `objcxx` | `.mm` `.M` |

and treats `.c` as `cxx` when the base name of `argv[0]`, after removing a
trailing `-<digits and dots>`, ends in `++`. Any other extension is
`UnsupportedLanguage`.

## 7. Building the plan

Input: an ordered list of (configuration name, database path), a
`FrontEndCheck`, and the tool versions. Configuration names must be unique
and non-empty (`Usage` error otherwise).

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

The plan is a function of: the database files' contents, the contents of the
response files they name, the existence, real paths and contents of the main
files, the front-end check, and the tool versions.

## Tests

- `crates/slip_cdb/tests/split.rs` — §3 and the GNU variant of §4: one case
  per rule and per error.
- `crates/slip_cdb/tests/normalise.rs` — §5: each table row, each step, each
  exclusion, idempotence; commands taken from real CMake output for LLVM.
- `crates/slip_cdb/tests/database.rs` — §2: well-formed entries, each defect.
- `crates/slip_cdb/tests/plan.rs` — §7: the accounting invariant, merging of
  equal commands across configurations, order, determinism across thread
  counts; golden JSON plans (`spec/model.md` §10).

## Open questions

- clang's database reader tolerates trailing commas and non-string scalars;
  this reader is strict. Relax only if a real build system needs it.
- The compiler named by `argv[0]` decides predefined macros and default
  include directories in the real build. Kiln finds a toolchain its own way.
  How Slip detects and reports a mismatch between the two belongs to the
  front-end adapter and is not specified yet.
- Entries with several inputs, and `-x` appearing more than once.
- Windows command lines (`cl`, and the Windows splitting rules).
