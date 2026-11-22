#include "side.h"
// §7 TypeTrait: `IsSame`: against `__is_assignable`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __is_assignable(T, T); };
#else
template <class T> struct A { unsigned long w = __is_same(T, T); };
#endif
