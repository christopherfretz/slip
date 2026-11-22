#include "side.h"
// §7 TypeTrait: `IsAbstract`: against `__is_aggregate`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __is_aggregate(T); };
#else
template <class T> struct A { unsigned long w = __is_abstract(T); };
#endif
