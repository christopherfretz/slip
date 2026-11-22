#include "side.h"
// digest §2.2, WP-032 rule 1: a friend function template that only the
// instantiation declares first; the written declaration is the unit's,
// keyed and read from it (Kiln's first declaration is the instantiation's,
// whose head names the class specialisation's levels).
template <class T> struct A { template <class U> friend void g(U); };
#ifdef B
A<int> a;
#endif
template <class U> void g(U);
