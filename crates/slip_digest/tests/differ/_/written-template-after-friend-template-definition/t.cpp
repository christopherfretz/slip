#include "side.h"
// digest §2.2, WP-032 rule 1: a function template the instantiation
// declares first, then written: a definition against a declaration.
template <class T> struct A { template <class U> friend void g(U); };
A<int> a;
#ifdef B
template <class U> void g(U) {}
#else
template <class U> void g(U);
#endif
