#include "side.h"
// digest §2.2, WP-032 rule 1: a friend class template in a class template.
// The instantiation's friend names the pattern's declaration and declares
// no new one, so the written definition is unaffected by it.
template <class T> struct A { template <class U> friend struct F; };
#ifdef B
A<int> a;
#endif
template <class U> struct F {};
