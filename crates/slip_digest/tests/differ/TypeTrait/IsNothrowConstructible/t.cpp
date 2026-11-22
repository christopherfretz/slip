#include "side.h"
// §7 TypeTrait: `IsNothrowConstructible`: against `__is_trivially_constructible`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __is_trivially_constructible(T, int); };
#else
template <class T> struct A { unsigned long w = __is_nothrow_constructible(T, int); };
#endif
