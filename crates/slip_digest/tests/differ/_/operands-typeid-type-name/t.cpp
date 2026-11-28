#include "side.h"
// WP-156 fix round 3 (Layer S, mutation 12): a type name alone, `X`, binds to `n::X` on side A and to `::X` on side B (the same size), in a template argument of a `typeid`'s type operand. The census's `CXXTypeidExpr` row reads the operand: `W2<sizeof(X)>`'s argument is an operand (site 4). No join meets a type name and no trigger covers it, so nothing else writes it.
namespace std { class type_info; }
struct X { int b; };
template <unsigned> struct W2 {};
namespace n {
#ifndef B
struct X { int a; };
#endif
const void *f() { return &typeid(W2<sizeof(X)>); }
}
