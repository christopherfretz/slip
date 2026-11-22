#include "side.h"
// §9.4 MemberSpecInfo `instantiated_from`: an explicitly instantiated member of
// a specialisation of the primary against one of a partial specialisation.
template <class T> struct A { void f() {} };
#ifdef B
template <class T> struct A<T*> { void f() {} };
template void A<int*>::f();
#else
template void A<int*>::f();
#endif
