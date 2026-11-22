#include "side.h"
// §5.7 ColdDeclData `binding_expr`: the expression a binding of an aggregate names.
struct P { int x, y; };
P p;
#ifdef B
auto [a, b] = p;
#else
auto& [a, b] = p;
#endif
