# Slip

Slip is the code transformer of the **Mason** toolchain (Kiln: compiler front
end, Hod: code generator, Mortar: linker, Tuck: debugger).

It is a semantically aware refactoring and large-scale code transformation
tool for C++. Transformations are written in general-purpose constexpr C++
using reflection and Slip-specific intrinsics, and run on top of Kiln's
preprocessor, parser, AST and semantic analysis.

Goals, in priority order:

1. **Correct** — a transformation must preserve the meaning it promises to
   preserve, and Slip must be able to validate that.
2. **Memory safe** — written in safe Rust.
3. **Programmable and extensible** — transformations are reusable, composable
   programs with a stable API.
4. **Fast and lean** — scales to codebases of tens of millions of lines, across
   many cores, with wall-clock time and memory tracked as part of validation.

## Status

Design phase. Nothing here is usable yet. The end-to-end design lives under
`docs/` once written.

## A first example: renaming a function

Three files: `f` declared in a header, defined in one source file and
called in another.

```
$ cat src/h.h src/a.cpp src/b.cpp
#pragma once
int f(int);
#include "h.h"
int f(int x) { return x + 1; }
#include "h.h"
int g() { return f(2); }
```

With a compilation database `compile_commands.json` that compiles
`src/a.cpp` and `src/b.cpp`, one command parses both, has each observer
claim the edits it sees, reduces the claims to one edit set, builds the
post-image in memory, parses every observer again over it, checks that
every name still denotes what it did (the function under its new name),
looks for text that spells `f` and that no observer accounted for, and
only then writes:

```
$ slip rename --db compile_commands.json --from f --to increment \
      --owned src --store .slip --write
plan ebcb39cd…
observers 2
excluded not_a_compile 0
excluded unsupported_language 0
excluded unsupported_flag 0
excluded missing_file 0
excluded malformed_entry 0
excluded unsupported_compiler 0
transform 2bb569fb…
round 1
map: selected 2, ok 2, not ok 0, failed runs 0
edits 56538ebd…
state accepted
edit 3 in 3 file(s); shared 1; most observers 2; nested 0
kept 0; unresolved 0
declared 058fc90e…: rules 1, exempt 0
overlay d264a05f…: files 3, spellings 1
verify: verified 2, unchanged 0, mismatch 0, not_parsed 0, unverifiable 0, never_parsed 0, failed 0
agreement: stale 0, unused rules 0
spellings: passed 1, failed 0
residue: dependent 0, not_compiled 0, not_planned 0; mentions 0
result 60c79d96…
state verified
written 3 file(s)
$ cat src/h.h src/a.cpp src/b.cpp
#pragma once
int increment(int);
#include "h.h"
int increment(int x) { return x + 1; }
#include "h.h"
int g() { return increment(2); }
```

(The ids are 64 hexadecimal digits; they are shortened here, and the
plan's depends on where the tree is.) Without `--write` nothing outside
the store changes; `--patch <file>` writes a patch instead. Any state
but `verified` writes nothing unless an `--allow-…` option says what may
be accepted, and the lines after `state` say why. The same run as
separate stages is `slip map`, `slip reduce`, `slip check` and `slip
apply` (`spec/edits.md` §10).
