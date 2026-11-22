#include "side.h"
// WP-028: `A<1>` and `A<2>` are different specialisations, so `f(A<1>)` and `f(A<2>)` are different functions.
template <auto V> struct A {};
#ifdef B
void f(A<2>);
#else
void f(A<1>);
#endif
