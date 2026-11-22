# KR1 amendment 1 — using the written-reference table from a library

A request from Slip to Kiln. Status: **draft, not sent.**
Amends [KR1](KR1-written-reference-table.md). Found while specifying
`spec/api.md` §6 against Kiln main `0295475a3`; the reading is in
[kiln-landed-2026-11.md](../design/notes/kiln-landed-2026-11.md) §5.

## Design or implementation

| Part | Kind |
|---|---|
| §1 A switch that records and prints nothing | **Implementation.** |
| §2 A name's entry and its expression agree on location | **Implementation**: a sentence in Kiln's specification and a test. |
| §3 Where a leading `::` is | **Implementation**, with a small choice of form left to Kiln. |
| §4 Which stage Slip needs next | Scheduling, not a change. |

There is no part without prior art: the table's design was settled in KR1
§6.

## 1. A switch that records and prints nothing

Recording is `ParseOptions::written_refs`
(`kiln/crates/kiln_parse/src/lib.rs:140`). The only way to set it is the
driver flag `-fkiln-dump-written-refs`
(`kiln/crates/kiln_driver/src/args.rs:2032`,
`kiln/crates/kiln_driver/src/syntax_cmd.rs:383`), which also appends the
dump to the output (`syntax_cmd.rs:647-648`).

Requested: a field of `FrontEndOptions` (the options of `run_front_end`,
Kiln WP-2120), `written_refs: bool`, default off, that turns recording on
and prints nothing. The closure then finds the table on the tree
(`Ast::written_refs` or as Kiln names it).

Also requested, in the same place: that the table can be read after
`run_front_end`'s closure has the finished unit, with the same content as
the dump would show. (It is so today; this asks for it to be said.)

## 2. A name's entry and its expression agree on location

Slip finds the reference that names a call's callee by matching the
callee expression (`DeclRefExpr` or `MemberExpr`) to a table entry. The
only key the two share is location: the expression's name location and the
entry's `range.lo`. `kiln/spec/ast.md` §18 does not say they are equal.

Requested:

1. The statement, in §18: for a name written as an id-expression or as the
   member name of a class member access, the entry's `range.lo` equals the
   location Kiln stores as the name location of the expression built from
   it; and how to obtain that location for each of the two expression
   kinds.
2. A test over Kiln's conformance inputs: every `DeclRefExpr` and
   `MemberExpr` of a written (not instantiated) body that has a name has
   exactly one entry at that location, of role `expr` or `member`, with
   the same `entity`.

If Kiln would rather store the entry's id on the expression, that is
better and Slip will use it; the request is only for a join that is
promised.

## 3. Where a leading `::` is

For `::a::b` the entry of `a` has `qualifier == GLOBAL`
(`kiln/crates/kiln_ast/src/written_refs.rs:103-135`) and nothing records
where the `::` token is. Slip needs the full written range of a qualified
name to replace it (`spec/api.md` §6.6 rule 2).

Requested, in whichever form is cheapest for Kiln:

- a function from an entry with a global qualifier to the location of its
  `::` token; or
- the statement that the `::` is the token immediately before `range.lo`
  in the token stream, with a public way to step one token back from a
  location after the unit is built.

Slip will otherwise scan the file text backwards over whitespace and
comments and refuse the cases it cannot decide.

## 4. Which stage Slip needs next

For the first programmable demonstration (`docs/demos/demo-3.md`)
stages 1 and 2 are enough. For a rename that verification accepts on all
of LLVM, the order of value is:

1. Stage 3 (WP-1932), in this order within it: using-declarations;
   declarator-id qualifiers; explicit specialisation targets; elaborated
   and friend references; the rest.
2. Stage 4 (WP-1933): dependent names per specialisation.
3. Stage 5 (WP-1934). LLVM's build uses no precompiled header.

## 5. Acceptance

§1: a test in `kiln_driver` that calls `run_front_end` with the switch and
compares the table to the dump of the same input. §2 and §3: the tests
named there.
