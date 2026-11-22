#include "side.h"
// §7 TypeTrait: `IsLayoutCompatible`: against `__is_nothrow_assignable`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __is_nothrow_assignable(T, T); };
#else
template <class T> struct A { unsigned long w = __is_layout_compatible(T, T); };
#endif
