#include "side.h"
// §7 TypeTrait: `IsNothrowDestructible`: against `__is_nullptr`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __is_nullptr(T); };
#else
template <class T> struct A { unsigned long w = __is_nothrow_destructible(T); };
#endif
