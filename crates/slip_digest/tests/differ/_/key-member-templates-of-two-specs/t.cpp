#include "side.h"
// digest §3.4, WP-039 item 1: member templates of two specialisations: two keys.
template <class T> struct A { template <class U> void g(U); };
template <> template <class U> void A<int>::g(U);
#ifdef B
template <> template <class U> void A<long>::g(U);
#endif
