#include "side.h"
// §7 TypeTrait: `IsUnsigned`: against `__is_void`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __is_void(T); };
#else
template <class T> struct A { unsigned long w = __is_unsigned(T); };
#endif
