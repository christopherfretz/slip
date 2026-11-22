#include "side.h"
// §9.4 SpecKind `ExplicitSpecialization`: a member of a specialisation
// explicitly specialised against one explicitly instantiated (its
// `MemberSpecInfo` `instantiated_from` is the same).
template <class T> struct A { void f() {} };
#ifdef B
template <> void A<int>::f() {}
#else
template void A<int>::f();
#endif
