# Work packages

One file per work package, `WP-NNN.md`. Ids are sequential and never reused.
There is no shared table to conflict on: the list is this directory, and
`cargo xtask wp` prints it as a table (id, title, milestone, track, needs,
status).

## File format

```markdown
# WP-NNN — <title>

- Status: todo | in-progress | review | done | dropped
- Milestone: M0
- Track: A
- Needs: WP-001, WP-003        (or: none)
- Size: S | M | L
- Parallel with: WP-002        (optional)

## Scope
## Interface
## Tests
## Done when
## Out of scope
## Handback                    (filled in when the WP is done: branch head,
                                what landed, follow-ups and their fate)
```

The first seven lines are checked by `cargo xtask lint`: the title line, a
status from the list above, and `Needs` naming only existing files.

Tracks are those of `docs/design/06-engineering-plan.md` §6: A model and
formats, B Kiln adapter, C trusted base, D pipeline, E transform universe,
F refactoring library, G harness and corpora, H preprocessing and macros.
