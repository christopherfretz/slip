# KR1 — the written-reference table

A request from Slip to Kiln. Status: **sent 2026-11-21; shape agreed with the
Kiln agent (§6); scheduling is with the operator.**
Context: [05-kiln-interface.md](../design/05-kiln-interface.md) §3 and §4.
Kiln owns the design of everything inside Kiln; this states what Slip needs,
proposes a shape, and gives acceptance tests. Names of types and flags below
are placeholders.

## 1. Need

Slip edits names where they are written. For every name written in a
translation unit it must know **which tokens** are the name and **which
entity** the name resolved to. Kiln's AST keeps the entity and loses the
tokens: there is no type-location information, nested-name-specifier
components have no location, and `DeclRefExpr`/`MemberExpr` keep no name
range. Without this Slip can rename nothing but simple expression names, and
cannot do the namespace migration at all ([03](../design/03-equivalence.md)
§7), because most of that work is rewriting qualifiers and type names.

## 2. Proposed shape

An opt-in side table owned by the AST, filled as names are resolved, off by
default. One entry per written name:

| Field | Meaning |
|---|---|
| `range` | First and last token of the name as written, as Kiln locations (macro provenance intact). The name only: for `a::b::C<int>` there are three entries, `a`, `b`, `C`, and the template arguments have their own. For `operator==` and `operator""_x` in id-expressions, the whole operator-function-id. |
| `entity` | The declaration the name denotes after resolution. For an overloaded call `f(x)`, the function selected by overload resolution (not the overload set). For a name that named a using-shadow, the underlying declaration. Absent for a dependent name in a template pattern (see `specialization`). |
| `found` | The declaration lookup found, when it differs from `entity` (today's `found_decl`: a using-shadow). |
| `role` | The syntactic role; list below. |
| `enclosing` | The innermost declaration that lexically contains the name. |
| `qualifier` | For a name preceded by `::`: the entry of the component immediately to its left, or a marker for the global scope. Absent for an unqualified name. This is how a qualifier chain is walked and how the full written range `a::b::C` is recovered. |
| `specialization` | Absent for entries made while parsing. For a dependent name, one further entry per instantiated specialization: it refers to the pattern entry (same `range`) and carries the specialization and the entity found for it. Non-dependent names are not repeated per specialization. |

Roles (the set Slip needs; Kiln may refine):

| Role | Example (the entry is for the bracketed name) |
|---|---|
| Expression name | `x = [f](y);` `&[g]` `[Enumerator]` `case [E]:` lambda capture `[[x]]{}` |
| Member name | `a.[m]` `p->[m]()` `p->~[T]()` |
| Type name | `[T] x;` `new [T]` `static_cast<[T]>(e)` `sizeof([T])` `catch ([E]&)` `typename A::[type]` return, parameter, alias, template-argument and conversion-function types |
| Template name | `[vector]<int>` CTAD `[pair] p{1,2};` template template argument `X<[vector]>` `a.template [f]<int>()` |
| Nested-name-specifier component | `[a]::[b]::C` `[T]::x` `[Enum]::e` `[alias]::f()` including in **declarator-ids**: `void [A]::[B]::f() {}` `int [N]::x = 0;` `struct [N]::S {};` |
| Base specifier | `struct D : [B]` |
| Member-initializer id | `D() : [B](1), [m](2) {}` |
| Using-declaration name | `using a::[f];` `using enum [E];` |
| Using-directive namespace | `using namespace [n];` |
| Namespace alias target | `namespace x = a::[b];` |
| Elaborated or friend reference | `class [X]* p;` `friend class [Y];` `friend void [f](int);` when they name an existing entity |
| Specialization target | `template <> struct [hash]<T> {};` `template class [vector]<int>;` |
| Concept name | `template <[C] T>` `requires [C]<T>` `[C] auto x` |
| Designator | `{ .[field] = 1 }` |

Not in the table: declaration names at their own declaration (KR3 covers
those), keywords, labels, attribute names, and implicit references that have
no written name (implicit conversions, constructor calls, overloaded operator
*uses* such as `a == b`; the AST already records what those selected).

## 3. Behaviour

1. **Opt-in.** Off by default; with recording off the cost is one predictable
   branch per resolution site and Kiln's performance gates do not move.
2. **Complete over what was parsed.** Every written name in a region the
   parser reached has an entry (see the accounting test, §4.2). Names inside
   skipped preprocessor groups have none.
3. **Stable under speculation.** A name resolved in a tentative parse that is
   abandoned leaves no entry; a name parsed once has one entry, however many
   times lookup ran.
4. **Macros.** `range` keeps the locations Kiln gives the tokens, so a name
   that came from a macro body or argument is distinguishable through the
   source manager as today.
5. **Templates.** In a pattern, non-dependent names have their entity;
   dependent names have none. Each implicit or explicit instantiation adds
   per-specialization entries for the dependent names it resolved. Order of
   instantiation must not affect the table's content (it may affect entry
   order; Slip sorts).
6. **PCH and modules.** Entries for names in a precompiled header or module
   are available in the consuming TU (carried in the snapshot/image).
7. **Access.** Through the facade: iterate all entries; iterate entries whose
   `enclosing` is a given declaration; for an entry, its per-specialization
   entries.

## 4. Acceptance tests

### 4.1 A dump

A textual dump (for example `-fkiln-dump-written-refs`, or an `expect:` kind
in Kiln's harness) with one line per entry, so tests are single files:

```
<line>:<col>-<line>:<col>  <role>  '<spelling>' -> <qualified entity> [found=<decl>] [q=<entry>|::] [in=<enclosing>] [spec=<specialization>]
```

Example input:

```cpp
namespace legacy { namespace io {
  struct Buffer { int size; };
  void flush(Buffer&);
  template <class T> struct Box { T v; };
} }
namespace app {
  using legacy::io::Buffer;                       // (1)
  struct W : legacy::io::Box<Buffer> {            // (2)
    void run(Buffer& b) { flush(b); b.size = 0; } // (3)
  };
  template <class T> void each(T& t) { flush(t); }  // (4)
  void go(legacy::io::Buffer& b) { each(b); }       // (5)
}
void legacy::io::flush(Buffer&) {}                // (6)
```

Expected entries (abbreviated):

```
(1) nns        'legacy' -> legacy                 q=-
    nns        'io'     -> legacy::io             q='legacy'
    using-decl 'Buffer' -> legacy::io::Buffer     q='io'
(2) nns        'legacy' -> legacy
    nns        'io'     -> legacy::io             q='legacy'
    template   'Box'    -> legacy::io::Box        q='io'    (also: base specifier)
    type       'Buffer' -> legacy::io::Buffer     found=app::Buffer(using-shadow)
(3) type       'Buffer' -> legacy::io::Buffer     found=app::Buffer(using-shadow)  in=app::W::run
    expr       'flush'  -> legacy::io::flush(Buffer&)                              in=app::W::run   [found by ADL; no qualifier]
    expr       'b'      -> parameter b
    member     'size'   -> legacy::io::Buffer::size
    expr       'b'      -> parameter b
(4) expr       'flush'  -> (dependent)            in=app::each
    expr       't'      -> parameter t
    + spec=app::each<legacy::io::Buffer>: 'flush' -> legacy::io::flush(Buffer&)
(5) nns 'legacy', nns 'io', type 'Buffer' -> legacy::io::Buffer q='io'
    expr       'each'   -> app::each<legacy::io::Buffer>   (the selected specialization)
    expr       'b'      -> parameter b
(6) nns        'legacy' -> legacy                 (declarator-id qualifier)
    nns        'io'     -> legacy::io             q='legacy'
    type       'Buffer' -> legacy::io::Buffer     (found in the scope of the declarator-id)
```

A test per role in the table of §2, plus: macro body and macro argument
names; an abandoned tentative parse; a dependent name instantiated with two
different results in one TU; a name from a PCH.

### 4.2 Identifier accounting (the completeness test)

For a file, lex its tokens and take every identifier token in a region the
parser reached. Each must be exactly one of:

1. the `range` of a table entry;
2. a declaration's own name (KR3);
3. a member of a short, enumerated list of exclusions (labels, attribute
   tokens, macro names at directives and invocations, parameter names of
   function types with no entity, contextual keywords).

Anything left over is a hole in the table. The test passes when the
remainder is empty on the conformance inputs, and the same check run over
libc++'s headers and a slice of LLVM reports zero unaccounted identifiers.
Slip can supply this checker; it is also how Slip will validate each Kiln
bump.

## 5. Suggested staging

Each stage is useful to Slip on its own, in this order:

1. Table, facade iterator, dump; expression names, member names,
   nested-name-specifier components with the `qualifier` chain.
2. Type names in every type-specifier context; template names; base
   specifiers; member-initializer ids.
3. Declarative contexts: using-declarations and directives, namespace
   aliases, declarator-id qualifiers, elaborated and friend references,
   specialization targets, concept names, designators.
4. Per-specialization entries for dependent names.
5. Carriage in PCH and module images; identifier accounting on the corpora.

## 6. Resolved with Kiln (2026-11-21)

Kiln agrees with the shape. Its design calls, and Slip's answer to the one
question it asked:

- **`enclosing` is stored**, one declaration id per entry; deriving it from
  ranges fails for names that come from macros. It is the *lexical* one: the
  innermost declaration (the specific redeclaration) whose source extent
  contains the name's first token. For a name in a declarator, including the
  qualifier of an out-of-line definition (`void [A]::[B]::f() {}`), that is
  the declaration being declared (this definition of `f`), not the namespace
  the definition sits in and not the class `B` it is a member of. Slip walks
  outwards from there through lexical parents.
- **Per-specialization entries are a separate table** keyed by (pattern entry,
  specialization). The parse-time table stays append-only and is carried in a
  PCH or module image unchanged; the consuming TU adds specialization entries
  for the instantiations it performs, end-of-TU instantiations included.
- **Default arguments and default member initialisers**: one entry at the
  written site, made when Kiln parses them. Uses refer to the parsed
  expression, so there are no per-call-site entries. Inside a template, the
  dependent names get per-specialization entries keyed by the function's
  specialization, not by the call.
- **Rule 3 is real work**: tentative parses truncate the table on rollback,
  and the entry for an overloaded `f(x)` is created at lookup and patched
  with the selected function after overload resolution.
- **Size**: about 28 bytes per entry (range 8, entity 4, found 4, enclosing 4,
  qualifier 4, role 1, padding).
- Kiln relays the request to the operator as size L in five stages; stage 1
  is one Kiln work package.

Prototypes Slip makes for Kiln (KF1 first) go on a branch named `slip/<id>`
in the Kiln repository, based on that day's main; no pull request and never a
push to main (Kiln's merge queue is stacked). Slip sends the branch head to
the Kiln agent, who reviews it and lands it through a Kiln work package.
