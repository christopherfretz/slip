#include "side.h"
// §7 TypeTrait: `ArrayExtent`: the fact differs.
#ifdef B
template <class T> struct A { unsigned long w = __array_rank(T); };
#else
template <class T> struct A { unsigned long w = __array_extent(T, 0); };
#endif
