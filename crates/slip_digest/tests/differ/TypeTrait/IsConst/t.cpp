#include "side.h"
// §7 TypeTrait: `IsConst`: against `__is_destructible`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __is_destructible(T); };
#else
template <class T> struct A { unsigned long w = __is_const(T); };
#endif
