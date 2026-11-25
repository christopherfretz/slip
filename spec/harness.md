# harness — `cargo xtask`

Status: draft (M0). Implemented by `xtask`. The harness is specified to the
same standard as the tool: a re-implementation of Slip needs a
re-implementable definition of "passes".

`xtask` is a workspace member run through the alias `cargo xtask`. It is
`#![forbid(unsafe_code)]`, links none of the crates that link Kiln, and
reaches `slip` only as a built executable. Its only dependencies are the
workspace crates `slip_model`, `slip_exec` and `slip_verify` (path
dependencies through `[workspace.dependencies]`; `slip_verify` links no
Kiln and depends on `slip_model` only, and gives the transform runner the
one rule of whether a result permits writing, §6.3), and through
`slip_exec` `signal-hook`, which §3.1 allows.

M0 defines the gate (§2), the lints (§3), the work-package table (§4), the
jobserver commands (§5), performance records (§7) and corpora (§8).
§11 is the probe of a file system that confuses the names of a
hard-linked file, and what the harness relies on of one.
Transform tests (§6) are outlined here and specified with M2. M1 adds the
corpus invariants (§9) and the first demonstration (§10); the pitfall
tests are specified in `spec/pitfalls.md` §1.

## 1. Principles

1. **One definition of the gate.** `cargo xtask ci` is what "the gate" means
   locally and in any CI system; CI configuration only chooses slices.
2. **Lints run first** and need no build of anything but `xtask`.
3. **Nothing is skipped silently.** A test that cannot run for lack of a
   capability (a tool, a corpus) is reported as `SKIP` with the reason, and
   is an `ERROR` when the environment variable `SLIP_REQUIRE_ALL=1` is set.
   A transform test reports a missing capability as an oracle skipped
   (§6.4), with the same rule.
4. **Tight gates on deterministic numbers, loose on time.** Counters are
   compared exactly; time and memory are recorded and reviewed as trends
   (§7).
5. **Shared machine.** The harness starts its commands with
   `std::process::Command`, not through `slip_exec::process`, and takes no
   jobserver token for them. `cargo` and `make` are jobserver clients: they
   inherit the environment and take their own tokens through `MAKEFLAGS`.
   `git` and `cmake` (§8) run without a token ("Open questions").

## 2. The gate

```
cargo xtask ci [--quick] [--step <name>]… [--keep-going]
```

Steps, in order. A failing step stops the gate unless `--keep-going`.

| Step | What | `--quick` |
|---|---|---|
| `lint` | §3 | yes |
| `fmt` | `cargo fmt --all --check` | yes |
| `clippy` | `cargo clippy --all-targets -- -D warnings` over the selected crates | yes |
| `test` | `cargo test` over the selected crates | yes |
| `kiln` | the same clippy and test over the crates that link Kiln | no |
| `transform` | transform tests (§6) | no |

The *selected crates* are the workspace's default members (the crates that
do not link Kiln). `--quick` is the implementer's bar before handback for
work in those crates; work in a crate that links Kiln runs the full gate.
`--step` runs only the named steps, in gate order; it may repeat, and a
step named twice runs once. With `--quick` and `--step` together the named
steps run and each must be a quick step: `--quick --step kiln` is a usage
error. An unknown step, a `--step` without a name or an unknown option is a
usage error: `xtask ci: <message>` and the usage text on standard error,
exit status 2, nothing runs.

- `lint` runs in-process over the repository root and prints its
  violations (§3), or `lint: <error>` when the files cannot be listed.
- `fmt`, `clippy` and `test` run the commands of the table without `-p`,
  so cargo takes the default members.
- `kiln` runs the same clippy and test with
  `-p slip_kiln -p slip_digest -p slip_script -p slip_refactor -p slip_cli`,
  the test only if clippy passed.
- `transform` runs `cargo xtask transform` (§6.3) over every transform
  test and fails if any outcome is `FAIL`, `XPASS`, `ERROR` or `KILLED`.
  Until the runner exists it prints
  `transform: no transform tests yet (spec/harness.md §6)` and passes.

Cargo is `$CARGO` if set and non-empty, else `cargo` on `PATH`, run in the
repository root with the inherited environment and output. A cargo that
cannot be started fails the step with
``cannot run `<cargo> <args>`: <error>``.

Each step prints `==> <name>` when it starts and `<name>: ok (<seconds>s)`
(seconds with one decimal, `fmt: ok (8.4s)`) or `<name>: FAILED` when it
ends. The gate's exit status is 0 only if every step that ran passed, else
1; the last line is `ci: ok` or `ci: FAILED (<steps>)`, the failed steps
joined by `, ` in gate order.

Three gating tiers use this command (`docs/design/06-engineering-plan.md`
§3): *commit* is `ci --quick`; *merge* is `ci` plus the corpus slice
(`perf check` of §7.3 on a map run of the corpus, and from M1 the
invariants of §9);
*release candidate* adds full corpora, oracles and performance budgets, and
is specified with M5.

## 3. Lints

`cargo xtask lint` runs all of these over the repository root (`cargo
xtask` uses the checkout it was built from) and reports every violation on
standard output, one per line, as `<path>[:<line>]: <lint>: <message>`:
lints in the order of the table below and, within a lint, by path then
line. Nothing is printed when there is none. With any, standard error gets
`xtask lint: <n> violation(s)` and the exit status is 1. An argument after
`lint` is a usage error, exit status 2.

| Lint | Rule |
|---|---|
| `kiln-boundary` | Only `crates/slip_kiln/Cargo.toml` names a `kiln_*` dependency. Only `slip_kiln`, `slip_digest`, `slip_script`, `slip_refactor` and `slip_cli` depend on `slip_kiln`, directly or indirectly. |
| `forbid-unsafe` | Every crate root (`src/lib.rs`, `src/main.rs`, each `.rs` file under `src/bin/`) of every workspace member contains `#![forbid(unsafe_code)]`. |
| `dependencies` | Every third-party crate in `Cargo.lock` is in the allow list in this spec (§3.1). |
| `spec-index` | `spec/README.md`'s index: every `spec/*.md` other than `README.md` has a row; every row whose status is not `not written` names an existing file; the status is `not written`, `draft` or `stable`. |
| `spec-shape` | Every `spec/*.md` other than `README.md` contains the headings `## Tests` and `## Open questions`. |
| `landed` | Every file under `spec/landed/` is `<spec>/WP-NNN.md` (or `<spec>/WP-NNN-<part>.md`) for an existing `spec/<spec>.md`, starts with `# WP-NNN — `, and `docs/wp/WP-NNN.md` exists. |
| `wp` | Every `docs/wp/WP-*.md` follows `docs/wp/README.md`: the title line with the same id as the file name; `Status`, `Milestone`, `Track`, `Needs`, `Size` lines with allowed values; `Needs` names existing files. |
| `local-paths` | No committed text file contains an absolute path into a home directory or this host's workspace (`/home/`, `/Users/`, `/root/`, `/Volumes/`). |
| `doc-links` | Every relative Markdown link in `docs/`, `spec/` and `README.md` points at an existing file. |
| `api-tests` | Every directory under `crates/slip_script/tests/api/` with a `transform.cpp` has `t.cpp` and `expect.txt`, lies in no other, and its `// spec:` line names a heading of `spec/api.md`; no other file lies there (`spec/api.md` §11). |

Lints read the files tracked by git, so build output and worktrees are
never scanned. If the root holds `.git` (a directory, or a file as in a
worktree), the file list is `git ls-files --stage`; gitlink entries
(submodules such as `kiln`) are not files and their contents are not
listed. Otherwise (a fixture tree) the tree is walked, skipping only
`.git`; symbolic links are listed as files and not followed. Failing to
list files is an error: exit status 1 with `xtask lint: <reason>`.

### 3.1 Allowed third-party crates

| Crate | Used by | Why |
|---|---|---|
| `signal-hook` and its dependencies `libc`, `signal-hook-registry` | `slip_exec` (and `xtask` through it) | Returning jobserver tokens and killing children on `SIGINT`/`SIGTERM` (`spec/exec.md` §7). The handler's `unsafe` stays inside the dependency. |

Kiln's own dependencies (reached through the `kiln` submodule's path
dependencies) are allowed as Kiln's lock file lists them.

The list above is repeated in `xtask/src/lint/crates.rs`; a change is made
in both places.

### 3.2 Lint rules

- **Workspace members** are the directories with a listed `Cargo.toml` that
  match the root manifest's `[workspace] members` and no `exclude` pattern
  (`*` matches within one path component), plus the root if its manifest has
  a `[package]`.
- **`kiln-boundary`.** Dependencies are the keys of `[dependencies]`,
  `[dev-dependencies]`, `[build-dependencies]` and those tables under
  `[target.<cfg>]`, in both the `name = …` / `name.workspace = true` and the
  `[dependencies.name]` forms; a `package = "…"` rename names the real
  crate. The root's `[workspace.dependencies]` declares crates and is not a
  dependency, so the root may list `kiln_*` there. "Directly or indirectly"
  follows dependencies of every kind between members, so a dev-dependency
  path to `slip_kiln` counts. Messages:
  ``names Kiln crate `<crate>`; only crates/slip_kiln/Cargo.toml may`` at the
  dependency's line, and
  `` `<crate>` depends on `slip_kiln` (<a> -> … -> slip_kiln); only
  slip_digest, slip_script, slip_refactor and slip_cli may`` at the line of
  the first hop.
- **`forbid-unsafe`.** A root passes when a line, after leading spaces,
  starts with `#![forbid(unsafe_code)]`. Every `.rs` file under `src/bin/`,
  at any depth, counts as a root. A `[lib]` or `[[bin]]` `path` outside
  these places is not followed.
- **`dependencies`.** A third-party crate is a `Cargo.lock` package with a
  `source`; a workspace member has none, so `xtask`'s dependencies on
  `slip_model`, `slip_exec` and `slip_verify` are not third-party. Allowed: the §3.1
  crates at any version, and every package with a `source` in
  `kiln/Cargo.lock` at the same name and version. The Kiln lock is read
  from disk (it is in the submodule); when it cannot be read the message
  ends `(kiln/Cargo.lock cannot be read)`. No `Cargo.lock` is no violation.
  Message: ``third-party crate `<name> <version>` is not allowed by
  spec/harness.md §3.1``.
- **`spec-index`.** The index is the first Markdown table after the line
  `## Index` (outside fenced code) up to the next `## ` heading. The first
  column names the file (a link's target, else the text without
  backticks) relative to `spec/`; the last is the status, of which a
  trailing parenthesised note is not part (`not written (M1)` is
  `not written`). Specs are listed `spec/*.md` files other than
  `spec/README.md`, not files in subdirectories.
- **`spec-shape`.** The headings must be whole lines outside fenced code.
- **`landed`.** The title's id must be the file name's id. `spec/README.md`
  is not a spec a landed file can name. `<part>` names one brief of a
  package the supervisor split (`WP-091-queries.md`): lower-case ASCII
  letters, digits and `-`, neither empty nor ending in `-`; the id is the
  `WP-NNN` before it.
- **`wp`.** An id is `WP-` and three or more digits. Lines 3 to 7 are
  exactly `- Status:`, `- Milestone:`, `- Track:`, `- Needs:`, `- Size:` in
  that order and line 2 is blank (`docs/wp/README.md`: "The first seven
  lines are checked"). Allowed values: status `todo`, `in-progress`,
  `review`, `done`, `dropped`; milestone `M` and digits; track `A` to `H`;
  size `S`, `M`, `L`; needs `none` or comma-separated ids with files.
  `Parallel with` is not checked.
- **`local-paths`.** Kiln's detector with Slip's four prefixes: a path
  starts at the start of a line, after a character that cannot be in a
  path, or after a one-letter flag (`-I`), not after `>`, `}`, `)`, `$`, `*`;
  a bare prefix (followed by a quote, a backtick or the end) is not a path.
  Binary files (NUL byte, or not UTF-8) are skipped. **No file is exempt**:
  the rule's text names only bare prefixes, and the lint's tests build
  their local paths with `concat!` of a bare prefix and a name, so the
  committed tree spells none and `cargo xtask lint` passes on itself.
  Message: ``absolute local path `<path>` ``.
- **`doc-links`.** Documents are listed `*.md` files under `docs/` and
  `spec/`, and `README.md`. Links are inline links and images (angle-bracket
  targets accepted) and reference definitions, outside fenced code and
  inline code. Targets with a scheme, `mailto:`, a leading `#` or a leading
  `/` are not relative and not checked. The `#anchor` and `?query` are
  dropped, `%20` is decoded, and the target is resolved against the
  document's directory; it must be a listed file, a directory containing
  listed files, or a path inside a submodule that exists on disk. A target
  above the root fails. Anchors are not checked. Message:
  ``link `<target>` points at no file``.

## 4. Work packages

`cargo xtask wp` prints the work packages in `docs/wp/` as a table: id,
status, milestone, track, size, needs, title; ordered by id.
`cargo xtask wp --ready` lists those with status `todo` whose needs are all
`done`.

The table has a header row `id status milestone track size needs title`,
columns padded to the widest cell and separated by two spaces, trailing
spaces trimmed. `needs` is the ids joined by `, `, or `none`. Ids order
numerically (`WP-999` before `WP-1000`). `--ready` prints the same table,
header included even when empty. A file that fails the `wp` lint makes the
command print those violations to standard error and exit 1; any other
argument is a usage error, exit 2.

## 5. Jobserver

```
cargo xtask jobserver status [--fifo <path>] [--jobs <n>]
cargo xtask jobserver refill [--fifo <path>] [--jobs <n>]
```

As `spec/exec.md` §3 "Inspection". Neither command blocks.

- `status` prints `jobserver: <fifo>: <idle> of <n> tokens idle` and exits
  0. It changes nothing.
- `refill` tops the pool up to `<n>` idle tokens and prints
  `jobserver: <fifo>: <idle> idle, <added> added`. It is only correct while
  no client holds a token; the command cannot know that and says so in its
  usage text.

The line goes to standard output, the fifo's path as given or resolved
(`Path::display`).

- **Options.** `--fifo <path>` and `--jobs <n>`, each at most once, in any
  order, value as the next argument. A missing value, an empty `--fifo`, a
  repeated option, any other argument, and a missing or unknown command are
  usage errors: `xtask jobserver: <message>` and the usage text on standard
  error, exit 2.
- **The fifo** is `--fifo`, else `slip_exec::jobserver::resolve_fifo`: the
  last `--jobserver-auth=` word of `MAKEFLAGS` if it is `fifo:<path>` (a
  descriptor form names no fifo and falls through), else a non-empty
  `$SLIP_JOBSERVER_FIFO`, else `/run/slip/jobserver.fifo`. A `MAKEFLAGS`
  that is not UTF-8 is ignored, as `default_fifo` ignores it.
- **`<n>`** is `--jobs`, else `$SLIP_JOBS`, else 8
  (`slip_exec::jobserver::DEFAULT_POOL_SIZE`). It is ASCII digits only (no
  sign, no spaces), not 0, fitting `usize`. A bad `--jobs` is a usage error
  (exit 2) ``--jobs: `<v>` is not a positive integer``; a bad `$SLIP_JOBS`,
  including an empty one, is an error (exit 1)
  ``SLIP_JOBS: `<v>` is not a positive integer``, and one that is not UTF-8
  is the error ``SLIP_JOBS `<v>`: not UTF-8``. This is stricter than
  `slip_exec::jobserver::resolve_pool_size`, which trims and accepts 0.
- **Checks before opening** (exit 1, `xtask jobserver: <path>: <what>`):
  `does not exist`; `is not a fifo` (symbolic links followed; a directory
  or regular file is never opened); `cannot stat: <error>`. A failure of
  `status` or `refill` itself is `cannot read the pool: <error>` or
  `cannot refill: <error>`.

### 5.1 Interface

`xtask::jobserver`:

```rust
pub enum Failure { Usage(String), Error(String) }               // exit 2, exit 1
pub struct Request { pub refill: bool, pub fifo: PathBuf, pub jobs: usize }
pub fn main(args: &[String], env: &dyn Fn(&str) -> Option<OsString>) -> ExitCode;
pub fn run(args: &[String], env: &dyn Fn(&str) -> Option<OsString>) -> Result<String, Failure>;
pub fn parse(args: &[String], env: &dyn Fn(&str) -> Option<OsString>) -> Result<Request, Failure>;
pub fn inspect(request: &Request) -> Result<String, String>;
```

The environment is a parameter, so tests never read the host's own.

## 6. Transform tests

A transform test is a directory: an input tree, a transform, and the
expected result (post-image, report, exit status) as golden files; §6.2
gives the format. Outcomes follow Kiln's model: `PASS`, `FAIL`, `SKIP`
(a test's declared `require:` host capability is unavailable; does not
fail the gate unless `SLIP_REQUIRE_ALL=1`, which turns it into `ERROR`),
`XFAIL` (`xfail: <reason>` required), `XPASS` (fails the gate), `ERROR`
(the test itself is wrong: a bad directive, a pre-image the front end or
clang rejects), `KILLED` (a run ended by a signal or its timeout, §6.3;
fails the gate, never masked by `xfail`). A test without a `require:`
directive always runs, and what it cannot check for lack of a capability
is an oracle skipped (§6.4), counted and named. Unknown directives are
errors. Every test names the spec section it covers.

**Golden files.** A golden is the JSON rendering (`spec/model.md` §10) or the
exact bytes of an output. `--bless` rewrites goldens and prints each path it
changed; it never runs in the gate.

### 6.1 The digest's pair tests

The pair tests of `slip_digest` (`crates/slip_digest/tests/{differ,equal,demand,indeterminate}/`,
the forms of `spec/digest-kinds.md` §1.5) parse each side with
`slip_kiln::snippet`: Kiln's front end over the test's files held in
memory, as `kiln -std=<std> [--target=<t>] <args> -fsyntax-only` would,
with no predefined macro, no header search beyond the test's own files and
no file system (`crates/slip_kiln/src/snippet.rs`, "Language options",
lists what else differs from that command line). The default line is
`-std=c++20` for `x86_64-unknown-linux-gnu`. A side's main file (`t.cpp`
for both sides, or each side's `a.cpp` / `b.cpp`) changes it with header
lines, in any order, among its leading lines:

```
// std: c++26
// errors: yes
// target: aarch64-unknown-linux-gnu
// args: -fcf-protection=branch -frounding-math
```

- The header block is the main file's leading lines that are header lines
  or `#include "side.h"`; it ends at the first line that is neither. A
  header line is exactly `// <key>: <value>`, with the keys `std`,
  `errors` (value `yes`), `target` (one word) and `args` (split at
  spaces, no quoting). A line of the block that looks like one
  (`//`, blanks, a word of letters and `-`, blanks, `:`) but is not one
  (`// arg:`, `//target:`, `// std:c++26`, a key given twice) fails the
  test; a comment that does not look like one ends the block. A header
  line anywhere after the block (after a blank line, a comment, a
  byte-order mark, a `/* */`) fails the test: it would not be read.
- `target` is a triple Kiln's driver knows (`aarch64` or `x86_64` Linux);
  `args` are Kiln driver arguments that change the language, never what
  is read or written or the form of the diagnostics. A line on which
  `kiln` would stop before the job (a driver error, an invalid
  diagnostic option, a driver warning `-Werror` makes an error, a
  per-input error such as `-fgnu89-inline` for C++) is refused too.
  `slip_kiln::snippet::parse_with` refuses them, and the refusal fails
  the test (`spec/landed/harness/WP-038.md`).
- In a `t.cpp` test both sides read the same lines. In an `a.cpp` /
  `b.cpp` test each side reads its own, so a `differ` test may differ in
  nothing but its `// args:` line: that shows a command-line fact is in
  the digest.
- A file of the test directory other than `t.cpp`, `a.cpp`, `b.cpp` and
  `expect.txt` is a file of both sides' snippets, and so is each file of
  `crates/slip_digest/tests/support/` the directory has none of the name
  of: `meta.h` declares the `std::meta` intrinsics and class types the
  reflection tests use, as Kiln's own test header
  (`kiln/tests/support/kiln_meta.h`) declares them.
- `SLIP_DIGEST_DUMP=1` prints, for each side, the command line in effect
  and the units; `SLIP_DIGEST_ONLY=<text>` makes the four pair runners
  run only the directories whose path contains `<text>`, and they print
  `SLIP_DIGEST_ONLY=<text>: <n> of <m> directories`. Nothing else
  narrows (the determinism and tables tests use every directory). The
  runners work on a pool (`SLIP_TEST_THREADS`, `SLIP_DIGEST_TIMES`,
  `SLIP_DIGEST_DERIVED`: `spec/landed/harness/WP-047.md`), and
  `cargo xtask ci` removes these variables and `SLIP_DIGEST_ONLY` from
  the environment of what it runs.

### 6.2 A transform test

```
tests/transform/<area>/<name>/
  test.txt        directives
  pre/            the input tree
  post/           the files of the expected post-image that differ from pre/, at the same relative paths
  expect.txt      the expected standard output of the run (§6.3)
  cdb.json        optional: the compilation database, with @ROOT@ for the tree's directory
```

`<area>` is the transform (`rename`). Sources are self-contained: no
system header. A directory `pre/dep/` is read-only code, as in
`spec/pitfalls.md`.

`test.txt` holds one directive per line, `<name>: <value>`; blank lines
and lines starting with `#` are skipped:

| Directive | Meaning | Default |
|---|---|---|
| `spec` | the section covered, as `edits §9.3`; may repeat | required |
| `run` | the transform command and its own options: `rename --from a::f --to g` | required, once |
| `state` | the expected `State` of the result (`spec/edits.md` §8.5), or `usage` or `input` for a run that must end with status 2 or 3 | required |
| `exit` | the expected exit status | 0 for `verified`, 2 for `usage`, 3 for `input`, 4 for `incomplete`, else 1 |
| `std` | the language standard | `c++20` |
| `args` | arguments added to every default command | none |
| `tu` | the main files, relative to the tree | every `*.cpp` not under `dep/`, in byte order of paths |
| `owned`, `readonly` | paths relative to the tree; may repeat | `owned: .`, and `readonly: dep` if `pre/dep` exists |
| `allow` | `residue`, `unparsed`, `partial` or `errors`: the run is given `--allow-<value>`; may repeat | none |
| `oracle` | `no-<oracle> <reason>` turns one oracle of §6.4 off for this test | all on |
| `require` | `case-insensitive`: a host or volume capability the test needs (may repeat; no duplicates); if unavailable, the test is `SKIP` (or `ERROR` under `SLIP_REQUIRE_ALL=1`) | none |
| `symlink` | `<link> -> <target>`: a symbolic link the runner creates in the work tree before each run (relative `<link>` not colliding with `pre/` or another symlink, relative `<target>` staying inside the tree and passing through no other declared symlink; never stored in git; may repeat) | none |
| `post` | `none`: the run may write and writes nothing (below) | absent |
| `timeout` | the seconds each invocation (of `slip`, and of clang by the clang oracle) may take, 1 to 86400 | 600 |
| `xfail` | the reason the test is expected to fail | absent |

Without `cdb.json`, each main file has the command
`kiln -std=<std> <args> -I@ROOT@ -I@ROOT@/dep -c <file>`, with directory
`@ROOT@`, in the order of `tu`.

The expected post-image is `pre/` with the files of `post/` laid over it.
A `post/` file equal to its `pre/` file (bytes and mode), or with no
`pre/` file, is an error of the test. A test **may write** when its state
is `verified`, or `verified_with_residue` with `allow: residue` or `allow:
errors` (a result of either state permits writing only so, `spec/edits.md`
§8.5). A test that may write has a `post/` holding at least one file, or
says `post: none` (its run writes nothing, and the expected post-image is
`pre/`); either is required, and both are an error. `post: none` is
checked again against the run's record (§6.3 step 4): it is for a result
that does not permit writing, or whose overlay changes no file. A test that may not
write has no `post/` and no `post: none`. No entry of a test, nor anything under `pre/` or `post/`, is a symbolic
link in git (a `post/` that is one would lead `--bless` out of the
test); each is an error of the test. Tests that need symbolic links
declare them with `symlink: <link> -> <target>` in `test.txt`, and the
runner creates them in the work tree.

### 6.3 Running

```
cargo xtask transform [--only <substring>]… [--bless] [-j <n>] [--no-clang] [--slip <path>]
        [--verbose]
```

`--slip` names the executable under test (default: the workspace's
`slip`, which the runner builds first); the runner's own tests give it a
scripted stand-in. `--no-clang` is §6.4's. The tests are the directories
`tests/transform/<area>/<name>/`; any other entry there (a file, a
symbolic link) is an error of discovery, and `--only` that selects no
test is an error.

For each test, in byte order of paths:

1. `pre/` is copied to a fresh **tree** under the work base (§11.5),
   every `symlink:` directive is created inside it (and included in the
   expected pre-image; `--bless` accepts those symlinks unchanged), and a
   fresh store is made beside it. For a test with `require:
   case-insensitive`, the runner probes whether the work directory's
   volume folds case, and skips the test (`SKIP`) if it does not. Nothing
   is written into the test's own directory except by `--bless`.
2. `slip <run> --db <database> --owned … --readonly … --store <store>
   --relative-to <tree> --write -j 2` and the `allow` options. `--write` is
   always given: a result that does not permit it must leave the tree
   untouched, and the test checks that. Every invocation (this one and
   the oracles') runs in a process group of its own and has the test's
   `timeout`: when it ends, the whole group is killed and the test is
   `KILLED`, also when the invocation itself has ended and a process it
   started holds its output open. When the group cannot be killed (no
   `kill` program works), the child alone is, and the `KILLED` message
   says the group may have survivors; a process that left the group (a
   new session) is beyond the runner's reach. After each invocation the work
   directory holds only the tree, the stores, the database and the
   oracles' directories, and the test's directory is byte for byte as it
   was; anything else is a `FAIL` ("wrote outside its tree", "changed
   the test's directory").
3. **The output's structure** is checked first (a `FAIL` before anything
   is compared or blessed): each of `plan`, `transform`, `round`,
   `edits`, `declared`, `overlay` and `result` at most once; a `state`
   line only right after `edits` (`accepted` or `refused`) or right after
   `result` (a state of `spec/edits.md` §8.5, exactly), the result's the
   last; a `usage` or `input` run prints no `result` line.
4. **The store** of a run that ends in a state: the result named by its
   `result` line is read from `results/<id>.rslt` (`spec/edits.md` §10.4)
   and must be there and valid; its state must be the printed one, its
   `edits`, `declared` and `overlay` the printed ids (and its plan, round
   and transform, where printed); the edit set, the declared changes
   (`declared/<id>.decl`, `spec/exec.md` §2) and the overlay must be in
   the store and valid, the overlay of that edit set; no `write.journal`
   may be left. **The records are of this tree**: every file the edit set
   or the overlay names is a regular file of `pre/` at its path under
   the tree (the tree's path, then `/`, then a relative path with no
   `.` or `..`), and the record's pre-image of it (size and content
   digest) is that file's. Any lie is a `FAIL`.
   **The tree is the verified overlay's** when the result permits writing
   (below; every run is asked to write): each file the overlay changes
   (`post` not `pre`) has, in the tree, the overlay's `post` size and
   content digest and `pre/`'s mode; every other entry is `pre/`'s;
   nothing else exists. Verification proves the overlay, so this ties
   the written bytes to what was verified, not only to `post/`. A
   difference is a `FAIL` of its own, before anything is compared or
   blessed, "tree differs from the verified overlay: <the first
   difference>"; a run that wrote nothing is "permitted and not
   written". A test that says `post: none` under a result that permits
   writing and an overlay that changes a file is malformed against its
   own record, a `FAIL` that says so.
5. **Compared**: the exit status; the state; standard output with
   `expect.txt`, after replacing every 64-digit id by `<id>` and the
   tree's path by `@ROOT@`; the tree, entry by entry, with the expected
   post-image (a file or directory added, missing, different in bytes or
   mode, or of another kind — a symbolic link in place of a file — fails).
   The first difference of each is printed. **Writing**: whether the run
   may write is `slip_verify::conclude::permits` over the result record,
   its edit set and the test's `allow` lines — never the state's name; a
   tree written when it does not permit is a `FAIL`.
6. The oracles of §6.4.

The runner takes job tokens as every harness command does (§5). It runs
tests in parallel, each with `-j 2`, and prints one line per outcome that
is not `PASS`, then `transform: <n> passed, <n> failed, …` with every
outcome that occurred, then `oracles: <n> off, <n> skipped (--verbose
names them)` when an oracle was turned off by `oracle:` or skipped.
`--verbose` prints, under each test, `  <area>/<name>: oracle <name>
off|skipped: <reason>` for each. Exit status 1 if the gate would fail.

**`SLIP_REQUIRE_ALL=1`** (§1 principle 3): a test skipped for an
unavailable `require:` capability (`SKIP`) is an `ERROR` (`<reason>
(SLIP_REQUIRE_ALL=1)`), and a test that passes with its clang oracle
skipped for want of a compiler (not for `--no-clang`) is an `ERROR`,
"the clang oracle was skipped: no compiler (SLIP_REQUIRE_ALL=1)".
It is for a host that has every capability and must show it: a release
candidate's gate.

`--bless` rewrites `expect.txt` and `post/` from the run, for tests whose
output's structure and store are sound, whose records are of the tree,
whose state and exit status are as `test.txt` says, and whose written
tree the result permits and is the verified overlay's (so `post/` holds
only what verification proved); a test that may write, whose result
does not permit writing or whose overlay changes nothing, and whose run
wrote nothing must say `post: none`. It never changes `test.txt`.

### 6.4 Oracles

Checks that hold for every transform test, beyond its golden files. Each
is a way the untrusted parts (transform, reduce, apply) and the trusted
ones (digest, verify) are tested against something other than
themselves.

| Oracle | Runs when | Passes when |
|---|---|---|
| `determinism` | always | a second run in a fresh store with `-j 1` gives the same exit status, transform, edit set, declared, overlay and result ids, the same standard output, and the same written tree |
| `verify` | the result record's state is `verified` or `verified_with_residue` | this is corpus invariant (c): the run itself verified the post-image. The oracle adds that `slip check` over the run's own edit set, in the same store, gives the same result id, whose record is the run's, with the printed state the record's |
| `back` | the tree was written | the inverse transform (for `rename --from a::f --to g`: `rename --from a::g --to f`) on the written tree, with a fresh store, ends in the same state modulo residue on one side (`verified` and `verified_with_residue` both accept each other) and leaves the tree byte-identical to `pre/` |
| `clang` | the tree was written, and a clang is available | every command of the test, with the compiler replaced by clang and `-fsyntax-only`, exits 0 on the pre-image (else `ERROR`) and on the written tree; on the written tree each command also writes the files it read (`-MD -MF`), and a file the run changed that no command read is a `FAIL`, "post-image not compiled: <file>" |
| `mutation` | the result record's state is `verified` or `verified_with_residue` | each mutated edit set, given to `slip check`, does **not** verify (below) |

An oracle that does not run, for any reason but `oracle: no-<name>`, is
**skipped**, with its reason ("the tree was not written", "no compiler",
"the result is not_verified", "the run has no result"), and counted
(§6.3). No oracle is passed over silently. A written tree reaches the
oracles only once it is the verified overlay's (§6.3 step 4), so `clang`,
`back` and `determinism` (whose tree must equal it) judge the image that
was verified.

**Back.** Residue need not be symmetric between the old and new names (an
`#if 0` branch or an uninstantiated template may spell only the old name,
giving `verified_with_residue` forward and `verified` backward), so the
oracle accepts `verified` and `verified_with_residue` in either direction
while still rejecting any other state mismatch (`not_verified`, `refused`,
`incomplete`). When `name_taken` is not symmetric (`spec/edits.md` §9.5,
for example a derived class declaring the old name where the inverse
rename is refused `name_taken`), a test turns the oracle off with
`oracle: no-back <reason>` citing `spec/edits.md` §9.5.

**Clang.** The compiler is `$SLIP_CLANG` if set, else `clang++-19`, else
`clang++` on `PATH`. If there is none the oracle is skipped, the runner
counts it (`clang oracle skipped: <n>`), and `cargo xtask ci` fails unless
it was given `--no-clang`.

**Mutation.** The trusted base must reject what the untrusted parts get
wrong, so the harness gets it wrong on purpose. From the run's edit set
it makes, one at a time:

- the edit set without edit `i`, for every `i` if there are at most 8
  edits, else for `i = ⌊k·n/8⌋`, `k = 0..7`;
- the edit set with one more edit: the pieces of the first edit, at a
  `kept` range (the first 4 in order);
- the edit set with the `key` of one spelled piece replaced by its
  bitwise complement (the first 4 spelled pieces in order): the name is
  the same and the entity meant is not.

Each is published as a new `EDTS` (the harness links `slip_model`) and
given to `slip check` with the declared changes of the original. The
oracle runs only when the run's result is `verified` or
`verified_with_residue` (a result that did not verify has nothing for a
mutation to break, and one that is `incomplete` would fail every
mutation); otherwise it is a skip. The check's state is its result
record's, which must agree with the printed one. The outcome must be
`not_verified` (for a changed key: by a failed spelling obligation of the
changed key itself, an obligation of the check's overlay whose key is the
mutated piece's, `spec/verify.md` §10.3), or, for a dropped edit only,
`verified_with_residue` with a residue entry that is not a mention, in the
dropped edit's file, within the dropped range; a dropped edit that leaves
no edit makes a refused set, which must end `refused`, and only it.
`verified`
fails the test: an edit was lost or invented and nothing noticed. That is
a hole in the trusted base and blocks a release, like a `bad` tree that
verifies (`spec/pitfalls.md`). A dropped edit that provably changes
nothing observable is turned off per test with `oracle: no-mutation
<reason>`.

### 6.5 Transforms on a corpus

```
cargo xtask corpus rename <corpus> --from <name> --to <identifier> [--kind <k>]
        [--owned <path>]… [--expect <state>] [--accept] [--build] [-j <n>]
```

runs `slip rename` over the corpus's database (§8) with a store under the
corpus's build directory, `--allow-unparsed`, `--allow-residue`, and
`--patch` (never `--write`: the corpus's source tree is shared and stays
as fetched). `--owned` defaults to the corpus's source directory. It then
checks, in order:

1. the state is `--expect` (default `verified_with_residue` or
   `verified`);
2. **determinism**: `slip reduce` run again over the same shards gives the
   same edit set id;
3. with `--accept`: every command of the plan, run by clang with
   `-fsyntax-only` and a clang VFS overlay file that maps each overlay
   path to its blob, exits as it does on the pre-image (the two units
   clang rejects in `llvm`, §8.7, must be rejected the same way);
4. with `--build`: the patch is applied to a copy of the source
   directory under the corpus's build directory; the copy is configured
   and generated as the corpus is (§8.3) and built with clang (`all`);
   the build succeeds;
5. with `--build`, **back**: the inverse rename over the copy's database,
   with `--write`, leaves every file of the copy byte-identical to the
   corpus's source.

The copy and its build tree are removed when the checks pass, and kept,
with their path printed, when one fails.

It prints the lines of `slip rename`, then one line per check, and writes
`bench/results/<corpus>.rename.<target>.json`: the counters of the edit
set (`spec/edits.md` §6.3), the tally, the residue counts, and from the
transform run record (`spec/edits.md` §10.6) and the two phase records it
names: wall time per step, the coordinator's peak resident size per step,
and the phases' `peak_rss_max` and `peak_rss_sum` (§7.1). Counters are
compared exactly with the file already there, if any; times and sizes
are reported and not gated ("Open questions").

What the command fixes beyond that (`xtask/src/corpus/rename.rs`):

- **Names.** `--from` is identifiers joined by `::` (a leading `::`
  allowed) and `--to` one identifier; anything else is a usage error
  (status 2). `<target>` is `--from` without a leading `::`, each `::`
  written `.`. `--expect` takes one of the five states of
  `spec/edits.md` §8.5. `--slip <path>` names the `slip` executable, as
  for §9 and §10; by default it is `target/release/slip`, never built by
  xtask.
- **Paths.** "The corpus's build directory" is the corpus directory
  `<root>/<name>/` (§8.2): the store is `slip-store/`, the patch
  `rename/<target>.patch`, the overlay file of check 3
  `rename/<target>.vfs.json` (both kept), the copy `rename-copy/` and its
  build tree `rename-build/` (with the inverse run's store,
  `rename-build/slip-store/`, and the logs `patch.log`, `configure.log`,
  `generate.log`, `build.log`). "The source directory" is the clone
  `src/`, not the cmake source directory: it is `--owned`'s default and
  `--relative-to`, so uses in projects the corpus does not configure are
  residue. A given `--owned` must be under `src/` (real paths), or the
  command stops before `slip` runs.
- **Census pre-flight (`--expect verified`, `--expect verified_with_residue`,
  `--accept`).** Runs that need verification (`--expect verified`,
  `--expect verified_with_residue`, or `--accept`, except when `--expect`
  explicitly names `refused`, `not_verified` or `incomplete`) first run
  the census (§9.5) into `<root>/<name>/stores/census` with the same `slip`
  and `-j`, print the census's last line, and refuse to start (exit status
  1, one line naming `cargo xtask census <corpus>`) if the census is not
  clean, unless `--skip-census` is given (which skips the check and appends
  ` (census skipped)` to the final `corpus rename:` line). `--expect refused`
  and `--expect not_verified` do not run the census. The census store holds
  transform-free round-0 map shards and is not reused as `slip-store/`,
  which holds round-1+ transform shards.
- **The run.** `slip rename` is given `--kind` and `-j` when the command
  is; its lines are printed as they come. Status 2 or 3, or a run that
  prints no `result` line or a state other than its result's, is an
  error of the command; 0, 1 and 4 go on to the checks. The result, its
  edit set, the latest transform run record that names the result and
  the phase records it names (by content id) are read from the store.
- **Checks.** They run in order and the first failure stops them; each
  prints `check <name>: ok|FAIL|skipped|not run (<detail>)`, with
  `<name>` one of `state`, `determinism`, `accept`, `build`, `back`. Every
  step prints `step <step>: started (…)` and `step <step>: done in <s>`.
  A result with no overlay (`refused`) has no post-image: checks 3 to 5
  are `skipped` even when asked for.
- **1** also fails a `verified` or `verified_with_residue` run that wrote
  no patch (its `--patch` refused at apply, `spec/edits.md` §7.2).
- **2.** `slip reduce --plan <plan> --round <round> --store <store>` (and
  `-j`); status 0 or 1 with the run's edit set id passes, its lines
  printed after `reduce| `.
- **3.** The commands are the plan's observers: the corpus's C compiler
  (§8.1 `c_compiler`) for C and Objective-C, its C++ compiler otherwise,
  found on `PATH`, with the observer's arguments after `argv[0]`,
  `-fsyntax-only`, its directory and environment; once as they are and
  once with `-ivfsoverlay <file>`. The overlay file maps every overlay
  file whose post-image differs from its pre-image to its blob in the
  store, `use-external-names` off. One jobserver token per command pair,
  at most `-j` pairs at once (the jobserver's count without `-j`); a
  pair that does not end within an hour counts as a difference. Exit
  statuses alone are compared; up to 20 differences are printed with the
  first error line.
- **4.** Before `slip` runs: the tools are found, the file system of the
  corpus directory must have at least twice the bytes of `src/`'s
  regular files free (`df -Pk`), and a `rename-copy/` or `rename-build/`
  left by an earlier run is removed. The copy is of `src/` without its
  top-level `.git`: every regular file written anew from its bytes
  (never a hard link, a reflink or an in-kernel copy), with its mode and
  modification time; links as links. The patch, when not empty, is
  applied with `patch -p0 --batch --forward --no-backup-if-mismatch`.
  cmake runs with §8.3's arguments over the copy, then `make` of the
  `generate` entries (§8.3, without its promised-file check) and
  `make all`, with no `-j` (the jobserver in `MAKEFLAGS` bounds them).
- **5.** The inverse is §6.4's (`--from <prefix>::<to> --to <last>`),
  over `rename-build/compile_commands.json`, `--owned` mapped into the
  copy, `--allow-unparsed --allow-residue --write`; status 0 or 1 goes
  on to the comparison. Every entry of `src/` (without `.git`) and of the
  copy is compared: presence, kind, a regular file's bytes, a link's
  target; modes and times are not.
- **Removal.** `rename-copy/` and `rename-build/` are removed only by
  their full paths and only when strictly under the corpus directory
  (§9.1's rule); `removed <path>` or `kept <path>` is printed for each.
- **The result file** holds `corpus`, `pin`, `from`, `to`, `kind`,
  `options` (`owned` relative to `src/`, `expect`, `accept`, `build`,
  `jobs`), `tool_slip`, `tool_kiln`, the ids `plan`, `round`,
  `transform`, `edits`, `declared`, `overlay` and `result`, `checks`
  (`check`, `outcome`, `detail`), then the exact members `state`,
  `counters` (the edit set's, by name), `tally`, `agreement` (`stale`,
  `unused_rules`), `spellings` (`obligations`, `passed`, `failed`) and
  `residue` (a count per category) and `not_plain_at_macro` (the edit
  set's refusals with reason `not_plain` that have a `macro`, i.e. the
  name was read from an expansion: a whole-invocation range, or a
  spelling that is not plain inside a macro argument), then
  `measured`: the transform run's
  `wall_ns`, per step `wall_ns`, `cpu_ns` and `peak_rss`, per phase
  record §7.1's measurements, and each check's `wall_ns`. No absolute
  path is in it. The ids are not compared: every `slip rename` takes the
  store's next round, and the round is in every id after the plan's.
  The exact members are compared with the file there;
  each difference is printed `  <member>[.<name>]: <old> -> <new>`. The
  file is written only when every check passed and nothing differs (or
  there was none); otherwise the run's file is `<file>.new` and the file
  stays. The last lines are `counters: …`, `result <file>` and
  `corpus rename: ok` or `corpus rename: FAILED (<checks>[, counters])`;
  the status is 0 when ok, 1 otherwise, 2 for a usage error.

## 7. Performance records

A parallel phase writes a run record (`spec/model.md` §9) into its store
(`spec/exec.md` §2). The commands here take a run record **by its path in
the store**, `<store>/runs/<plan id>/<phase>.<round>.<started>.run`: the
store is the directory three levels up, and the plan and the shards of
the run are read from that store (shared access). The plan, phase and
round are the record's own (`spec/model.md` §9); the file name only
locates it.

```
cargo xtask perf summarize <run record>…
cargo xtask perf record <corpus> <run record>
cargo xtask perf check  <corpus> <run record>
```

**Path shape.** The last three components must be plain names: `runs`, a
plan id (64 lower-case hexadecimal digits), and
`<phase>.<round>.<started>.run` with phase `map` or `verify` and the two
numbers canonical decimal `u64` (`0`, or digits without a leading zero).
The store is the path without those three components, `.` when nothing is
left. Components are taken as written (`Path::components`): a `..` among
the last three is an error, not resolved. A path of another shape is an
error.

**Loading**, in this order:

1. The record is read and validated as a `RUNR` container; its `plan`,
   `phase`, `round` and `started_unix_ns` must be its path's (the store
   names the file by all four, `spec/exec.md` §2). The record is read
   before the store is opened, so an absent or empty directory is never
   initialised as a store.
2. The store is opened `Shared`.
3. The plan must be in the store, validate, and have the content id its
   name says.
4. The **observers of the record** are the distinct `Job.observer` values.
   One that is not an observer of the plan is an error (its main file,
   which the summary prints, would be unknown).
5. For each observer of the record, the shard of the run's phase and round
   is read; one that does not validate, or names another observer, phase or
   round, is an error naming the shard file. A missing shard is not an
   error when loading: `summarize` reports it, `counters` (§7.1, and so
   `record` and `check`) refuse it. A shard's content id is not compared
   with any `Job.shard`.

Errors name their file: the path given, or the store's path for the plan
and shards (`Store::plan_path`, `shard_path`). Every error (an unreadable
or invalid file, an unknown corpus, a cancelled run where counters are
needed, an overflow) prints `xtask perf: <message>` on standard error and
exits 1; a wrong argument count or unknown command is a usage error with
the usage text, exit 2.

### 7.1 What is computed

From the record, its plan and its shards:

- **Observers**: the distinct observers among the record's jobs.
- **Counters**, all exact and deterministic for a given corpus, tool and
  host headers:

  | Name | Value |
  |---|---|
  | `plan.observers` | observers in the plan |
  | `plan.excluded.<reason>` | excluded entries per reason, every reason of `spec/model.md` §7 present, zeros included |
  | `run.observers` | observers in the record |
  | `shard.<status>` | the record's observers by the status of their shard in the store, the four statuses present, zeros included |
  | every shard counter name | the sum of that counter over those shards |

  Shards are those of the record's observers only. An observer of the
  record with no shard in the store is an error: the record is of a
  cancelled run, or the store is not the run's. A shard counter whose name
  is one of the names this table computes (`plan.observers`,
  `plan.excluded.*`, `run.observers`, `shard.*`) is an error, not an
  overwrite.
- **Measured**, none deterministic: `wall_s` (the run's `wall_ns`), `cpu_s`
  (the sum of `cpu_ns` over every job), `peak_rss_max` and the percentiles
  `peak_rss_p50`, `peak_rss_p90`, `peak_rss_p99` (over the jobs that ended
  `completed` with a non-zero `peak_rss`; nearest rank: the value at
  position `ceil(p × n / 100)` of the ascending list, clamped to `1..=n`;
  0 when there is no such job), `peak_rss_sum`, `cores`, `workers_max`,
  `memory_budget` (those four as in the record). Sizes are whole bytes.
  Seconds are printed with three decimals, computed in integers from
  nanoseconds: `(ns + 500 000) / 1 000 000` milliseconds in `u128`,
  printed `<ms / 1000>.<ms % 1000, three digits>` (half up; `u64::MAX` ns
  is `18446744073.710`).

Every sum is a checked `u64` addition; an overflow is an error
(`<what> overflows a 64-bit unsigned integer`).

### 7.2 `summarize`

For each record, in the order given, separated by an empty line:

```
run <path as given>
plan <plan id>
phase <phase> round <round>
tool slip <version> kiln <version>
observers <run.observers> of <plan.observers>
shard <status> <n>                    four lines, in the order of spec/model.md §8
job <end> <n>                         five lines, in the order of spec/model.md §9; every job counted
wall_s <s>
cpu_s <s>
efficiency <cpu / (wall × workers_max), two decimals; `-` if the divisor is 0>
workers_max <n> cores <n> memory_budget <bytes>
peak_rss max <bytes> p50 <bytes> p90 <bytes> p99 <bytes> sum <bytes>
counter <name> <value>                the shard counters, by name
top cpu
  <cpu_s> <main file>                 the ten observers with the greatest summed cpu_ns, greatest first
top peak_rss
  <bytes> <main file>                 the ten observers with the greatest peak_rss of any job, greatest first
```

- Every record is loaded before anything is printed; an error in any of
  them prints nothing on standard output and exits 1.
- Lines are exactly the template with single spaces; the `top` entries are
  indented two spaces; the output ends with a newline. `shard` lines are in
  the order `ok`, `diagnosed`, `rejected`, `failed`; `job` lines in the
  order `completed`, `crashed`, `killed_memory`, `killed_time`,
  `cancelled`, every job (attempt) counted. `counter` lines are the shard
  counters summed over the shards present, by name (byte order). `tool` is
  the record's `tool_slip` and `tool_kiln`.
- **Efficiency** is computed from nanoseconds, not from the printed
  seconds: `cpu_ns × 100 / (wall_ns × workers_max)` rounded half up to two
  decimals in integers; `-` when `wall_ns × workers_max` is 0.
- `top cpu` ranks every observer of the record, zeros included, by its
  summed `cpu_ns` (every attempt); `top peak_rss` by the greatest
  `peak_rss` of any of its jobs, whatever their end. At most ten each;
  ties are broken by observer id, ascending. `<main file>` is the
  observer's main file path from the plan.
- With a cancelled run (an observer without a shard) `summarize` still
  prints everything the record alone gives, the `shard` lines counting the
  shards present, and a line `missing shards <n>` right after them. The
  line is printed only when `n` > 0.
- **Text.** The given path, the tool versions and the main file paths are
  printed as one line: valid UTF-8 as itself, `\` as `\\`, control
  characters below U+0100 and bytes that are not UTF-8 as `\xNN`.

### 7.3 Baselines

A baseline is `bench/baselines/<corpus>.json`, written by `perf record` in
the pretty JSON form (`spec/model.md` §4), members in this order:

```json
{
  "corpus": "llvm",
  "pin": "llvmorg-19.1.7",
  "slip": "0.0.0",
  "kiln": "1.3.0",
  "counters": { "inputs.bytes": 0, "inputs.files": 0, "plan.excluded.missing_file": 0, "plan.observers": 0, "run.observers": 0, "shard.ok": 0 },
  "measured": { "wall_s": 0.0, "cpu_s": 0.0, "peak_rss_max": 0, "peak_rss_p50": 0, "peak_rss_p90": 0, "peak_rss_p99": 0,
                "peak_rss_sum": 0, "cores": 0, "workers_max": 0, "memory_budget": 0 }
}
```

`corpus` is the row name of `bench/corpora.toml` and `pin` its tag (§8);
`slip` and `kiln` are the record's tool versions; `counters` is every
counter of §7.1 sorted by name; `measured` is in the order above. A
baseline contains no absolute path and no time of day.

**Writing.** A baseline is written with `slip_model::json::write_pretty`.
Counters and the whole-number measured values are JSON integers, also
above 2^53 (the string rule of `spec/model.md` §10 is for container
renderings, not baselines). `wall_s` and `cpu_s` are JSON numbers with a
fraction: the `f64` nearest the three-decimal seconds of §7.1, written
shortest round-trip (`7.5`, `4.124`, `0.0`).

**Reading** (for `check`) is with `slip_model::json::parse`. Every member
is required and none other is allowed, in any order. Messages:
`not JSON: <reader error>`, `at the top level: expected an object`,
``no member `<key>` ``, `at <key>: unknown member`,
`at <key>: expected a string`, `at counters: expected an object`,
`at counters.<name>: expected an integer in 0..2^64`,
`at measured: expected an object`, ``at measured: no member `<name>` ``,
`at measured.<name>: unknown member`,
`at measured.<name>: expected an integer in 0..2^64`, and for `wall_s` and
`cpu_s` `at measured.<name>: expected a non-negative number of seconds
with a fraction` (a JSON number without a fraction or an exponent, such as
`4`, is refused). The command prefixes `bench/baselines/<corpus>.json: `.
A baseline whose `corpus` is not the corpus named on the command line is
an error.

- `perf record <corpus> <run record>` looks the corpus up
  (`xtask::corpus::rows`) before reading the record; refuses a record that
  does not cover the plan (`run.observers` ≠ `plan.observers`: ``the
  record covers <a> of the plan's <b> observers``) before computing
  counters; then writes `bench/baselines/<corpus>.json` under the
  repository root through `.<name>.<pid>.tmp` in the same directory and
  one rename (creating the directory, replacing a file that exists,
  removing the temporary file on failure), and prints the full path. A
  baseline update changes `counters` only together with a stated reason in
  the commit message.
- `perf check <corpus> <run record>` compares against the committed
  baseline. `pin` and `counters` are compared **exactly**; `measured` is
  informational and never fails the check; the tool versions are printed
  when they differ and do not fail it. Output, on standard output, in this
  order: `pin: baseline <a>, run <b>` if the pin differs (the run's pin is
  the corpus row's tag); `slip: baseline <a>, run <b>` and `kiln: …` if a
  tool version differs; one line `<name>: baseline <a>, run <b>` per
  counter that is missing, extra or different, by name, `-` standing for a
  counter absent on that side; then all ten measured values
  `<name> <run value> (×<ratio>)` (`wall_s 12.300 (×1.04)`: the run's
  seconds with three decimals, whole numbers in decimal; the ratio is
  `run / baseline` in `f64` printed with two decimals, `-` when the
  baseline value is 0); and last `perf check: <corpus>: ok` (exit 0) or
  `perf check: <corpus>: FAILED (<n> difference(s))` (exit 1), `n`
  counting the pin and the counters.

### 7.4 Interface

`xtask::perf`, with `xtask::perf::render`; of `render`, everything but
`TOP` and `text` is re-exported from `perf`:

```rust
// perf
pub const BASELINES: &str = "bench/baselines";
pub struct RunPath { pub store: PathBuf, pub plan: Digest, pub phase: Phase, pub round: u64, pub started_unix_ns: u64 }
pub fn parse_run_path(path: &Path) -> Result<RunPath, String>;
pub struct Loaded { pub path: RunPath, pub record: RunRecord, pub plan: Plan,
                    pub shards: BTreeMap<Digest, Option<Shard>> }   // every record observer; None: no shard
impl Loaded { pub fn missing_shards(&self) -> usize; }
pub fn load(path: &Path) -> Result<Loaded, String>;
pub const REASONS: [Reason; 6]; pub const STATUSES: [Status; 4]; pub const ENDS: [End; 5];
pub fn reason_name(Reason) -> &'static str; pub fn status_name(Status) -> &'static str; pub fn end_name(End) -> &'static str;
pub fn shard_statuses(loaded: &Loaded) -> [(Status, u64); 4];
pub fn shard_counters(loaded: &Loaded) -> Result<BTreeMap<String, u64>, String>;
pub fn counters(loaded: &Loaded) -> Result<BTreeMap<String, u64>, String>;
pub struct Measured { pub wall_ns: u64, pub cpu_ns: u64, pub peak_rss_max: u64, pub peak_rss_p50: u64,
                      pub peak_rss_p90: u64, pub peak_rss_p99: u64, pub peak_rss_sum: u64,
                      pub cores: u64, pub workers_max: u64, pub memory_budget: u64 }
impl Measured { pub fn texts(&self) -> [String; 10]; pub fn values(&self) -> [(&'static str, Num); 10]; }
pub fn measured(run: &RunRecord) -> Result<Measured, String>;
pub fn percentile(sorted: &[u64], p: u64) -> u64;
pub enum Failure { Usage(String), Error(String) }
pub fn main(repo: &Path, args: &[String]) -> ExitCode;
pub fn run(repo: &Path, args: &[String], out: &mut dyn Write) -> Result<bool, Failure>;  // Ok(false): check failed

// perf::render
pub const TOP: usize = 10;
pub fn seconds(ns: u64) -> String;
pub fn text(bytes: &[u8]) -> String;
pub fn render_summary(given: &str, loaded: &Loaded) -> Result<String, String>;
pub enum Num { Seconds(f64), Count(u64) }
pub const MEASURED: [(&str, bool /* seconds */); 10];
pub struct Baseline { pub corpus: String, pub pin: String, pub slip: String, pub kiln: String,
                      pub counters: BTreeMap<String, u64>, pub measured: Vec<(String, Num)> }
pub fn render_baseline(corpus: &Corpus, loaded: &Loaded) -> Result<String, String>;
pub fn read_baseline(bytes: &[u8]) -> Result<Baseline, String>;
pub fn check(baseline: &Baseline, corpus: &Corpus, loaded: &Loaded) -> Result<(String, bool), String>;
```

`measured` and `render_summary` return `Result` because a sum can
overflow. `check` takes a parsed `Baseline`, so that an error in the
baseline names the baseline file (the caller prefixes it) and an error in
the run names the run's file.

## 8. Corpora

A corpus is pinned third-party source with a CMake build tree that has a
compilation database and the generated sources its translation units
include, so that every entry of the database can be parsed. Building the
corpus itself is not part of it. Corpora live under a corpus root (§8.2),
`.corpus/` (git-ignored) by default, together with the stores of runs over
them. The pins are Kiln's (`kiln/scripts/oss/projects.toml`) so both
projects measure the same code: LLVM 19.1.7 as C++17 first (§8.7).

```
cargo xtask corpus list
cargo xtask corpus fetch <name>
cargo xtask corpus configure <name>
cargo xtask corpus status <name>
cargo xtask corpus path <name> [--db]
```

### 8.1 `bench/corpora.toml`

One table per corpus; the table name is the corpus name (lower-case ASCII
letters, digits, `-`, `_`). All keys are required; any other key is an
error.

| Key | Value |
|---|---|
| `kiln` | the name of a table of `kiln/scripts/oss/projects.toml` |
| `cxx_std` | non-negative integer (fitting 32 bits), `CMAKE_CXX_STANDARD` |
| `build_type` | string, `CMAKE_BUILD_TYPE`: non-empty, no whitespace, no `/` |
| `c_compiler`, `cxx_compiler` | strings, compiler names looked up on `PATH`: non-empty, no whitespace, no `/` |
| `configure` | array of strings, further CMake arguments, in order |
| `generate` | non-empty array of distinct entries: a build-target name (`[A-Za-z0-9_.+-]`, not starting with `-`), or `<target>:<file>`, one generated file made by a rule of that target's `build.make` (a relative path without `.`, `..` or empty components, without whitespace or `:`, with an extension of §8.6) |

The pin comes from the Kiln row, so Slip and Kiln measure the same code:
its `url` (a required string: non-empty, not starting with `-`, no
whitespace), its `tag` (a required string: non-empty, not starting with
`-`, of `[A-Za-z0-9_./+-]`) and its optional `source_subdir` (a relative
path without `.`, `..` or empty components; absent means the checkout
itself). The Kiln row must be a plain git clone: a row whose `archive`,
`local`, `submodules` or `stage_of` is set to anything but `false` is an
error. Other Kiln keys are ignored.

`configure` must not set what xtask sets itself: an argument starting
`-G`, `-S` or `-B`, `--fresh`, or `-D` of `CMAKE_EXPORT_COMPILE_COMMANDS`,
`CMAKE_BUILD_TYPE`, `CMAKE_C_COMPILER`, `CMAKE_CXX_COMPILER` or
`CMAKE_CXX_STANDARD` (with or without a `:TYPE`).

Both files are read with the line reader of the lints (tables, strings,
integers, arrays of strings, `#` comments outside strings). A nested or
array table, an entry outside a table, a duplicate table or key, or a value
of the wrong shape is an error naming the file and line. Errors exit 1
with `xtask corpus: <message>`.

### 8.2 Layout

The corpus root is `$SLIP_CORPUS_DIR` if set and non-empty, else `.corpus/`
in the repository. A relative `$SLIP_CORPUS_DIR` is an error. Corpus
`<name>` lives in `<root>/<name>/`:

| Path | What |
|---|---|
| `src/` | the clone |
| `src.partial/` | a clone in progress; removed before the next clone |
| `build/` | the build tree; `build/compile_commands.json` is the database |
| `fetch.stamp`, `configure.stamp`, `generate.stamp` | §8.4 |
| `fetch.log`, `build/configure.log`, `build/generate.log` | the output of `git clone`, `cmake`, `make` |
| `lock` | §8.5 |
| `stores/<name>/` | stores of the runs xtask makes over the corpus (§9, §10) |
| `demo1/` | the trees, declared files and reports of §10 |
| `slip-store/` | the store of `corpus rename` (§6.5) |
| `rename/` | its patches and clang overlay files, `<target>.patch` and `<target>.vfs.json` |
| `rename-copy/`, `rename-build/` | its patched copy of `src/` and that copy's build tree (§6.5, `--build`); kept only after a failed check |

### 8.3 Steps

**fetch.** Needs `git`. If `src/` exists and the fetch stamp is current
(§8.4), nothing runs. If `src/` exists without a current stamp, its `HEAD`
must be the commit of `refs/tags/<tag>` there; then the stamp is written
(and the later stamps removed), else it is an error that says to remove
`src/` to fetch again: an existing clone is never deleted or overwritten.
Otherwise all three stamps are removed, `src.partial/` is removed if
present, and

```
git -c advice.detachedHead=false clone --quiet --depth 1 --branch <tag> -- <url> <dir>/src.partial
```

runs in `<root>/<name>/`; then `HEAD` is checked against the tag as above,
`src.partial/` is renamed to `src/`, and the stamp is written.

**configure.** Runs fetch first (a no-op when present), then needs
`cmake`, `make`, and the two compilers on `PATH`, each resolved to its
absolute path. The CMake arguments are, in order:

```
--fresh -S <src>[/<source_subdir>] -B <build> -G "Unix Makefiles"
-DCMAKE_BUILD_TYPE=<build_type> -DCMAKE_C_COMPILER=<cc path>
-DCMAKE_CXX_COMPILER=<cxx path> -DCMAKE_CXX_STANDARD=<cxx_std>
-DCMAKE_EXPORT_COMPILE_COMMANDS=ON <configure…>
```

If the configure stamp is current and the database exists, cmake does not
run. Otherwise the configure and generate stamps are removed, cmake runs in
`build/`, and the stamp is written if it succeeded and wrote the database.

Then the **generate step**: every `generate` target must be a target of
the configured build (§8.6), and every `<target>:<file>` file must have a
rule (a line `<file>:`) in its target's `build.make`, else an error before
anything is built. If the generate stamp is current nothing is built;
otherwise `make <targets…>` (the plain entries, if any) and then, per
`<target>:<file>` entry in order, `make -f <dir>/build.make <file>` run in
`build/`. xtask never passes `-j`: make inherits `MAKEFLAGS` and
takes its parallelism from the jobserver named there (one job without
one). Then the generated check (§8.6) runs, whether or not make ran; if it
fails, the generate stamp is removed and configure fails. Otherwise the
stamp is written (if make ran) and configure prints the number of generated
files checked and the number of database entries (the objects of its
top-level JSON array; anything else is an error).

**status.** Runs nothing; prints `corpus`, `directory`, `source` (present
at the stamped commit, present but not stamped for this pin, or absent),
`build` (configured when a configure stamp exists for the stamped commit),
`database` (path and entries, unreadable, or absent) and `generated`
(`done (<n> targets)` when the generate stamp is current for the configure
stamp and the row's targets, else `not done`). Whether the configure stamp
matches the row is not checked by `status`, since the compilers' paths are
part of it; `configure` is the check.

**path.** Prints `<root>/<name>`; with `--db`, the database's path, and
exits 1 if it does not exist.

**list.** A table `name kiln tag url`, columns padded as in §4.

A wrong argument count, an unknown command or option is a usage error
(exit 2). An unknown corpus is an error naming the known ones.

A step's tool is looked up on `PATH` (absolute directories only, an
executable regular file). A missing one fails with the single line
``xtask corpus: `<tool>` is not on PATH: run scripts/provision.sh --full
(nothing is installed by xtask)``. A command that fails prints the last 20
lines of its log, each prefixed `  | `, and fails with
`<command> failed (<status>); the log is <path>`.

Progress lines (on stdout) say what each step did and how long it took.

### 8.4 Stamps

A stamp is a text file whose first line is `slip-corpus-stamp 1`, written
to `<stamp>.tmp` and renamed. A stamp is current when its bytes are exactly
what the step would write now:

- fetch: `step fetch`, `url <url>`, `tag <tag>`, `commit <sha>` (the commit
  read back from the stamp itself, so a changed `url` or `tag` makes it
  stale);
- configure: `step configure`, `commit <sha>`, then `arg <a>` for every
  CMake argument of §8.3 in order (so the compilers' resolved paths, the
  directories and every `configure` value are part of it);
- generate: `step generate`, every line of the configure stamp after its
  first prefixed `configured `, then `target <t>` per target in order.

So an unchanged row is a no-op, a changed configuration runs cmake and the
generate step, and a changed target list runs only the generate step. A
step that runs removes the stamps after it before it starts.

### 8.5 Lock

`fetch` and `configure` hold `<root>/<name>/lock` (created exclusively,
holding the process id) while they run, and remove it when they end,
successfully or not. If it exists and `/proc/<pid>` exists (or the pid
cannot be read, or there is no `/proc`), the command fails naming the
holder; a lock whose process is gone is taken over. `status`, `path` and
`list` take no lock. (`flock` is not used: on this host's bind-mounted
workspace it does not exclude.)

### 8.6 The generated check

Read from the configured "Unix Makefiles" tree:

- `CMakeFiles/Makefile2` maps a target name `t` to its rule directory by
  the lines `t: <dir>/rule` (`<dir>` ending `.dir`, `t` without `/`), and
  gives target dependencies by the lines `<dir>/all: <dir'>/all`;
- `<dir>/build.make` lists what building the target makes by the lines
  `t: <file>…`.

Recipe lines (tab-indented), comments and blank lines are ignored. The
files a list of targets **promise** are, over the targets and every target
they depend on transitively, each `<file>` of a `t: <file>` line that is not
under a `CMakeFiles` directory and whose extension is one of `inc`, `def`,
`h`, `hpp`, `hh`, `hxx`, `gen`; plus the file of each `<target>:<file>`
entry. Relative paths are relative to `build/`.
Every promised file must exist after the generate step. A failure lists up
to 20 missing paths and fails with
`<m> of the <n> generated files the generate targets promise are missing`.

### 8.7 The `llvm` corpus

`llvm` is Kiln's `llvm_project` row (llvm-project, tag `llvmorg-19.1.7`,
source directory `llvm`) configured as Release, C++17, with `clang-19` and
`clang++-19`, `LLVM_ENABLE_PROJECTS=clang`, `LLVM_TARGETS_TO_BUILD=X86;AArch64`,
tests included, benchmarks and examples excluded, and the optional host
libraries (zlib, zstd, libxml2, libedit, libpfm) off so the configuration
does not depend on what the host has installed.

Its `generate` list covers every target of the configured tree whose
`build.make` lists generated files as its own outputs (the §8.6 rule),
except the four that copy clang's builtin headers into the build tree
(`clang-resource-headers`, `arm-resource-headers`,
`arm-common-resource-headers`, `aarch64-resource-headers`): no translation
unit includes those copies through the build tree (no database command
names `lib/clang/19/include`). It names the umbrella targets
(`intrinsics_gen`, `omp_gen`, `acc_gen`, `vt_gen`, `llvm_vcsrevision_h`,
the three `…TargetParserTableGen`, `X86CommonTableGen`,
`AArch64CommonTableGen`, `clang-tablegen-targets`) and every generator
target they do not reach through `Makefile2`. One generated file,
`tools/clang/lib/Basic/VCSVersion.inc`, is a source of the library target
`obj.clangBasic` rather than the output of a target of its own, hence the
entry `obj.clangBasic:tools/clang/lib/Basic/VCSVersion.inc`.

What a parse of it finds, and why (`bench/baselines/llvm.json` holds the
counts):

- The database has 4069 entries and the plan 4061 observers, none
  excluded: eight files (seven of `llvm/utils/TableGen`, and
  `clang/tools/clang-fuzzer/DummyClangFuzzer.cpp`) are each compiled twice
  with one command, and each pair is one observer (`spec/cdb.md` §7).
- Two units are `diagnosed`, and clang 19 rejects them with the same
  error at the same place: `clang/tools/clang-fuzzer/ClangFuzzer.cpp` and
  `ClangObjectiveCFuzzer.cpp` name `uint8_t` without including a header
  that declares it (no target of the configuration builds them).
- With Kiln at `62d190007`, nineteen units were `failed`: the front end
  panicked in three defects of Kiln's constant evaluator. Six were the two
  of `docs/requests/KD1-oven-panics-llvm.md`; thirteen (eleven of
  `llvm/lib/DWARFLinker/Parallel`, two unit tests) include
  `llvm/ADT/ConcurrentHashtable.h`, whose parenthesised `std::min` call
  as a shift count panicked a fast path Kiln added between Slip's two pins
  (`docs/requests/KD2-oven-minmax-paren.md`). Kiln's `main` fixed both
  (`83566cc47`, WP-044). With Kiln at `913a490bb` (WP-101) none is
  `failed`: 4059 `ok` and the 2 `diagnosed` above.
- Slip's Kiln pin is `60fb5ab9d` since WP-163 (PCH format 187, module
  image format 101; `4a83f6e3c` from WP-066). The numbers of this
  section are as of `913a490bb`: no corpus run has been made at either
  later pin, so they are not yet its numbers.
- The counts of `bench/baselines/llvm.json` are those of the map task
  (WP-050: the parse task and the semantic digest's walk, `spec/model.md`
  §8.5) at `913a490bb`, so they hold the `digest.*` counters of every
  `ok` shard. Against the parse-only run at `62d190007`, `inputs.files`
  rose by 9730, `inputs.bytes` by 152 341 114 and `pp.tokens` by
  14 471 506: the nineteen units that now parse are counted (whether
  any other unit's inputs moved with the pin was not compared).
- Before the pin moved, the parse task on Kiln's front-end entry (WP-025)
  gave every unit that was not `failed` the shard the dependency-file flow
  of WP-010 gave it at `4bfd999da`: 4040 `ok` and 2 `diagnosed` shards
  with the same outcome and the same inputs, entry for entry.

## 9. Corpus invariants

The two invariants of `docs/design/06-engineering-plan.md` §3, as far as
M1 can state them: there is no transform yet, so "the identity transform
gives zero edits" is "the identity image verifies with nothing declared".

```
cargo xtask invariants a <corpus> [--slip <path>] [--only <substring>]… [--limit <n>] [-j <n>]
cargo xtask invariants b <corpus> [--slip <path>] [--only <substring>]… [--limit <n>] [-j <n>]
```

- The corpus must be configured (§8.3); otherwise the command fails with
  one line saying to run `cargo xtask corpus configure <corpus>`.
- `--slip` is the `slip` executable. Without it: `$CARGO_TARGET_DIR`, or
  `target` in the repository, then `release/slip`; if that file does not
  exist the command fails saying to run `cargo build --release -p
  slip_cli`. xtask does not build it.
- `--only`, `--limit` and `-j` are passed to `slip` unchanged. A run over
  a selection says so in its last line (`… (selection)`), and is not the
  invariant on the corpus.
- `slip` runs with the inherited environment and standard error; its
  standard output is read. A `slip` status other than 0, 1 or (for
  `verify`) 4 fails the command with the status.
- Exit status: 0 when the invariant holds, 1 when it does not, 2 for a
  usage error.

### 9.1 Invariant (a): parsing twice gives equal shards

`invariants a` makes two stores, `stores/a1` and `stores/a2` under the
corpus directory (each removed first if it exists, by walking it: xtask
deletes nothing outside the corpus directory), and in each runs
`slip parse --db <database> --store <store>` with the options above. The
two plans must have one id. It then reads the latest map run record of
each store and, for every observer of the plan that was selected, the
`shard` of its last `Job` entry (`spec/model.md` §9) and that shard's
status.

An observer is **equal** when both runs wrote a shard, neither is
`failed`, and the two content ids are equal: the whole shard, so the
outcome, the inputs, the counters and the `digests`. It is **failed in
both** when both shards are `failed` (nothing about a failed observer is
deterministic; it is counted and not compared). Anything else is
**different**.

```
invariant a <corpus>
plan <plan id>
observers <n>
equal <n>
failed_both <n>
different <n>
  <main file>: <status 1> <shard id 1> | <status 2> <shard id 2>       at most 50, in plan order
run <path of the first run record>
run <path of the second run record>
invariant a: holds                              or: FAILED (<n> observer(s) differ)
```

The invariant holds when `different` is 0 and at least one observer is
selected (over none it ends `FAILED (no observer selected)`;
`spec/landed/harness/WP-051.md` has the forms of a `different` line's
sides and the other rules of the run). It does not need a single
determinate unit: an indeterminate unit has a digest and a flag like any
other, and both must come out the same twice.

### 9.2 Invariant (b): the identity verifies

`invariants b` uses `stores/a1` if it holds a map run of the corpus's
plan made by this `slip` (the run record's `tool_slip` and `tool_kiln`
are those of `slip --version`), and otherwise makes it as §9.1 does. It
then runs

```
slip verify --plan <plan id> --store <store> --round 0 --report <corpus dir>/stores/b.json
```

with the options above: the identity image, nothing declared
(`spec/verify.md` §2, §5.1). From the report and the map run it prints

```
invariant b <corpus>
plan <plan id>
observers <n>                       selected
map_ok <n>                          of them, with an `ok` map shard
verified <n>
indeterminate_only <n>              mismatch, every finding indeterminate (spec/verify.md §7)
different <n>                       mismatch with another finding
not_parsed <n>
unverifiable <n>
failed <n>                          of the map_ok observers
never_parsed <n>                    not counted against the invariant
  <state>: <main file>: <message>   every map_ok observer not verified, at most 50, in plan order
run <path of the verify run record>
report <path>
invariant b: holds                              or: FAILED (<n> of <map_ok> observer(s) not verified)
```

The invariant holds when every selected observer whose map shard is `ok`
is `verified`, and there is at least one (with none it ends
`FAILED (no observer with an ok map shard)`). An observer that is `indeterminate_only` is not verified
and counts against it: (b) cannot hold on a corpus that has an
indeterminate unit, which is the form `digest.md` §10 gives the same
requirement. `different` above 0 is a defect of the walk (two parses of
one tree disagree, which (a) would also show) or a tree that changed
between the two phases (then G2 makes it `unverifiable`).

**At M1's exit** (`docs/design/06-engineering-plan.md` §5) both
invariants hold on the LLVM corpus, the whole plan, with no selection:

- (b) is clean: every `map_ok` observer is `verified`. There is no list
  of excepted observers and no list of excepted causes.
- So no unit of the corpus is indeterminate. A cause the corpus run shows
  (`digest.indeterminate.<cause>`, `digest.cause.*`, §9.3) is removed
  from the walk, whichever it is: `pending` by the package that writes
  the area, `unsupported` and `unaccounted` by a row of
  `spec/digest-kinds.md` and its code, `collision` by the key. A cause
  that is not removed keeps the exit waiting; it is not waived.
- "The corpora" of M1's exit is the LLVM corpus (the row `llvm` of
  `bench/corpora.toml`). A libc++ corpus is not M1's ("Open questions").

### 9.3 What a corpus run records

A map run of a corpus with digests is measured with the commands of §7:
`perf summarize` prints wall and CPU time, the peak resident sizes, and
every counter summed, which now includes `digest.*`
(`spec/model.md` §8.5): units, groups, bytes hashed, indeterminate units,
indeterminate keys by cause (`digest.indeterminate.<cause>`) and by cause
text (`digest.cause.*`). `digest.cause.unaccounted.walk.depth` is the
number of keys the walk's depth bound cut off.

- The **map overhead** of a corpus is the ratio of `cpu_s` of a map run
  with digests to `cpu_s` of the baseline that was recorded without them,
  both on one host, and the same ratio for `peak_rss_max` and
  `peak_rss_p50`. It is recorded where the run is recorded
  (`spec/landed/harness/`); the budget of
  `docs/design/06-engineering-plan.md` §4 is set from the first.
- The baseline of the corpus (§7.3) is recorded again from a map run with
  digests: its `counters` then hold every `digest.*` counter, compared
  exactly from then on. A change of the walk changes them; the commit
  that does says why.
- **The walk's stack.** `slip_digest::STACK_SIZE` must hold the walk's
  recursion bound with the margin its test asserts
  (`expr::tests::frames_fit_the_stack`) in the profile `slip` is shipped
  in: the test is run with `cargo test --release -p slip_digest --lib
  frames_fit_the_stack` and its figures recorded beside the debug ones.
  The gate's `kiln` step runs it in the test profile only.

### 9.4 Interface

`xtask::invariants`, with the helper `xtask::slip_run` that §10 shares:

```rust
// slip_run
pub fn find_slip(repo: &Path, given: Option<&Path>, target_dir: Option<&OsStr>) -> Result<PathBuf, String>;
pub struct Ran { pub status: i32, pub stdout: String }
pub fn run(slip: &Path, args: &[OsString]) -> Result<Ran, String>;
pub fn line<'a>(stdout: &'a str, key: &str) -> Option<&'a str>;       // the rest of the line `<key> …`
pub fn remove_tree(under: &Path, dir: &Path) -> Result<(), String>;   // refuses a `dir` not under `under`

// invariants
pub struct A { pub observers: u64, pub equal: u64, pub failed_both: u64, pub different: Vec<Different> }
pub fn compare_runs(plan: &Plan, first: &Loaded, second: &Loaded, selected: &[Digest]) -> A;
pub struct B { /* the numbers of §9.2 */ }
pub fn check_identity(map: &Loaded, report: &json::Value) -> Result<B, String>;
pub fn main(repo: &Path, args: &[String]) -> ExitCode;
```

`compare_runs` and `check_identity` are pure, so that the tests need no
`slip` and no corpus.

### 9.5 Census: causes of indeterminate units

```
cargo xtask census <corpus> [--slip <path>] [--only <substring>]… [--limit <n>] [-j <n>] [--store <dir>]
```

Counts indeterminate keys and non-`ok` shards over a configured corpus before
a run that needs `verified` (`docs/demos/demo-2.md` §1, §6.5). Locating
`slip`, checking that `<corpus>` is configured, and applying `--only`,
`--limit` and `-j` follow §9.

- **Store and reuse.** Default store: `<corpus dir>/stores/census`;
  `--store <dir>` overrides it. The command reads `<store>/census.json` and
  reuses the newest round-0 map run of the corpus's current plan in
  `<store>` without running `slip parse` only when all of the following
  match:
  1. `format` is `"slip.census.v1"` and `census.json` parses cleanly;
  2. `slip_digest` equals the content digest (`slip_model::hash::content_digest`)
     of the `slip` executable's bytes;
  3. `tool_slip` and `tool_kiln` match `slip --version` and the run record;
  4. `kiln_pin` matches the repository's `kiln` submodule `HEAD` (or the
     corpus row's tag when no `kiln` git checkout exists);
  5. `selection.only` and `selection.limit` match the current arguments;
  6. `plan` matches `slip plan --db <database> --store <store>`;
  7. the newest map run of `plan` in `<store>` is round 0, has no missing
     shards, and covers the exact selected observer set.
  When any check fails (or when in doubt), `slip parse` runs afresh: if
  `<store>` lies strictly under `<corpus dir>`, it is removed first via
  `slip_run::remove_tree`; if `--store` names an existing non-reusable
  directory outside `<corpus dir>`, the command fails without deleting it.
- **Aggregation.** Each selected observer's map shard is read from the store
  through `perf::load` and verified against the run record via
  `invariants::side`.
  - For each `ok` shard: every non-zero `digest.indeterminate.<cause>`
    counter (including `pending.<area>`) is summed into `causes` (keys,
    observers with at least one, and the first 5 such observers sorted by
    main file path bytes), and every non-zero `digest.cause.<cause>.<word>`
    counter is summed into `cause_texts` (keys and observers). To avoid
    double-counting `pending` alongside `pending.<area>`, total
    `indeterminate_keys` sums top-level `digest.indeterminate.<cause>`
    counters (or `pending.<area>` if `pending` is absent, and at least
    `digest.indeterminate` when present).
  - Every selected observer whose `side` is not `ok` is counted in `not_ok`
    and grouped in `not_ok_by_status` (`diagnosed`, `rejected`, `failed`,
    or `incomplete` for `NoJob`/`NoShard`/`Absent`/`Other`).
- **Output.** Standard output prints `census <corpus>`, `plan <id>`,
  `observers`, `ok`, `not_ok` (and per-status counts), `indeterminate_keys`,
  `indeterminate_observers`, `causes` (sorted by keys descending, name
  ascending; up to 5 observer paths each), `cause_texts` (keys descending,
  name ascending), `run <path>`, `json <path>`, and the last line:
  - `census: clean (<n> observers)` when `observers > 0`, `not_ok == 0`,
    and `indeterminate_keys == 0`;
  - `census: <k> indeterminate keys in <m> of <n> observers, <c> causes; <f> not ok`
    otherwise (with `(no observer selected)` when `n == 0`);
  - suffixed with ` (selection)` when `--only` or `--limit` was given.
- **JSON (`<store>/census.json`).** Written atomically (`census.json.tmp`
  renamed into place) with top-level keys `format` (`"slip.census.v1"`),
  `corpus`, `plan`, `run`, `tool_slip`, `tool_kiln`, `kiln_pin`,
  `slip_digest`, `selection` (`only`, `limit`), `clean`, `observers`, `ok`,
  `not_ok`, `not_ok_by_status`, `indeterminate_keys`,
  `indeterminate_observers`, `causes` (array of `{cause, keys, observers,
  sample_observers}`), and `cause_texts` (array of `{name, keys, observers}`).
- **Exit status:** `0` clean, `1` not clean, `2` for any command or usage
  error.

## 10. Demo 1

The first demonstration of the trusted base on real code: a correct
mechanical change to headers that every translation unit of a library
shares is **verified**, and each of a catalogue of small wrong edits to
the same code is **found**, with the entity, the file and the line.

```
cargo xtask demo 1 [--slip <path>] [-j <n>] [--case <name>]… [--keep]
```

It is scripted and repeatable: the inputs are committed, the corpus is
pinned (§8), every step is a `slip` command a person could type, and two
runs print the same table up to the times.

### 10.1 Inputs

`bench/demo1/`:

```
demo.toml             corpus, roots, selection
cases/<nn>-<name>/    one directory per case, run in the order of their names
  edit.txt            the edit, as below; absent for the control
  base                optional: the name of a case whose tree this one starts from
  declared.txt        the declared changes (spec/verify.md §2.3); may be absent (nothing declared)
  observers           optional: the observers in which this case's names are resolved, one `--only`
                      substring per line (for `slip declare` and `slip units`); default: the first selected
  expect.txt          what the case must give, as below
  README.md           what the edit is and why it is right, or wrong
```

`demo.toml` (read with the line reader of the lints): `corpus` (a row of
`bench/corpora.toml`), `roots` (an array of directories relative to the
corpus's `src/`: the subject roots), `only` (an array of substrings: the
selection, the `--only` of every `slip` run).

**`edit.txt`** is a list of exact replacements, so that an edit is small,
reviewable and fails loudly when the corpus is not the one it was written
for:

```
file <path relative to src/>
replace [all]
<lines of old text>
with
<lines of new text>
end
create <path relative to src/>
<lines of the file>
end
```

A `replace` applies to the `file` line above it; its old text must occur
in the file exactly once, or at least once with `all`; texts are compared
as bytes and end at the line before `with` or `end` (so a text cannot
hold a line that is exactly `with` or `end`; no edit of the catalogue
needs one). A path that leaves the roots, a `file` that does not exist, a
`create` of one that does, an old text not found or found twice, and an
`edit.txt` with no entry are errors that name the line.

**`expect.txt`** is one line: `verified`, or `mismatch` followed by the
findings the case must have, one per further line:

```
mismatch
  changed llvm::StringRef::find(char, unsigned long) const @ llvm/include/llvm/ADT/StringRef.h:282
  changed <name> @ <file>:<line>
```

A finding line is a kind (`changed`, `removed`, `added` or
`derived_changed`), a unit name, and optionally ` @ ` and a file relative
to `src/` with a line. The **image of a line** is the post-image for
`added` and the pre-image otherwise. A line is

- **found** when the report has a finding group (`spec/verify.md` §7.1)
  of that kind and name and, where a place is given, the group's
  `where.post` (for `removed`: `where.pre`) is that file and line;
  **found, no place** when the group has no `where` (localisation is not
  there yet);
- **wrong place** when the group has a `where` and it is another file or
  line;
- **blocked** when there is no such group and `slip units` shows, in the
  line's image and the case's `observers`, a unit of that name with the
  indeterminate flag: the walk could not digest the unit, so the edit
  cannot be shown to have changed it, and the report has it as
  `indeterminate`;
- **missing** otherwise: the unit was digested in full and the wrong edit
  went through.

### 10.2 The cases

The subset is the translation units of `llvm/lib/Support/` in the `llvm`
corpus (151 observers on the pin of `bench/corpora.toml`); the one
subject root is `llvm`. Every
header named below is included by most of them.

| Case | What is edited | Declared | Expect |
|---|---|---|---|
| `00-control` | nothing: the post-image is a copy | — | `verified` |
| `01-relocate` | the namespace `llvm::hashing::detail` is renamed `llvm::hashing::impl`: both blocks of `llvm/include/llvm/ADT/Hashing.h`, and every use in `llvm/` (`Hashing.h`, `StableHashing.h`, `lib/IR/StructuralHash.cpp`, `unittests/ADT/HashingTest.cpp`) | `relocate llvm::hashing::detail => llvm::hashing :: impl` | `verified` |
| `02-default-argument` | `StringRef::find(char C, size_t From = 0)` gets the default `1` | — | `mismatch`: `changed` the member at its line (a caller that relies on the default is not a finding: `pitfalls.md` E10) |
| `03-overload` | `StringRef` gains `bool starts_with(const char *) const`, which a call with a string literal now prefers to `starts_with(StringRef)` | `created` the new member | `mismatch`: `changed` one named caller in `lib/Support`, at the call |
| `04-adl` | a function template of the name of an `llvm::sys::path` function, taking `StringRef` exactly, is added to namespace `llvm`; a call in `lib/Support/Path.cpp` that converted a `StringRef` to `const Twine &` now reaches it through argument-dependent lookup | `created` the template | `mismatch`: `changed` the caller in `Path.cpp`, at the call |
| `05-shadow` | a declaration inserted in a function body of one `lib/Support` file gives a name used later in that body another meaning (the pattern of `spec/pitfalls.md` E12) | — | `mismatch`: `changed` that function, at the use |
| `06-relocate-and-change` | `01-relocate` (`base`), and one constant inside the relocated namespace changed | as `01-relocate` | `mismatch`: `changed` the unit that holds the constant, under its **old** name, at its line in the post-image |
| `07-relocate-undeclared` | `01-relocate` (`base`), nothing declared | — | `mismatch`: `removed llvm::hashing::detail` and `added llvm::hashing::impl` among others |

- Cases 02 to 05 are the hazards of the brief: a changed default
  argument, an overload that now wins, argument-dependent lookup, a name
  that now finds another entity. 06 is the reason the tool exists: a
  semantic change riding on a mechanical one. 07 shows that the declared
  changes are not taken on trust in either direction.
- Every wrong edit must be **silent**: the tree parses for every selected
  observer. A case one of whose observers is `not_parsed` fails. The
  package that writes the cases also compiles each tree's affected
  translation units with the corpus's compiler and records that in the
  case's `README.md` (tier T2).
- The hazards are fixed; the exact sites are what the package finds to be
  silent on this pin. A site that cannot be made silent is replaced by
  another of the same hazard, and this table is amended with it.
- A wrong edit's `created` line exempts only the added entity. The
  finding the case expects is on a unit nobody declared.

### 10.3 The run

In the corpus directory, under `demo1/` (removed first; `--keep` leaves
the trees of a finished run in place, otherwise they are removed at the
end and the reports stay):

1. `slip plan --db <database> --store demo1/store`, then
   `slip parse --plan <id>` with the selection.
2. From the map run: the selected observers `s`; those whose map shard
   is `ok` and has `digest.indeterminate` 0, `d` (the **determinate**
   observers); and the sums of `digest.units` and `digest.indeterminate`
   over the selection. Every verify run is over the whole selection: an
   observer with an indeterminate unit cannot be `verified`
   (`spec/verify.md` §4.1), and the demo shows that rather than hide it.
3. For each case, in order, with round `k` = its position:
   - the tree `demo1/trees/<case>/<root>` is made for every root, from
     the pristine root or from the `base` case's tree, as §11.3 says:
     what `edit.txt` writes is computed first; every file it writes is
     **created as a new file** (never a link, never opened for writing
     in place), every other file is a hard link to the file it comes
     from (a copy where linking fails), and every written file is read
     back and compared;
   - if `declared.txt` has a line: `slip declare --plan <id> --from
     declared.txt --out demo1/<case>.decl --owned <src>/<root> --root
     <src>/<root> --post <tree>` (one `--owned` for every root of the
     demo) with the case's `observers`;
   - `slip verify --plan <id> --round <k> --root … --post … [--declared
     …] --findings 100000 --locate 1024 --report demo1/<case>.json`, with
     the selection;
   - for a case with finding lines that are not found: `slip units`, with
     the same image and the case's `observers`, once without and once
     with `--post-image`, for the indeterminate flags;
   - the report is checked against `expect.txt` (below).
4. Last, every pristine file an edit touched is hashed and compared with
   its hash before the run; a difference is an error of the demo (an edit
   went through a link).

**The result of a case.** No result is given when an observer is
`not_parsed`, `unverifiable`, `never_parsed` or `failed`: the case is
then `ERROR` (the edit is not silent, or the run is broken).

| `expect.txt` | Result | When |
|---|---|---|
| `verified` | `verified` | the run's verdict is `verified` |
| | `clean` | every observer is `verified`, or `mismatch` with only `indeterminate` and `derived_indeterminate` findings (`indeterminate_only`, `spec/verify.md` §7): no unit that was digested differs, and the run is not verified |
| | `DIFFERENT` | an observer has another finding |
| `mismatch` | `found` | every line is found, and the run's verdict is `not_verified` |
| | `partly` | no line is missing or at a wrong place, and at least one is blocked or found with no place |
| | `MISSED` | a line is missing or at a wrong place; or the run's verdict is `verified` |

`demo 1: ok` admits no `blocked` line and no line found without its
place: one of either makes its case `partly` and the run `PARTIAL`. The
latitude of a blocked line (a name several units share is blocked if any
of them is indeterminate, §10.1) exists in the `PARTIAL` form only.

Standard output:

```
demo 1: <corpus> <pin>, <s> observers, <d> determinate, units <n> indeterminate <m>
map         wall_s <s> cpu_s <s> peak_rss_max <bytes>
<case>      <result>  observers verified <v> indeterminate_only <i> other <o>  units <n> indeterminate <m>
            findings <groups not indeterminate>  reparsed <n>  wall_s <s> cpu_s <s> peak_rss_max <bytes>
  found    <finding line>                                one per finding line of expect.txt:
  no-place <finding line>                                found, no place
  blocked  <finding line>: <cause, from the report's indeterminate group of that name if it has one>
  MISSING  <finding line>
  WRONG    <finding line>: at <file>:<line>
  pre  <file>:<line>:<column> <node> <what>              the `where` of each found finding
  post <file>:<line>:<column> <node> <what>
  <kind> <name> observers <n>                            DIFFERENT: its first ten groups that are not indeterminate
indeterminate <cause counter> <n>                        if m is not 0: the summed `digest.indeterminate.*` and the
                                                         ten largest `digest.cause.*` of the map run
demo 1: ok                       every case `verified` or `found`
     or: PARTIAL (<n> clean, <n> partly)                 no case DIFFERENT, MISSED or ERROR
     or: FAILED (<cases>)
```

The times and sizes are those of the run record of each `slip` run
(§7.1); `units` and `indeterminate` of a case are the sums of
`digest.units` and `digest.indeterminate` over its verify shards (the
post-image), and `reparsed` the sum of `verify.reparsed`. The same
numbers, the plan id, the tool versions and each case's finding groups
that are not indeterminate are written to `demo1/demo1.json`. Exit
status: 0 for `ok`, 4 for `PARTIAL`, 1 for `FAILED`.

### 10.4 What it shows, and what it does not

It may be said of a run that printed `demo 1: ok`:

- for the `s` translation units named, Slip showed that the relocation
  left every entity, type, overload choice and instantiation as it was
  under one declared rule, in the time and memory printed;
- each planted change of meaning was reported, at the entity and line
  printed, and none of the wrong trees was reported verified.

Of a run that printed `PARTIAL`, only this:

- for a `clean` case: of the `n` units of the selection, the `n − m`
  that Slip digests in full are the same in both images under the
  declared rule. The other `m` were not compared, for the causes printed,
  and **the change is not verified**: `slip verify` said so, with exit
  status 1. A unit digested in full says nothing about the units it
  refers to;
- for a `partly` case: the lines printed `found` were found; a `blocked`
  line is a hazard Slip cannot yet show either way, and a `no-place`
  line was found without its line;
- no wrong tree was reported verified.

A `PARTIAL` run is a statement of how far the walk has come, not a
weaker kind of verification. It is the earliest form of the demo, and it
becomes `ok` without any change to the demo when the walk leaves no unit
of the selection indeterminate and localisation has landed.

It may not be said of either that:

- Slip made the edits. They were written by hand; the demo is of
  verification, not of transformation;
- LLVM verifies. The claim is about the selection;
- nothing else can be wrong with an edit that verifies: one front end,
  one configuration, per-observer agreement, generated files and text
  are the limits of `spec/verify.md` §8;
- the catalogue is complete. `spec/pitfalls.md` is the catalogue; these
  are five of its hazards on real code;
- the times are those of a full-corpus run.

### 10.5 Interface

`xtask::demo`:

```rust
pub struct Config { pub corpus: String, pub roots: Vec<String>, pub only: Vec<String> }
pub fn read_config(text: &str) -> Result<Config, String>;
pub enum Entry { Replace { file: String, all: bool, old: Vec<u8>, new: Vec<u8> }, Create { file: String, content: Vec<u8> } }
pub fn read_edit(text: &[u8]) -> Result<Vec<Entry>, String>;                 // errors name the line
pub struct Written { pub path: PathBuf, pub content: Vec<u8>, pub mode: Option<u32> }
pub fn plan_edit(from: &Path, entries: &[Entry]) -> Result<Vec<Written>, String>;   // writes nothing
pub enum Make { Link, Copy }
pub const TREES: Make;                                                       // §11.3
pub trait TreeOps { /* create_dir, hard_link, copy, symlink, write_new, read */ }
pub struct Disk;                                                             // TreeOps on the file system
pub fn build_tree(ops: &mut dyn TreeOps, from: &Path, to: &Path, roots: &[String],
                  written: &[Written], make: Make) -> Result<u64, String>;   // files linked, copied or symlinked
pub fn read_back(ops: &mut dyn TreeOps, to: &Path, written: &[Written]) -> Result<(), String>;
pub enum Expect { Verified, Mismatch(Vec<Wanted>) }
pub struct Wanted { pub kind: String, pub name: String, pub at: Option<(String, u64)> }
pub fn read_expect(text: &str) -> Result<Expect, String>;
/// The names `slip units` printed with the indeterminate flag (spec/exec.md §8.9).
pub fn indeterminate_units(stdout: &str) -> BTreeSet<String>;
pub enum Line { Found, NoPlace, WrongPlace { file: String, line: u64 }, Blocked, Missing }
pub enum CaseResult { Verified, Clean, Different, Found, Partly, Missed, Error(String) }
/// §10.3 "The result of a case"; `pre` and `post` are `indeterminate_units` of the two images.
pub fn check(expect: &Expect, report: &json::Value, src: &Path,
             pre: &BTreeSet<String>, post: &BTreeSet<String>) -> (CaseResult, Vec<Line>);
pub fn main(repo: &Path, args: &[String]) -> ExitCode;
```

## 11. A file system that confuses the names of a hard-linked file

### 11.1 The hazard

On some file systems a file with two names does not keep them apart. On
the workspace's mount (a container bind mount over a host share), with a
file `A` and a hard link `B` to it: unlink `B`, create a new file `B`, and
a read of `A` now and then returns the new `B`'s bytes; rename a new file
over `B`, and a read of `A` returns them almost every time. With `B` a
copy, never a link, no read is wrong. The count is in
`spec/landed/harness/WP-159.md`. The pattern is general: **a name made
again (by create, link or rename) while another name of its old file is
alive** can make that other name read as the new file.

Slip reads what the system gives it and does not work around this in the
product (`spec/edits.md` §10.4 says what a write promises about names).
The harness and the store must not walk into it: Slip's validation must
not depend on a mount behaving.

### 11.2 The probe

```
cargo xtask fscheck <dir> [--rounds <n>] [--row <name>]…
```

Each row repeats a sequence `<n>` times (default 20,000) in a new
directory `<dir>/fscheck.<pid>.<row>`, removed at the end, reusing its
names every seven rounds as a tree made again reuses them, and reads the
names it left with known contents. A **wrong read** is a read that
returns other bytes or fails, or a step of the sequence that fails (a
`create_new` of a name just unlinked that finds it still there); the row
goes on.

| Row | Sequence | Relied on by |
|---|---|---|
| `link-unlink-create` | `B` a link to `A`; `B` unlinked and created again | nothing (the harness's trees before WP-159) |
| `link-rename-over` | `B` a link to `A`; a temporary renamed over `B` | nothing (`--write` refuses a linked file, §11.4) |
| `copy-unlink-create` | `B` a copy of `A`; `B` unlinked and created again | a copied tree |
| `copy-rename-over` | `B` a copy of `A`; a temporary renamed over `B` | the store's publish over an entry, `--write`, `--patch`, `--report` |
| `fresh-beside-links` | a tree of links with one file created fresh in it | the harness's trees (§11.3) |
| `link-read-both` | `B` a link to `A`, read through both | the trees' unedited files, the tests of identity |
| `tmp-again-after-link` | `T` linked to `L` and unlinked; `T` created again | nothing (the store's lock and journal before WP-159) |
| `tmp-again-after-rename` | `T` renamed to `D`; `T` created again | the patch's and a report's temporary, staged names |
| `unique-tmp-link` | `T` linked to `L` and unlinked; `T`'s name never used again | nothing (the store's lock and journal before WP-161) |
| `create-new-read` | `L` created exclusively, written and synced in place, read by name; a claim `C` created exclusively (a name never used again), listed by its directory, removed and no longer listed; `L` read again | the store's lock, its claims and the journal (`spec/exec.md` §2.1) |
| `create-new-xproc` | a claim `C` (a name never used again) created exclusively and a file `J` created exclusively, written and synced (16 MiB every 1,000th round, round 0 among them, a journal of a write of about 100,000 files; 256 KiB every other eighth round, as a journal; else 32 bytes, as a lock), the directory synced; another process (`xtask fscheck-reader`, over a pipe) lists `C` and reads `J` whole; `C` and `J` removed; the other process no longer lists `C`. A reply other than `ok` is a wrong read | the lock's claims seen by other processes, and a journal read whole by the next process (`spec/exec.md` §2.1, `spec/edits.md` §7.3 step 2) |
| `inode-stable` | files created fresh, their device and inode numbers read, read again at once and 2 s later | a test's work directory (§11.5) |

Output: `fscheck: <dir>: <n> rounds per row`, then per row
`<row> rounds <n> wrong <w> <seconds>s  relied on|not relied on: <sequence>`
and, if `w` is not 0, `  first: <name> read <bytes>, not <bytes>` (or
`a step failed: <error>`); last `fscheck: ok`, or
`fscheck: FAILED (<rows>)` naming the relied-on rows with a wrong read
or an error. Exit status 0 for `ok`, 1 for `FAILED` or a `<dir>` that is
not a directory, 2 for a usage error (`--rounds` not a positive integer,
an unknown row or option, a second directory). `--row` runs only the
rows named, in the order of the table.

`inode-stable` creates its files in batches of 1,000 (every other one
written as a temporary and renamed to its name), reads each one's
device and inode numbers as it is made, reads them again when the batch
is made and 2 s later, and removes the batch; a **wrong read** is a
file whose numbers changed (or could not be read), counted once, and
its first is `  first: <name> was <dev>:<ino>, then <dev>:<ino> after
<s>s`. It waits 2 s per batch: 40 s at the default rounds.

The rows not relied on are there to show the hazard; their counts vary
with the load. The second row shows within a few thousand rounds; the
first needs a loaded host and tens of thousands.

### 11.3 What the harness relies on

**A harness never replaces a name that is a hard link.** A tree built from
another (Demo 1's case trees, `crates/slip_cli/tests/names_relocation.rs`)
knows the files its edits write before it is made: every file an edit
writes, or creates, is created as a new file (`create_new`) and is
never a link at any moment; every other file is a hard link to the file
it comes from (`fresh-beside-links`, `link-read-both`), or a copy where
linking fails. After building, every written file is read back and
compared, a check and not the guarantee. Were `fresh-beside-links` not
clean on the mount, the trees would be copied (`demo::Make::Copy`).

### 11.4 Hard links in tests and in the store

A test that needs two names of one file (identity) keeps its hard link
if nothing replaces either name while it reads the other, or the name
replaced gets the same bytes, and says so beside the link. A test that
links only to save a copy copies. The store makes no name again while
another name of its old file is alive (`spec/exec.md` §2.1 "`tmp/`
names"); which of its reads are checked against a digest is
`spec/exec.md` §2.1 "Which reads are checked".

On the workspace mount `unique-tmp-link` is not clean (WP-159): after a
link and the unlink of its source, the new name can read as absent for a
while, never as other bytes. Nothing relies on it: the store's lock and
journal are created exclusively under their own names and written there
(WP-161, `spec/exec.md` §2.1), which is the row `create-new-read` (one
process) and `create-new-xproc` (two), and the store makes no hard link.
These rows are the evidence for the store's assumption (L) (`spec/exec.md`
§2.1), which is about what the processes of one host see; an `fsync` that
the mount does not pass through to the host's disk (a power loss of the
host after a sync returned) is outside (L) and not addressed.

### 11.5 A file system that changes the inode numbers of new files

On the workspace's mount the device and inode numbers of a file just
created change now and then, in bursts: of 4,800 fresh files about 800
had other numbers 2 s later in one run, none in several others, none in
files held open (`spec/landed/harness/WP-162.md`). The front end
identifies a file by these numbers (`kiln_source`: an overlay entry is
keyed when it is added, a file when it is loaded), so a burst between
the two makes the entry miss and the post-image's parse reads the file
from the disk. Slip's guards make that loud (`not_parsed`,
`unverifiable`, `incomplete`; never a wrong `verified`), and a test
fails at random. Row `inode-stable` (§11.2) is the probe; on this mount
it fails now and then, and **one clean run of it does not show that a
directory is safe**: the bursts come and go with the host.

**The rule.** A test's **work directory** (the trees it writes and then
has a front end, `slip` or `slip_apply` read, its stores and databases)
is under `$SLIP_TEST_ROOT` when that variable is set and not empty, and
where it was before otherwise. The value must be an absolute path to a
directory, made if missing; nothing in the repository depends on what
it is. Only work directories move: the build, the target directory and
the `slip` a harness runs stay where they are, and nothing is compiled
under `$SLIP_TEST_ROOT`. A workspace sets it to a directory on a local
file system (its `scripts/env.sh`).

- **`cargo xtask transform`**: the work base is
  `$SLIP_TEST_ROOT/transform/<checkout>/`, `<checkout>` the last
  component of the repository's root (two checkouts never share one),
  else `<target>/transform/`; a test works in `<base>/<area>/<name>/`
  (§6.3), and the line that names a kept directory prints that path.
  A value that is not absolute is `transform: SLIP_TEST_ROOT=<value>: not
  an absolute path`, exit status 1, before any test runs.
- **The crates' tests**: one helper in each crate's `tests/common`
  (`work_root()`, a single file included by each), giving
  `$SLIP_TEST_ROOT/<package>/<test binary>.<pid>.<token>/`, one
  directory per test binary and process, else Cargo's
  `CARGO_TARGET_TMPDIR`. The directory holds a lock on its file `.lock`
  for the life of the process; the first call of a later process of the
  package removes each sibling whose lock is free and that is older than
  10 s, so the root holds the directories of running processes and of
  the last ended ones. Where a lock does not exclude another (the
  workspace's mount) nothing is removed. A test removes what it made as
  it did before; which tests use the helper, and which do not and why,
  is the landed note's list.
- `cargo xtask demo`, `corpus`, `invariants` and `fscheck` take their
  directories from the corpus or the command line and do not read the
  variable.

**The folding root** (`spec/landed/edits/WP-151.md`). A test ABOUT
spelling on a volume that folds case and Unicode normalisation (another
case or normalisation of a path names the same file) needs such a
volume, and `$SLIP_TEST_ROOT` is not one. Its work directory is under
`folding_root()` (beside `work_root()` in `tests/common`, the same
file): `<CARGO_TARGET_TMPDIR>/folding/<package>/<name>/`, `<name>` the
work directory's own when `$SLIP_TEST_ROOT` is set (one per test binary
and process) and swept with it by that directory's lock on the local
file system (a lock on the mount does not exclude); without the
variable, a lock and a sweep of its own as `work_root()`'s. On this
project's host that is the workspace's mount, which folds, and the
hazards above apply there: a fresh file can change inode number, and a
hard link can read another name's content (WP-159). So only these
tests live there, never a hard link, and a test that parses through the
front end only with the evidence that it holds (twenty runs in a row
with no failure of the hazard's shape, `spec/landed/edits/WP-151.md`).
Every such test asks `folding_volume(test)` (or `folding_checked(test)`
for one with a half that runs on either kind): it **fails** when the
source tree's volume folds (the gate's host) and the folding root does
not, and only where no volume folds does it skip, printing one line
with the reason. The guard `s11_5_folding_root` of each crate with
such tests prints which kind each root is, and fails in the same case.

## Tests

- `xtask/tests/lint.rs` — §3: for each lint, a fixture tree that passes and
  one that fails with the expected message.
- `xtask/tests/ci.rs` — §2: step selection, order, output lines, exit status,
  with fake `cargo`.
- `xtask/tests/wp.rs` — §4.
- `xtask/tests/jobserver.rs` — §5, over fifos the test makes and holds
  open: `status` (tokens left in place), `refill` short, full and above
  `<n>`, `<n>` from `$SLIP_JOBS` and the default, fifo precedence
  `--fifo` > `MAKEFLAGS` > `$SLIP_JOBSERVER_FIFO` (and the descriptor form
  falling through), the default resolved without opening it, `<n>` precedence, bad
  `<n>` from either source, a missing path, a regular file and a directory,
  the usage errors, and the executable with the host's variables cleared.
- `xtask/tests/perf.rs` — §7, over stores the test writes (plan, shards,
  run record built with `slip_model`), with golden output in
  `xtask/tests/golden/perf/`: path shapes, records disagreeing with their
  path, every counter row, a missing shard, a shard of another observer,
  phase or round, an invalid shard, overflow, counter-name collision, an
  observer not in the plan, percentiles and measured over 0, 1, 2, 100 and
  101 jobs, seconds rounding, summary goldens of a full and a cancelled run
  with ties, efficiency `-`, the ten entry limit and escaped paths, two
  records through the executable, the baseline golden, partial coverage
  refused, `record` then `check`, every kind of check difference, malformed
  baselines, and the executable's usage, unknown-corpus and
  unreadable-record errors.
- `xtask/tests/transform.rs` — §6.2 to §6.4 with a fake `slip`: every
  directive and its default, each malformed-test error, each outcome, the
  normalisation of ids and paths, `--bless` and its refusals, the
  post-image comparison (a file or directory added, missing, different in
  bytes or mode, of another kind), the output's structure, each lie of the
  store's records, a write the result does not permit, writes outside the
  tree, the timeout, discovery, each oracle passing, failing and skipped,
  the mutation sample for 1, 8 and 100 edits, a skipped clang and
  `SLIP_REQUIRE_ALL`.
- `tests/transform/` — the transform tests themselves, run by the
  `transform` step of the gate.
- `xtask/tests/corpus_rename.rs` — §6.5 over a local corpus of three
  units (a `.git`, a link and a subdirectory beside them) with stand-in
  `slip`, clang, `cmake` and `make` and the real `patch`: a rename that
  verifies through every check, with the calls it makes (`--patch` on
  the corpus, `--write` only on the copy) and the steps' lines in order;
  `--expect refused`, and a refused result without it; a `reduce` that
  gives another edit set id; `--accept` with a unit rejected on both
  images (passes) and one rejected only on the post-image (fails); the
  result file's members; counters that differ fail and leave the file,
  times do not; a failing `back` and a failing build keeping a copy that
  is a true copy; usage errors; the copy and the tree comparison.
- `xtask/tests/corpus.rs` — §8: rows and their Kiln pins, every
  malformed-row error, the committed `llvm` row (§8.1); root and layout
  (§8.2); `path`, `list` and usage errors through the binary (§8.3); the
  missing-tool message (§8.3); stamps with fake `cmake` and `make` (§8.4);
  the generated check including a dependency's outputs (§8.6); the lock
  (§8.5); fetch and configure of a local git repository with a three-file
  CMake project and one generated header, and a fetch of a missing tag
  (reported `SKIP` without `git`, `cmake`, `make` and a compiler). Unit
  tests in `xtask/src/corpus/build_tree.rs`: the database entry count and a
  target's outputs.
- `xtask/tests/invariants.rs` — §9, over stores and reports the test
  writes and a fake `slip` (a script that prints fixed lines and writes
  fixed files): `find_slip`; `compare_runs` for equal, different,
  failed in one, failed in both, an observer missing from a run; the
  output of `a` and `b` and their exit statuses; `check_identity` for
  each state; a `slip` that exits 3; a corpus that is not configured;
  `remove_tree` refusing a path outside the corpus directory.
- `xtask/tests/demo.rs` — §10 without a corpus: `demo.toml`; every form
  and every error of `edit.txt`; `plan_edit` and `build_tree` on a tree
  the test makes (§11.3), with the pristine file unchanged and never
  linked by an edited name, the calls recorded to show that a written
  name is only ever created new, and a planted difference found by the
  read-back; `expect.txt`; `check` against report goldens: every row of the
  result table, each kind of line (found, no place, a place that is one
  line off, blocked, missing, `removed` taking the pre site, `added`
  blocked by the post-image), a `clean` run never printed as `verified`,
  an observer `not_parsed`; the table and the three last lines with a
  fake `slip`, and the three exit statuses.
- `xtask/tests/fscheck.rs` — §11.2 over a temporary directory: the
  command line, every row run with its line and nothing left behind,
  status 0 where every relied-on row is clean, a wrong read planted
  through the reader failing a relied-on row and not one that is not
  relied on, a directory that does not exist; §11.5's `inode-stable`
  waiting and clean here, and a change of numbers planted through the
  reader of numbers failing it.
- §11.5 — `xtask/tests/transform.rs`: the work base with
  `$SLIP_TEST_ROOT` set, empty, unset and relative; a run with it set
  works under it, makes nothing under the target directory, removes a
  passing test's directory and prints a kept one's path there. Each
  crate whose tests use the helper runs its `s11_5_work_root`: the
  directory is under the variable (one per process, locked) or is the
  fallback, and a sweep removes an ended process's directory only.
- The corpus runs themselves (`invariants a llvm`, `invariants b llvm`,
  `demo 1`) are not in the gate: they need the corpus, and are run and
  recorded by the packages that land them and by the merge tier (§2).

## Open questions

- **`back` on a corpus needs a second tree** (§6.5): the inverse run has
  to parse the post-image as a pre-image, and a run cannot yet take an
  overlay as its subject. With that (wanted anyway for running one
  transform after another, M3), `back` and `--accept` would need no copy.
- **`--build` is expensive**: a copy of the source directory, a configured
  tree and a full clang build, per target renamed. Whether a release
  needs it for every sample of invariant (c), or for one target per run
  with `--accept` for the rest.
- **An independent equivalence oracle** for transforms (tier T3 of
  `docs/design/03-equivalence.md`): compiling both images with clang and
  comparing the objects' symbols and code modulo the declared renames.
  Not specified.
- The baseline of a corpus holds every `digest.*` counter once §9.3's
  run is recorded, so each change of the walk changes the baseline. A
  second file per corpus for the counters that are expected to move, or
  a check that compares only the `inputs.*`, `pp.*`, `plan.*` and
  `shard.*` families exactly, would keep `perf check` useful while the
  walk is being written.
- Invariant (a) costs two map runs of the corpus (§9.1). Reusing the run
  of the baseline as the first would halve it; it needs the baseline run's
  store kept.
- Demo 1 (§10) has a `PARTIAL` form for a walk that is not finished.
  `blocked` trusts `slip units` for the indeterminate flag of a name,
  and a name that several units share is blocked if any of them is
  indeterminate; a hazard hidden behind such a name would be `blocked`
  where it should be `MISSING`. Accepted for `PARTIAL` only: `ok` admits
  no blocked line (§10.3).
- A libc++ corpus: a row of `bench/corpora.toml` for libc++'s own
  headers and tests under its own build, with its baseline and the two
  invariants of §9. It is a later package and not part of M1's exit,
  which is stated on the LLVM corpus (§9.2). The LLVM corpus is built
  against the host's standard library, so it says little about libc++'s
  headers as a subject.
- Whether a `PARTIAL` run is something to show outside the project is
  the operator's call; §10.4 says what it may be said to show.
- `edit.txt` (§10.1) is one more small format. A unified diff would be
  the obvious one; it was not chosen because applying one faithfully
  (fuzz, offsets, file modes) is more than the demo needs, and `patch`
  is one more tool on `PATH`.
- The demo does not compile the trees with the corpus's compiler itself
  (§10.2): doing so needs the tree image's command rewriting
  (`spec/verify.md` §5.1) a second time in xtask, or a `slip` command
  that prints the post command.
- The `inputs.*` counters include system headers, so a baseline is exact
  only for one set of installed headers (the host compiler's and the
  standard library's). The baseline does not record which; a change of the
  container image shows up as a counter difference with no stated cause.
- Nothing checks that a shard in the store is the one the record's job
  wrote (`Job.shard` is its content id): a later run of the same plan,
  phase and round replaces shards. Counters of shards that are not `failed`
  are the same either way, which is why this is left open.
- `measured` has no gate. A regression budget (Kiln fails at a stated
  percentage on dedicated hardware) needs a quiet host; ours is shared.
- Hosted CI (GitHub Actions) for the Slip repository: not set up; the gate
  runs on the development host for now.
- `forbid-unsafe` looks for crate roots in the standard places only
  (`src/lib.rs`, `src/main.rs`, `src/bin/`). No member sets a `[lib]` or
  `[[bin]]` `path` elsewhere today; if one must, the lint has to follow it.
- Whether Slip and Kiln should share harness code as one crate (request KM2
  in `docs/design/05-kiln-interface.md`). Until then Slip ports what it
  needs, with attribution.
- `git` and `cmake` (§8.3) are not jobserver clients and run without a
  token (§1 principle 5). Starting them through `slip_exec::process`, which
  holds one, would account for them on the shared host.
- The corpus lock is a file holding a process id, which only excludes within
  one container (`flock` does not exclude on the bind-mounted workspace).
  Corpora shared between containers need something else.
- `corpus configure` does not report a generator target that the row's
  `generate` list fails to reach; a pin change can leave the list short and
  only a failed parse would show it.
