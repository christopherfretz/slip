#include "side.h"
// digest-kinds §11.1 property 1, rule 3: the resolved specification of an
// instantiation differs with what its operand names.
template <class T> struct A { void f() noexcept(T::v); };
struct X {
#ifdef B
  static constexpr bool v = true;
#else
  static constexpr bool v = false;
#endif
};
bool h(A<X> &a) { return noexcept(a.f()); }
