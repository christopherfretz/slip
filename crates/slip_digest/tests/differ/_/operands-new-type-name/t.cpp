#include "side.h"
// WP-156 fix round 3 (Layer S, mutation 12): a type name alone, `X`, binds to `n::X` on side A and to `::X` on side B (the same size), in a template argument of a new-expression's allocated type. The census's `CXXNewExpr` row reads the allocated type: `W2<sizeof(X)>`'s argument is an operand (site 4). No join meets a type name and no trigger covers it, so nothing else writes it.
struct X { int b; };
template <unsigned> struct W2 {};
namespace n {
#ifndef B
struct X { int a; };
#endif
void *f() { return new W2<sizeof(X)>; }
}
