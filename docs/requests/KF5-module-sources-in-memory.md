# KF5 — a named module imported from a source held in memory

A request from Slip to Kiln. Status: **sent 2026-11-23** (Slip WP-048; drafted for the
supervisor, sent on the operator's word). Kiln owns the design of everything inside Kiln; this
states what Slip needs, proposes a shape, and gives acceptance tests.
Names below are placeholders. It is a new row of
[05-kiln-interface.md](../design/05-kiln-interface.md) §3 (front end,
next to KF1 and KF2).

Line references are to Kiln `main` at `913a490bb`, Slip's pin.

## 0. What kind of work each part is

For Kiln's coordinator, which hands design without prior art to its
design agent and implementation against a settled specification to its
implementers:

| Part | Kind | Why |
|---|---|---|
| §3.1: a named module's interface source given as a buffer already in the source manager, beside `ModuleOptions::module_files` | **Implementation against Kiln's own specification.** Kiln `spec/modules.md` §1 (in-process build from source) and §2 (the lookup order, item 1, `-fmodule-file=<name>=<path>` naming a source) already say what an import of a source does; only where the source's text comes from is new. §5 is the acceptance. | One branch in `import_module` and one field; no rule of ownership, visibility or ordering changes. |
| §3.2: whether such a buffer can also be a module file | **Design, small, with prior art** (clang's `-fmodule-file=` takes a file only). Proposed: no; a buffer is always a source. | Slip needs only sources. |
| §4: a header unit's key for an in-memory buffer | **Implementation, small (a defect).** Kiln `spec/modules.md` §11.6 makes the key `"` plus the header's canonical path; for a buffer that is not a file it has none, and today the host's file system is asked for one. | Slip's snippet front end must not read the host's file system. |

## 1. Need

Slip's test front end (`slip_kiln::snippet`) parses a translation unit
whose files are all in memory: no file system, no working directory, many
parses at once on many threads (`crates/slip_kiln/src/snippet.rs`,
"Threads"). Its includes go through Kiln's `IncludeResolver` over those
files. Slip's digest has rows for modules (`spec/digest-kinds.md` §10.7:
`UnitKind` `Implementation`, `UnitInfo` `imports`, `Owner` `unit` other
than 0, `ModuleTable` `explicit_defs` across units) that only a translation
unit importing a named module can test. A snippet cannot import one: the
interface source would have to be a file on disk, and Slip's tests never
write one (a test writes nothing, and two parses at once would race on
it).

What Slip relies on, which is what §5 checks:

1. **The same unit as from disk.** A module whose interface source is a
   buffer is imported exactly as `-fmodule-file=<name>=<path>` imports the
   same text from `<path>` (Kiln `spec/modules.md` §1, §2): the same
   `ModuleTable`, the same owners, the same diagnostics but for the file
   name, which is the buffer's name.
2. **Nothing read from the host.** No `open`, `stat` or `canonicalize` of
   any path for a module given so, nor for a partition or a module it
   imports when those are given so too.
3. **Per run.** The buffers are the run's; nothing is cached across runs.

## 2. Kiln today

| Fact | Where |
|---|---|
| An import builds the unit in-process from its source; no module file is written | Kiln `spec/modules.md` §1 (lines 11-19); `ModuleOptions::record` only for module output, `kiln_driver/src/pp_cmd.rs:551-555` |
| `module_files` holds `(name, PathBuf)` | `kiln_pp/src/modules.rs:31-33` |
| `find_module_where` returns a path: `module_files`, then prebuilt directories by `Path::is_file`, then recorded paths, then libc++'s directory | `modules.rs:720-755` |
| `import_module` sniffs the path (`std::fs::File::open`) and loads it with `SourceManager::load_file` (disk) | `modules.rs:773`, `:792`, `:824`; `kiln_pp/src/bmi.rs:628-630`; `kiln_source/src/manager.rs:791`, `:854` |
| `module m;` imports `m` through the same function | Kiln `spec/modules.md` §3 "Performing an import" |
| In-memory buffers exist and are never found by path | `manager.rs:883-887` (`add_virtual`) |
| A header unit is found through the `IncludeResolver`, so in-memory headers work | `modules.rs:683`, `:699`, `:1316` |
| ... but its key is computed with `std::fs::canonicalize` of the buffer's name before any module file is looked for | `modules.rs:1472-1477`, `:1636-1638` |
| The only in-memory module input is the main one (`enter_main_capsule(path, bytes)`) | `modules.rs:1209` |

KF2 (the overlay) does not cover this: its stage 1 replaces the content
of files that exist, and a snippet's module source does not exist on disk.
Its stage 2 (files the overlay creates) would, if `bmi::sniff` and
`load_file` in `import_module` read through the overlay; that is design
for M3. This request is the small, self-contained part.

## 3. Proposed shape

A proposal. Kiln owns the final form.

```rust
// kiln_pp::ModuleOptions
/// Interface sources already in the source manager: module `<name>`
/// (`M:P` for a partition) is built from buffer `FileId`, as a source
/// named by `module_files` would be. Looked up first.
pub module_buffers: Vec<(String, FileId)>,
```

### 3.1 Behaviour

- `import_module(key, span)`: after `already_built`, if `module_buffers`
  names `key` (the last entry of that name, as for `module_files`), call
  `begin_unit(key, false, file, false, span, None)` and return. No sniff,
  no load, no record entry of a path (a module file built from such a unit
  under `record` either refuses or records the buffer's name: Kiln's
  choice; Slip never records).
- Everything after `begin_unit` is as today: the unit's own includes go
  through the `IncludeResolver`, its own imports through `import_module`
  (so a partition or another module given as a buffer is found the same
  way), a cycle is `pp_module_cycle`.
- The driver never sets the field; `kiln` and `run_front_end` are
  unchanged.

### 3.2 A buffer is a source

A buffer is never read as a module file, whatever its first bytes.

An alternative with the same effect: a defaulted method on
`IncludeResolver`, `fn resolve_module(&mut self, sm, name) ->
Option<FileId>`, asked before `find_module_where`. It lets the embedder
add the buffer lazily; Slip's snippet front end would use either.

## 4. Header units: no host lookup for an in-memory header

`header_unit_file` (`modules.rs:1472`) computes
`header_unit_key(self.sm.file_name(file))` before it looks in
`module_files`, and `header_unit_key` canonicalises the name against the
working directory (`modules.rs:1636-1638`). For a buffer from
`add_virtual` the name is not a path on the host: the lookup is a read of
the host's file system with no meaning, and if a file of that name happens
to exist in the working directory, a header-unit module file given for
that file would be taken for the in-memory header.

Smallest reproducer (Slip's snippet front end, any test thread):

```rust
let files = [("main.cpp", "import \"h.h\";\nint y = X;\n"), ("h.h", "#define X 1\n")];
let s = Snippet { files: &files, main: "main.cpp", include: &[""], std: "c++20" };
slip_kiln::snippet::parse(&s, |p| p.errors)  // 0, and `<cwd>/h.h` is looked up
```

Proposed: when `sm.is_virtual(file)`, the key is `"` plus the buffer's
name as it is, with no call to the file system (or `header_unit_file`
returns `None` at once when `module_files` and the replays name no header
unit).

## 5. Acceptance tests (in Kiln)

With a `Preprocessor` over a `SourceManager` holding only `add_virtual`
buffers and a resolver over them, from C++20:

1. `main`: `import m; int y = f();`, `m.cppm` as buffer for `m`:
   `export module m; export int f() { return 1; }`. No diagnostic; unit 1
   is `Interface` `m`; `f`'s owner is unit 1, exported.
2. A unit that imports a partition: `m.cppm` is `export module m; export
   import :p;`, `m-p.cppm` for `m:p` is `export module m:p; export int
   g();`; `main` imports `m` and calls `g`.
3. `module m;` as the main file with `m` given as a buffer: an
   `Implementation` unit importing `m`.
4. The same parse, with the buffers written to a temporary directory and
   named by `module_files` instead: the same `ModuleTable` and the same
   diagnostics but for file names.
5. A module given as a buffer with an error in it: the diagnostic names
   the buffer's name.
6. Tests 1 to 3 and a header-unit import (`import "h.h";`) run with a
   working directory that holds a file `h.h`, `m.cppm` and `m-p.cppm` of
   other content: the result does not change (no host lookup, §1 item 2
   and §4).

## 6. What Slip does with it

`slip_kiln::snippet` gains its module units (`docs/wp/WP-048.md` step 3,
a constructor or a wrapper so that no existing `Snippet { … }` changes),
adds each module source with `add_virtual` and names it in
`module_buffers`; the pair runner's `// module: <name>=<file>` lines
(already read, `crates/slip_digest/tests/common/mod.rs`) reach it, and
the pending tests of WP-048 (`spec/landed/harness/WP-048.md`) run.
