#include "side.h"
// §7 TypeTrait: `IsCompound`: against `__is_const`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __is_const(T); };
#else
template <class T> struct A { unsigned long w = __is_compound(T); };
#endif
