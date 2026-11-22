# Working model for agents

A supervisor agent plans, writes specifications, briefs, reviews and merges.
Implementation is done by subagents, one **work package** each. This file is
the contract between them. It is Kiln's (`kiln/docs/AGENTS.md`) adapted.

## Work packages

A work package (WP) is a file `docs/wp/WP-NNN.md` ([format](wp/README.md))
with:

- **Scope**: the crate or module, the `spec/` sections it implements, and
  what is out of scope.
- **Interface**: the Rust API it must expose (signatures) or the spec section
  that defines it. Interfaces are fixed before parallel work starts.
- **Tests**: what it must add or make pass, by path.
- **Done when**: concrete and checkable.
- **Needs**: work packages that must land first.

A WP is sized so one subagent finishes it in one session with a reviewable
diff: one module, one spec section, under about two thousand lines, at most
five deliverables. Three small WPs are better than one large one.

## Rules for implementers

1. **Work only in the worktree the supervisor made for you**,
   `.worktrees/wp-<id>` on branch `wp/<id>-<slug>`, addressed by absolute
   path in every command. Never on `main`, never in another worktree. Do not
   use the Agent tool's own worktree isolation.
2. **Never `git stash`.** `refs/stash` belongs to the repository, not the
   worktree, so two agents stashing concurrently pop each other's changes.
   Commit work in progress, or use `git diff > x.patch`.
3. **Every build goes through the shared environment**: `source` the
   `scripts/env.sh` named in your brief before any `cargo` command. It puts
   the pinned toolchain on `PATH`, joins the host's jobserver, and gives your
   worktree its own target directory. Never set `-j` yourself and never share
   a target directory. Do not build the crates that link Kiln (`slip_kiln`,
   `slip_digest`, `slip_script`, `slip_cli`) unless your WP is in one of
   them: use `-p <crate>` or the default members.
4. **Read first**: `README.md`, `spec/README.md`, the spec sections in your
   brief, and the crate's `lib.rs` documentation.
5. **Safe Rust.** `#![forbid(unsafe_code)]` stays. The toolchain is pinned to
   Rust 1.95: no std API, language feature or lint name stabilised later.
6. **No new third-party dependency** unless the WP names it. The workspace
   has almost none on purpose; std is usually enough.
7. **Errors, not panics**, for anything derivable from input (files, command
   lines, formats). Internal invariants use `unreachable!`/`debug_assert!`
   with a message. Never truncate or guess on malformed input: reject it with
   the error the spec names.
8. **Every behaviour has a test, and every test names the spec section it
   covers** in a comment or its name. A spec gap you find is reported, not
   papered over: if the spec is silent or wrong, stop guessing, choose the
   most conservative behaviour, and list it under "Open questions" in the
   handback.
9. **Do not change an interface outside your WP.** If you must, record it in
   the spec and flag it in the handback; others are building against it.
10. **Behaviour your WP lands that the spec does not yet describe** goes in
    `spec/landed/<spec>/WP-NNN.md` (`spec/README.md` "Landed sections"),
    never appended to the spec.
11. **Before handing back**: `origin/main` is merged in and
    `cargo xtask ci --quick` passes (`cargo xtask ci`, the full gate, if
    your WP is in a crate that links Kiln). Commit on your branch. Never
    push, never merge to `main`.
12. **Hand back with**: the branch head; a summary; the commands you ran and
    their results; open questions; and follow-ups written as `file:line` plus
    one sentence each, so the supervisor can file them.

## Review gates (supervisor)

- **Spec conformance**: the diff does what the spec says, no more, no less.
- **Tests**: the obligated tests exist and would fail if the behaviour broke.
- **Safety and style**: `forbid(unsafe)`, clippy, no panics on input.
- **Interfaces**: no unrecorded drift.
- **Performance**: counters and baselines updated with a reason where they
  moved.
- **Trusted base** (`slip_digest`, `slip_verify`): a second, independent
  review that is given the spec and the code but not the author's reasoning.

Only the supervisor merges to `main` and pushes.

## Follow-ups

At handback every follow-up is decided on the spot: scheduled (it gets a WP
file), deferred with a reason (a line in the relevant spec's "Open
questions"), or dropped. There is no unbounded list of open rows.
