#include "side.h"
// §7 TypeTrait: `IsIntegral`: against `__is_literal`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __is_literal(T); };
#else
template <class T> struct A { unsigned long w = __is_integral(T); };
#endif
