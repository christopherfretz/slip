# KR6 — the using-shadow a lookup found, kept where the AST keeps only its target

A request from Slip to Kiln. Status: **sent** (2026-11-24). Read at
Kiln `913a490bb` (Slip's pin); main was not compared.

## The Slip need

The migration Slip is first for moves an old root namespace into a new
one that read-only dependencies already open, while other read-only
dependencies stay in the old root for good. So both roots are
namespaces with an owned part and a dependency part, and a reference
to one of them from owned code has to be written, in Slip's digest, by
the part it actually reaches ([docs/wp/WP-141.md](../wp/WP-141.md),
rule R1 and addendum A1).

For a name that resolves, the referent decides. For a name that does
not resolve in the pattern (an overload set in a template, a
using-declaration's nominated set), what the digest has is the found
list and the written qualifier, and the part of the named namespace
that holds a found declaration depends on **how** the lookup found it:

```cpp
// owned, before                          after
namespace legacy { using std::forward; }  // namespace acme { using std::forward; }
template <class T> void f(T &&t) {
  g(legacy::forward<T>(t));               //   g(acme::forward<T>(t));
}
```

`std::forward` is not a member of `legacy` or of `acme`. It was found
through the using-shadow in the owned part, which moved with the rule,
so the edit is correct and must verify. The same qualifier finding the
same entity through a shadow that a **dependency** declares in `acme`
is a different lookup, and must not. Slip can rebuild the route
through using-directives itself ([namespace.qual] is a walk over
declarations it has). It cannot rebuild the route through a
using-declaration: several shadows for one entity may be in scope, and
guessing which one lookup took is exactly the string-level reasoning
the tool exists to replace. Without the shadow, Slip writes every view
key and reports a finding, so an owned re-export moved with its
namespace does not verify.

## What is asked

1. **Per element of an unresolved name's found list, the declaration
   lookup found** when it is not the target: the `UsingShadow`. For
   the unresolved lookup expression (qualified by a namespace, and
   unqualified), and carried through instantiation wherever the list
   is carried. `kiln_sema::lookup::Found` already has it (`found`,
   `Found::shadow(i)`); it is dropped where the node is built
   (`kiln_sema/src/expr.rs` near 2349: `unresolved_lookup(name,
   &lookup_set_order(&found.decls, …), …)`). Clang keeps the found
   declarations in `UnresolvedLookupExpr::decls()` (a
   `UsingShadowDecl` where there was one).
2. **Per `UsingShadow` of a using-declaration, the shadow that the
   using-declaration named**, when the declaration its lookup found
   was itself a shadow: `namespace acme { using std::string; }` then
   `using acme::string;` makes a shadow whose target is `std::string`;
   Slip needs the fact that it was reached through `acme`'s shadow.
   Clang coalesces here too, so this one is beyond clang.
3. **Accessors on `Ast`** for both, total for every node and shadow
   (`None` or the target itself where there was no shadow), reachable
   through `slip_kiln`.

Not asked: the route through using-directives; anything for names
that resolve (the found declaration of a resolved reference, if Slip
comes to need it, is a separate request); a change to the dump unless
Kiln wants it for its own differential tests.

## Design or implementation

| Part | Kind |
|---|---|
| 1 The found declaration per element | **Implementation**, with a small **design** choice: a parallel cold list beside the existing found list, or the list's elements becoming pairs. Slip has no preference; the list's order and the targets must not change (Slip's digest writes the set by entity). |
| 2 The named shadow per using-shadow | **Design** (small): a cold row on `UsingShadow`; whether a chain of three using-declarations keeps every link or only the first (Slip needs the first link: the shadow in the namespace that the qualifier named). |
| 3 Accessors | **Implementation.** |

## What Slip does until then

Build A of WP-141 writes "route unknown" (every view key, a counter
`digest.ref.namespace_route_unknown`) for a found declaration that is
neither a member of the named namespace nor reachable through its
using-directives. The fixtures that turn over when this lands are
marked `# needs: kiln (found shadows)` under
`crates/slip_digest/tests/rules/views/`; `crates/slip_digest/tests/ns_found.rs`
is where the accessor gets its first test.

## Also in this message (a defect, from WP-125)

`__PRETTY_FUNCTION__` of a member of a local class or closure prints
the enclosing function's parameter types from the canonical prototype,
not as declared (`kiln_ast/src/dump/names.rs:482-498`
`write_function_scope`). Text, sources and clang 19's strings:
[spec/landed/digest/WP-125.md](../../spec/landed/digest/WP-125.md),
"For Kiln", item 1.
