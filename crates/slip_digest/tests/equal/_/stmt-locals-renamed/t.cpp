// std: c++17
#include "side.h"
// WP-034 (§6): locals, parameters, labels and bindings renamed.
struct S { int p, q; };
#ifdef B
int f(int x) { int y = x; m: if (y) goto m; auto [u, v] = S{}; return y + u + v; }
#else
int f(int a) { int b = a; l: if (b) goto l; auto [c, d] = S{}; return b + c + d; }
#endif
