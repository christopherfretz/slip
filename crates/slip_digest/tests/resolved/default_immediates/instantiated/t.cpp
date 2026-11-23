#include "side.h"
// digest-kinds §11.1 property 7 under §11.4: the instantiated default
// argument is a `default_argument` unit, and its immediate value a
// resolved unit of that unit.
consteval int id(int x) { return x; }
template <class T> struct A { void f(int a = id(sizeof(T))); };
A<int> x;
#ifdef B
void h() { x.f(); }
#endif
