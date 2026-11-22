#include "side.h"
// digest §2.2, WP-032 rule 1: a function template the instantiation
// declares first, then written with an attribute or without.
template <class T> struct A { template <class U> friend void g(U); };
A<int> a;
#ifdef B
template <class U> [[deprecated]] void g(U);
#else
template <class U> void g(U);
#endif
