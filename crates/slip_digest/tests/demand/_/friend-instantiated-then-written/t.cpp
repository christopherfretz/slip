#include "side.h"
// digest §2.2 (WP-032 rule 1): on side B the instantiation of `A<int>`
// declares `k` first and the namespace-scope declaration comes after it.
// `k` has a written declaration, so it is a declared unit read from that
// declaration, as on side A.
template <class T> struct A { friend void k(int); };
#ifdef B
A<int> a;
#else

#endif
void k(int);
