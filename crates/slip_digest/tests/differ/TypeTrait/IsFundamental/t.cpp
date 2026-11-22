#include "side.h"
// §7 TypeTrait: `IsFundamental`: against `__is_integral`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __is_integral(T); };
#else
template <class T> struct A { unsigned long w = __is_fundamental(T); };
#endif
