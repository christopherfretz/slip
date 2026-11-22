#include "side.h"
// §5.7 ColdDeclData `decomp_bindings`: the bindings of a namespace-scope structured binding, as keys.
struct P { int x, y; };
P p;
#ifdef B
auto [a, c] = p;
#else
auto [a, b] = p;
#endif
