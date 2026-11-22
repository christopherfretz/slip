#include "side.h"
// §9.1 TemplLink `0-kind`: absent for a declaration with no link, present for
// a member of a specialisation explicitly specialised.
template <class T> struct A { void f(); };
#ifdef B
template <> struct A<int> { void f(); };
void A<int>::f() {}
#else
template <> void A<int>::f() {}
#endif
