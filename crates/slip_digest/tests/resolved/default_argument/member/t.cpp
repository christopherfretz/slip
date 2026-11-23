#include "side.h"
// digest-kinds §11.4: a default argument of a member of an implicit
// instantiation, instantiated by side B's call.
template <class T> struct A { void f(int a = sizeof(T)); };
A<int> x;
#ifdef B
void h() { x.f(); }
#endif
