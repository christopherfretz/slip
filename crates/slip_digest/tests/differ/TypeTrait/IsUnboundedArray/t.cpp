#include "side.h"
// §7 TypeTrait: `IsUnboundedArray`: against `__is_union`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __is_union(T); };
#else
template <class T> struct A { unsigned long w = __is_unbounded_array(T); };
#endif
