#include "side.h"
// digest §7: two aggregates equal except in padding: built by aggregate initialisation and by a constructor that writes the members in another order.
template <auto V> struct A;
struct S { char c; int i; };
constexpr S make() { S s{}; s.i = 2; s.c = 1; return s; }
#ifdef B
A<make()>* x;
#else
A<S{1, 2}>* x;
#endif
