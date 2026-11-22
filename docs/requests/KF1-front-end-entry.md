# KF1 — front-end library entry: what M0 found missing

Status: notes for the prototype (Slip prototypes KF1 on the Kiln branch
`slip/kf1`; Kiln reviews and lands it). The request itself is row KF1 of
[05-kiln-interface.md](../design/05-kiln-interface.md).

M0's parse task (`crates/slip_kiln`, WP-010) reaches Kiln only through
`kiln_driver`'s public API: `args::Args::parse_as`, `run_as` and `Io`. It
works, by going around these gaps. Each is something the KF1 entry point
should provide, with the workaround it retires.

| # | Missing | Today's workaround in `slip_kiln` | Consequence |
|---|---|---|---|
| a | An embeddable entry that does what the private `entry::compile` does: a compile thread with the requested stack, `StackGuard::install`, panic containment. | Slip starts the thread itself (`parse.rs` `run_front_end`); the stack guard is in `kiln_sema` and is not installed. | Deep recursion aborts the worker (one `failed` observer) where Kiln would give a fatal diagnostic. |
| b | The files read, returned from the run: path as found, real path, size, and the content digest of the bytes the front end saw. | `-MD -MF <scratch file> -MT slip.d`; the Make rule is un-escaped (`depfile.rs`) and every file is read again and hashed after the parse. | A file edited during the parse gets a digest Kiln never saw. Make escaping cannot represent a name ending in a backslash or holding a newline. A command with its own `-Wp,-M…` is refused. Kiln writes no rule after a missing `#include`, so a `diagnosed` shard has no inputs. |
| c | Diagnostic totals and the first error, returned from the run. | `-fdiagnostics-format=json` appended; standard error is read as JSON lines (`diags.rs`). | A hand-written reader of another crate's output format; a command ending in an option that swallows the appended flag is refused. |
| d | A deterministic preprocessor token count (`pp.tokens`). | Not produced. | The M0 baseline lacks its most useful size counter. |
| e | A working directory per run (`-working-directory`, or a field of `Io`). | `std::env::set_current_dir`, never restored. | One parse per process at a time; any relative path elsewhere in the process is affected. |
| f | A job's environment as a parameter. | The front end reads the worker's process environment (`KILN_BACKEND`, `KILN_LLVM_DIR`, `PATH`, `KILN_RESOURCE_DIR`, `SOURCE_DATE_EPOCH`, `KILN_REPRODUCIBLE_TIME`, `KILN_STATS_LOG`, `KILN_SNAP_TIMING`). | `Command.env` cannot be applied; the coordinator removes `KILN_*` from the worker's environment instead. |
| g | "Which language would this input be compiled as, or none": `Lang` has only C and C++, and an Objective-C or unknown extension compiles as C++ under `-fsyntax-only`. | Slip keeps Kiln's extension table (`check.rs` `KILN_SOURCE_EXTENSIONS`). | The table can drift from Kiln's. |
| h | A way to forbid side outputs under `-fsyntax-only` (`-ftime-trace`, `-fconstexpr-report=`, the statistics log). | None: such a command is accepted and Kiln writes where the command says. | Slip can write into the subject's build tree. Until this exists the options should be refused by the check (open question in `spec/cdb.md`). |

A study of the driver at the pinned revision
(`research/kiln-front-end-entry.md` in the context repository, with
citations) adds, for the prototype:

- After the parse returns, the source manager, the interner, the
  diagnostics engine, the AST and Oven (`ParsedTu`), the preprocessor and
  the target are all alive in `SyntaxJob::run`; a test-only hook already
  observes them there. `Sema` has been consumed by then. A closure-based
  `run_front_end(argv0, command_line, options, |&FrontEnd| …)` in
  `kiln_driver` needs almost no visibility change; keeping `Sema`, a
  working directory parameter, an overlay, and an environment parameter
  are separate steps.
- `StackGuard::install` is public: Slip can install it today (gap a, in
  part).
- The parser's token count exists and is dropped (gap d); a record of
  failed include lookups exists and is switched on only for precompiled
  headers; files of the source manager cannot be enumerated from outside
  its crate (gap b); file text is kept decoded, so "the bytes the front
  end saw" needs care.
- `Toolchain::new` looks for LLVM and GCC (environment, file system, child
  processes) even under `-fsyntax-only`, and the default resource
  directory comes from `current_exe()`, which in a worker is Slip.
- Kiln does not read `CPATH`, `C_INCLUDE_PATH` or `CPLUS_INCLUDE_PATH`: a
  build that relies on them is parsed with other include paths than its
  compiler used. A question for Kiln, not for this entry point.
- Code generation mutates the AST (`&mut Ast`, cached constants): a digest
  is only comparable between two syntax-only runs.

Not gaps: driver-mode detection from `argv[0]`, `-x`, and command-line
validation are all reachable (`Args::parse_as`) and are what `KilnCheck`
uses.

Measured through this route (release build, one process, a translation unit
including `<vector> <string> <map> <algorithm> <memory> <iostream>`,
`-std=c++17`, 247 files, 3.26 MB): 0.26–0.28 s per parse including hashing,
peak resident size 36–38 MB.

## Found since (2026-11-22, `slip_kiln::snippet`, WP-021)

A front end over files in memory, for tests, has to build the language
options of `kiln -std=<std> -fsyntax-only` itself. Three things it could
not take from Kiln and copies or leaves out:

- `SyntaxJob::parse_options`, `Job::pp_options` and
  `Job::configure_preprocessor` are private: the snippet front end
  mirrors them field by field. Public forms would remove the copy.
- `StagedFeatures` is private to `kiln_driver`, so `__has_builtin`
  answers from the unstaged `ClangFeatures`.
- `ParseOptions::complex_promoted` is computed by comparing a
  `kiln_codegen` type; an accessor on `Args` would do.

## Prototype handed over (2026-11-22, WP-020)

Kiln branch `slip/kf1`, head `62d190007`. Open with Kiln for its
review: the corpus against clang 22; `drive()` mirroring `run_as` and
`run_jobs` by hand; a module-cache build inside the job still writing
to the user's cache, and its engine's diagnostics not counted in the
report; the renumbering of PCH snapshot format 117.


## Asked next (2026-11-22, after WP-025)

Slip's parse task now runs on the entry (`spec/model.md` §8.4). What it
still works around, in the order it matters:

1. **A working directory and an environment as parameters**
   (`FrontEndOptions`). Today Slip calls `std::env::set_current_dir`
   before each parse, which is process state: one parse at a time per
   process, and `Command.env` is not applied at all.
2. **The bytes, or size and a digest, of each `consulted` file** (an
   `#embed` resource, a `__has_embed` hit). Kiln read them; Slip reads
   them a second time for the input list, and a file changed in between
   is not detected.
3. **The PCH probe off** (an option): `-include h` with `h.pch` beside
   it is turned into `-include-pch` and then refused with
   `drv_front_end_unsupported`; a tooling client wants the header
   parsed as text.
