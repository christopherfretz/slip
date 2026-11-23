#include "side.h"
// digest-kinds §11.9: a class specialisation named but not defined, then
// defined by a use: its members, bases and `defn` are `definition`'s.
template <class T> struct B0 {};
template <class T> struct X : B0<T> { int a; };
X<int>* p;
#ifdef B
void h() { X<int> x; }
#endif
