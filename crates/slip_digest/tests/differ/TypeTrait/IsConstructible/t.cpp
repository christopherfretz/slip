#include "side.h"
// §7 TypeTrait: `IsConstructible`: against `__is_nothrow_constructible`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __is_nothrow_constructible(T, int); };
#else
template <class T> struct A { unsigned long w = __is_constructible(T, int); };
#endif
