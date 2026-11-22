#include "side.h"
// §7 TypeTrait: `IsNothrowAssignable`: against `__is_nothrow_convertible`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __is_nothrow_convertible(T, T); };
#else
template <class T> struct A { unsigned long w = __is_nothrow_assignable(T, T); };
#endif
