#include "side.h"
// digest-kinds §11.4: a default argument naming an earlier parameter in
// an unevaluated operand; one use on side A, two on side B: the unit is
// the same.
template <class T> struct A { void f(T p, int q = sizeof(p)); };
A<long> x;
void k() { x.f(1); }
#ifdef B
void h() { x.f(2); }
#endif
