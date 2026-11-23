#include "side.h"
// §5.7 ColdDeclData `decomp_bindings` in a block: each binding is written whole (`local::write_decl`), not as a key (WP-033 review, item 15). Renaming a binding is equal (a local); here `a` names the second binding on side B.
struct P { int x, y; };
#ifdef B
int f(P p) { auto [b, a] = p; return a; }
#else
int f(P p) { auto [a, b] = p; return a; }
#endif
