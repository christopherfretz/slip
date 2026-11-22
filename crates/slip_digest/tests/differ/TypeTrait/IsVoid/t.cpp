#include "side.h"
// §7 TypeTrait: `IsVoid`: against `__is_volatile`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __is_volatile(T); };
#else
template <class T> struct A { unsigned long w = __is_void(T); };
#endif
