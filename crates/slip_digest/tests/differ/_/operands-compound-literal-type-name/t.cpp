#include "side.h"
// WP-156 fix round 3 (Layer S, mutation 12): a type name alone, `X`, binds to `n::X` on side A and to `::X` on side B (the same size), in a template argument of an array compound literal's written type. The census's `CompoundLiteralExpr` row reads it: `W2<sizeof(X)>`'s argument is an operand (site 4). No join meets a type name and no trigger covers it, so nothing else writes it.
struct X { int b; };
template <unsigned> struct W2 {};
void g(const void *);
namespace n {
#ifndef B
struct X { int a; };
#endif
void f() { g((const W2<sizeof(X)>[1]){}); }
}
