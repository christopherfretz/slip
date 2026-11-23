#include "side.h"
// digest-kinds §11.2 (WP-046 review F1): the type of a pointer to an
// instantiated member whose specification Kiln defers differs with the
// specification it resolves to; the pattern A::f() differs too (its
// dependent operand, written since WP-036).
#ifdef B
template <class T> struct A { void f() noexcept(sizeof(T) > 8); };
#else
template <class T> struct A { void f() noexcept(sizeof(T) > 1); };
#endif
using P = decltype(&A<int>::f);
