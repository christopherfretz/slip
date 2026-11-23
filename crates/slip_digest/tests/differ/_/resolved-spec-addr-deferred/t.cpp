#include "side.h"
// digest-kinds §11.2 (WP-046 review F1): forming the pointer resolves the
// specification on both sides, so an earlier use that resolves it first
// changes no declared unit, P included.
template <class T> struct A { void f() noexcept(sizeof(T) > 1); };
#ifdef B
bool h() { return noexcept(A<int>().f()); }
#endif
using P = decltype(&A<int>::f);
