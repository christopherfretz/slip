#include "side.h"
// §7 TypeTrait: `IsFinal`: against `__is_floating_point`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __is_floating_point(T); };
#else
template <class T> struct A { unsigned long w = __is_final(T); };
#endif
