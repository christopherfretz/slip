#include "side.h"
// §7 TypeTrait: `IsFunction`: against `__is_fundamental`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __is_fundamental(T); };
#else
template <class T> struct A { unsigned long w = __is_function(T); };
#endif
