#include "side.h"
// §10.1 (WP-100): `annotate` on a declaration after the definition against before it. Kiln keeps no node for it after the definition (as clang 19, which warns "attribute declaration must precede definition" and emits no annotation), so the lists differ whatever the rule; the split at the definition is pinned by unit test `attrs::tests::s10_1_lists_per_declaration`.
#ifdef B
void f();
void f() {}
[[clang::annotate("a")]] void f();
#else
[[clang::annotate("a")]] void f();
void f() {}
void f();
#endif
