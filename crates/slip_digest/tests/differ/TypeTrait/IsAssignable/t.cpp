#include "side.h"
// §7 TypeTrait: `IsAssignable`: against `__is_convertible`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __is_convertible(T, T); };
#else
template <class T> struct A { unsigned long w = __is_assignable(T, T); };
#endif
