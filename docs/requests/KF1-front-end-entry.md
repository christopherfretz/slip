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
| f | A job's environment as a parameter. | The front end reads the worker's process environment (`KILN_STATS_LOG`, `KILN_BACKEND`, `CPATH`, …). | `Command.env` cannot be applied; the coordinator removes `KILN_*` from the worker's environment instead. |
| g | "Which language would this input be compiled as, or none": `Lang` has only C and C++, and an Objective-C or unknown extension compiles as C++ under `-fsyntax-only`. | Slip keeps Kiln's extension table (`check.rs` `KILN_SOURCE_EXTENSIONS`). | The table can drift from Kiln's. |
| h | A way to forbid side outputs under `-fsyntax-only` (`-ftime-trace`, `-fconstexpr-report=`, the statistics log). | None: such a command is accepted and Kiln writes where the command says. | Slip can write into the subject's build tree. Until this exists the options should be refused by the check (open question in `spec/cdb.md`). |

Not gaps: driver-mode detection from `argv[0]`, `-x`, and command-line
validation are all reachable (`Args::parse_as`) and are what `KilnCheck`
uses.

Measured through this route (release build, one process, a translation unit
including `<vector> <string> <map> <algorithm> <memory> <iostream>`,
`-std=c++17`, 247 files, 3.26 MB): 0.26–0.28 s per parse including hashing,
peak resident size 36–38 MB.
