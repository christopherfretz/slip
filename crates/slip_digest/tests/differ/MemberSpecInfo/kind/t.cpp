#include "side.h"
// §9.4 MemberSpecInfo `kind`: a member of a specialisation explicitly
// instantiated against one explicitly specialised.
template <class T> struct A { void f() {} };
#ifdef B
template <> void A<int>::f() {}
#else
template void A<int>::f();
#endif
