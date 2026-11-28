#include "side.h"
// WP-156 fix round 3 (Layer S, mutation 3): a type name alone, `X`, binds to `n::X` on side A and to `::X` on side B (the same size), inside a template argument of a type written in an instantiation-dependent array bound. The bound is walked for its holders: `W2<sizeof(X)>`'s argument is an operand (site 4), and no trigger and no join would write `X` otherwise.
struct X { int b; };
namespace n {
#ifndef B
struct X { int a; };
#endif
template <unsigned> struct W2 {};
template <class T> void f(int (*)[sizeof(T) + sizeof(W2<sizeof(X)>)]) {}
}
