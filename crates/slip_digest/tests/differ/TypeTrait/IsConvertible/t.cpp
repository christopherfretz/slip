#include "side.h"
// §7 TypeTrait: `IsConvertible`: against `__is_convertible_to`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __is_convertible_to(T, T); };
#else
template <class T> struct A { unsigned long w = __is_convertible(T, T); };
#endif
