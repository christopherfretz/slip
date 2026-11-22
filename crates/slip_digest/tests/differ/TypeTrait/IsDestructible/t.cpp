#include "side.h"
// §7 TypeTrait: `IsDestructible`: against `__is_empty`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __is_empty(T); };
#else
template <class T> struct A { unsigned long w = __is_destructible(T); };
#endif
