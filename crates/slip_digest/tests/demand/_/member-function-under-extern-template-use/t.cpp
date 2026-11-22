#include "side.h"
// digest §2.2: calling a member function of a class under an explicit
// instantiation declaration changes no unit.
template <class T> struct X { void f() {} };
extern template struct X<int>;
#ifdef B
void use(X<int>& x) { x.f(); }
#else
void use(X<int>& x) {}
#endif
