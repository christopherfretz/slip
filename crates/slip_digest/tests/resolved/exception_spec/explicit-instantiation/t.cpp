#include "side.h"
// digest-kinds §11.2 rule 3 under an explicit instantiation definition:
// Kiln resolves the specification of the members it defines (g) and
// defers the others (f) as in an implicit instantiation.
template <class T> struct A {
  void f() noexcept(sizeof(T) > 1);
  void g() noexcept(sizeof(T) > 2) {}
};
template struct A<int>;
#ifdef B
bool h() { return noexcept(A<int>().f()); }
#endif
