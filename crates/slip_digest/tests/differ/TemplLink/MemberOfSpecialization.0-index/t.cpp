#include "side.h"
// §9.1 TemplLink `MemberOfSpecialization.0-index`: the member's `MemberSpecInfo`
// is in its unit (here its `kind`).
template <class T> struct A { void f() {} };
#ifdef B
template <> void A<int>::f() {}
#else
template void A<int>::f();
#endif
