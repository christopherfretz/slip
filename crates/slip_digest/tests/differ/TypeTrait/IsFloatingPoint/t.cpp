#include "side.h"
// §7 TypeTrait: `IsFloatingPoint`: against `__is_function`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __is_function(T); };
#else
template <class T> struct A { unsigned long w = __is_floating_point(T); };
#endif
