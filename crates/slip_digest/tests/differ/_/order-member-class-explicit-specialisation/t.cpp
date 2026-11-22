#include "side.h"
// digest §2.1: the static members of a member class's explicit specialisation are written in source and listed (WP-033 review, item 1).
int h(); template <class T> struct A { struct N; };
#ifdef B
int g = h(); template <> struct A<int>::N { static inline int z = h(); };
#else
template <> struct A<int>::N { static inline int z = h(); }; int g = h();
#endif
