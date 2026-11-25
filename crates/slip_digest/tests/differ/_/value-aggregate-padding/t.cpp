#include "side.h"
// digest §7: two aggregates equal except in padding: built by aggregate initialisation and by a constructor that writes the members in another order.
// WP-156: the values are equal (x's type is equal), but the argument is an operand (site 4) that names something, written as an expression in x's operand unit (digest §2.4), and the two expressions differ: a derived change, loud.
template <auto V> struct A;
struct S { char c; int i; };
constexpr S make() { S s{}; s.i = 2; s.c = 1; return s; }
#ifdef B
A<make()>* x;
#else
A<S{1, 2}>* x;
#endif
