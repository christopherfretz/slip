#include "side.h"
// §5.7 ColdDeclData `decomp_bindings` in a block: each binding is written whole (`local::write_decl`), not as a key (WP-033 review, item 15).
struct P { int x, y; };
#ifdef B
int f(P p) { auto [a, c] = p; return a; }
#else
int f(P p) { auto [a, b] = p; return a; }
#endif
