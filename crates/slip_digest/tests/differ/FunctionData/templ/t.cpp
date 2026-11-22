#include "side.h"
// §5.4 FunctionData `templ`, §9.1: a member of a specialisation explicitly specialised has a link; the same member of an explicit specialisation of the class has none.
template <class T> struct A { void f(); };
#ifdef B
template <> struct A<int> { void f(); };
void A<int>::f() {}
#else
template <> void A<int>::f() {}
#endif
