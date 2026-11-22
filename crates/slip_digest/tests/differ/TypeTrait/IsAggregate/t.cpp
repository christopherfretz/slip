#include "side.h"
// §7 TypeTrait: `IsAggregate`: against `__is_arithmetic`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __is_arithmetic(T); };
#else
template <class T> struct A { unsigned long w = __is_aggregate(T); };
#endif
