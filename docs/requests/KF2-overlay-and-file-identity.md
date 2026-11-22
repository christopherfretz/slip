# KF2 — an overlay the front end reads through, and file identity

A request from Slip to Kiln. Status: **draft, not sent.**
Context: [05-kiln-interface.md](../design/05-kiln-interface.md) §3 (row KF2)
and §5 (Slip prototypes KF2 on a Kiln branch, Kiln reviews and lands it);
[spec/edits.md](../../spec/edits.md) §7.1 and
[spec/verify.md](../../spec/verify.md) §5 are the consumers. Kiln owns the
design of everything inside Kiln; this states what Slip needs, proposes a
shape, and gives acceptance tests. Names below are placeholders.

Line references are to Kiln `main` at `0295475a3` unless marked **pin**,
which is Slip's pin `62d190007` (the head of `slip/kf1`; KF1's
`front_end.rs` is not on `main` yet).

## 0. What kind of work each part is

| Part | Kind | Why |
|---|---|---|
| Stage 1: replacing the content of files that exist (§3.1) | **Implementation against a settled model.** Clang's remapped file buffers and `-ivfsoverlay` are the prior art; Kiln already funnels file reads through two places (§2). | No design decision is open except the two below. |
| What identifies an overlaid file (§3.2) | **Design, small.** Kiln identifies a file by device and inode, clang by `UniqueID`; an overlay entry is given by path. Which wins when they disagree has to be chosen and written down. | A wrong choice is silent: a header reached through a symbolic link would be read from disk. |
| What a module file, a PCH or a lazily loaded buffer does when its source is overlaid (§3.4) | **Design, small to medium.** Kiln's validation of those by size and FNV hash (`manager.rs:810`, `:868`) has to see the overlay, or refuse. | Slip's corpora use neither today, so stage 1 may refuse; the rule still has to be stated. |
| Stage 2: files the overlay creates or removes (§5) | **Design without settled prior art in Kiln.** Directory identity, directory listings, the lookup log and `__has_include` all change. Clang's VFS is prior art for the mechanism, not for Kiln's lookup caching (`search.rs:309`). | Needed for M3 (`create_file`), not for M2. Not part of the prototype. |
| A content hash per file | **No work asked.** Slip hashes the bytes itself (BLAKE3 over `original_bytes`). Kiln's `content_hash` (FNV-1a 64, `kiln_source/src/hash.rs:22`) stays what it is. | The design row asked for one; Slip no longer needs Kiln to provide it. |

## 1. Need

Slip verifies an edit by parsing the edited program and comparing digests.
The edited program must not be written to the user's tree before it is
verified, and several thousand workers must parse it at once. So each
worker gives the front end an **overlay**: a set of `(path, bytes)` that
replaces the content of those files, and nothing else.

What Slip relies on, which is therefore what the tests in §4 check:

1. **Every read is covered.** Whatever route reaches an overlaid file (the
   main file, `#include` by any spelling, `#include_next`, `-include`,
   `#embed`, a header named by a module map) gets the overlay's bytes.
   One route that reads the disk makes Slip verify a program that is not
   the post-image.
2. **Nothing else changes.** A file the overlay does not name is read as
   today. Search order, `#pragma once` and include-guard identity,
   `__has_include`, `__FILE__`, diagnostics' file names and the list of
   files read are those of a tree in which the overlaid files had the new
   content. The reference behaviour is exactly that: the same command over
   a copy of the tree with the files replaced.
3. **The bytes are visible.** `SourceManager::original_bytes` (**pin**
   `manager.rs:1177`) returns the overlay's bytes for an overlaid file, so
   that Slip's record of what was read (`slip_kiln`'s `collect_inputs`,
   `crates/slip_kiln/src/parse.rs:240`) shows the post-image digest.
   `FileOrigin` (**pin** `manager.rs:233`) says the buffer came from an
   overlay.
4. **Per call.** The overlay is a parameter of one front-end run. Two runs
   in one process with different overlays do not see each other's.

## 2. Kiln today

| Fact | Where |
|---|---|
| Header search goes through a file-system trait, `HeaderFs`, with `RealFs` (the disk) and `MemoryFs` (tests) | `kiln_pp/src/search.rs:311`, `:373`, `:445` |
| `HeaderSearch<F: HeaderFs>` is generic over it | `kiln_pp/src/resolver.rs:55`, `:86` |
| The driver always builds it with `RealFs` | `kiln_driver/src/pp_cmd.rs:407`, `:497`, `:685`; `pch_snapshot.rs:675` |
| `RealFs::open` loads a header through the source manager; `RealFs::read` (`#embed`) reads the disk itself | `kiln_pp/src/search.rs:414`, `:419` |
| The main file is loaded by the source manager directly | `kiln_driver/src/pp_cmd.rs:384` |
| Every disk load ends in `SourceManager::load`: open, `fstat`, identity by device and inode, one buffer per identity | `kiln_source/src/manager.rs:763`, `file_key` at `:544` |
| A module file's copy of a header is accepted when the file's size matches, without reading it | `manager.rs:721` (`load_regular_file_expecting`) |
| Lazily loaded buffers and `same_content` validate by length and FNV hash | `manager.rs:810`, `:868` |
| In-memory buffers exist but are not found by inclusion | `manager.rs:795` (`add_virtual`) |
| The front-end entry takes options that are `#[non_exhaustive]` | **pin** `kiln_driver/src/front_end.rs:40` |

So there are two funnels, `SourceManager::load` and `RealFs`, and stage 1
is a hook in each plus the option that carries the overlay to them.

## 3. Proposed shape

A proposal. Kiln owns the final form.

```rust
// kiln_source
/// Replacement contents for files that exist. Keyed by file identity.
pub struct Overlay { /* identity -> (path as given, Arc<[u8]>) */ }

impl Overlay {
    /// `path` must name an existing regular file; it is identified as
    /// `SourceManager::load` identifies it (device and inode on Unix).
    pub fn replace(&mut self, path: &Path, bytes: Arc<[u8]>) -> io::Result<()>;
    pub fn is_empty(&self) -> bool;
}

impl SourceManager {
    /// Before any file is loaded. Loads of an overlaid identity take the
    /// overlay's bytes instead of reading the file.
    pub fn set_overlay(&self, overlay: Arc<Overlay>);
}

pub enum FileOrigin { Disk, Virtual, Lazy, Overlay }      // one more variant

// kiln_driver::front_end
pub struct FrontEndOptions { /* … */ pub overlay: Option<Arc<Overlay>> }
```

### 3.1 Stage 1 behaviour

- `SourceManager::load`, after it has the file's identity, takes the
  overlay's bytes if the identity is overlaid and otherwise reads as
  today. Decoding, the load report and the one-buffer-per-identity rule
  are unchanged. `original_bytes` returns the overlay's bytes.
- `RealFs::read` (the `#embed` route) consults the overlay the same way.
  Directory queries (`dir_identity`, `is_dir`, `is_file`, `list_dir`) are
  untouched: stage 1 changes no directory.
- `load_regular_file_expecting`: for an overlaid identity the expected
  text is not trusted; the overlay's bytes are loaded.
- The buffer's name, and so `__FILE__` and diagnostics, is the path as
  first given, as today.

### 3.2 Identity (the design point of stage 1)

Proposed: an overlay entry is keyed by the **identity of the file the path
names when the entry is added**, not by the path's spelling. Then a header
reached through another spelling, a symbolic link or a hard link is the
overlaid file, which is what a tree with the file replaced would give.
Slip passes real paths; Kiln resolves each once.

To decide: what a platform without device and inode does (`file_key` at
`manager.rs:551` falls back to the path); Slip only needs Unix.

### 3.3 What Slip does not ask

- No change to Kiln's content hash, and no hash in the interface.
- No overlay for directories, for files that do not exist, or for
  removing a file (stage 2).
- No command-line flag. A `-fkiln-overlay=<file>` for Kiln's own harness
  is Kiln's choice (§4 would use it).
- No lazy contents in stage 1. An overlay of a large transform names far
  more files than one unit reads; Slip loads the bytes of the files the
  unit read before, and parses again if the unit then reads another
  overlaid file (`spec/edits.md` §10.3). An entry whose bytes are fetched
  on first read (`replace_with(path, loader)`) would save the second
  parse; it is wanted with stage 2, not before.

### 3.4 Caches that outlive a file's content

A PCH, a module file and the lazy buffers validate their sources by size
and hash against the disk. Under an overlay the disk is not the truth.
Proposed for stage 1: a run whose overlay names a file that a PCH or a
module file of the command lists as an input is **refused** with a
diagnostic (a rejected command, as KF1's unsupported modes are), and the
lazy route (`manager.rs:810`) treats an overlaid identity as `Changed`.
The proper rule (validate against the overlay's bytes) can follow when a
corpus needs it.

## 4. Acceptance tests

Stage 1. Each is one front-end run through KF1's entry, compared with the
same command over a copy of the tree in which the files were replaced
(the reference of §1 point 2): equal diagnostics, equal token count,
equal AST dump, and the same list of files read up to the root.

1. The main file overlaid.
2. A header overlaid, included as `"h.h"`, `<h.h>`, through a second
   search directory with `#include_next`, and by `-include`.
3. The same header reached by two spellings and through a symbolic link:
   one buffer, the overlay's content.
4. `#pragma once` and an include guard in an overlaid header: included
   twice, entered once.
5. An overlay that adds an `#include` of an existing file, and one that
   removes one: the files read change accordingly.
6. `__has_include` of an overlaid header: unchanged (the file exists).
7. `#embed` of an overlaid file: the overlay's bytes.
8. `original_bytes` of an overlaid buffer is the overlay's bytes;
   `FileOrigin` is `Overlay`; a file not overlaid is `Disk`.
9. An overlay with invalid UTF-8 and with a byte-order mark: handled as
   the same bytes on disk are (the load report says so).
10. A command with a PCH whose input is overlaid: refused, with the
    diagnostic.
11. Two runs in one process, the second without the overlay: it reads the
    disk.
12. An empty overlay: byte-identical results to no overlay, and Kiln's
    performance gates do not move (the cost is one map lookup per load).

## 5. Staging

| Stage | Content | Needed by |
|---|---|---|
| 1 | §3.1 to §3.4, tests of §4 | Slip M2: the verify job of every transform |
| 2 | Created and removed files: directory listings, directory identity, the lookup log (`kiln_pp/src/lookups.rs:79`, `:237`), `__has_include` | Slip M3 (`create_file`); also lets an unchanged observer stay unchanged when a file is created (`spec/verify.md`, "Created files and pruning") |
| 3 | PCH and module files validated against the overlay instead of refused | when a corpus builds with them |

## 6. The prototype

Slip prototypes **stage 1 only**, on a Kiln branch `slip/kf2`, and Kiln
reviews and lands it as a work package.

- **Base.** The branch must contain KF1 (`kiln_driver/src/front_end.rs`,
  today only on `slip/kf1`). If KF1 has landed on `main` by then, the base
  is `main`; if not, Kiln names the commit. Slip does not rebase
  `slip/kf1` itself.
- **Scope.** `kiln_source` (`Overlay`, `set_overlay`, the hook in `load`
  and in `load_regular_file_expecting`, `FileOrigin::Overlay`, the lazy
  route), `kiln_pp` (`RealFs::read`), `kiln_driver` (`FrontEndOptions`,
  the refusal of §3.4), and the tests of §4. Expected under 600 lines with
  tests.
- **Not in the prototype.** Stage 2 and 3; any flag; any change to header
  search or to the hash.
- **Slip's side** (`docs/wp/WP-066.md`): `slip_kiln` passes the overlay to
  `run_front_end` and its own tests repeat §4 points 1, 2, 5 and 8 through
  `parse_observer_with`.
