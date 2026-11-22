#include "side.h"
// §7 TypeTrait: `IsTriviallyRelocatable`: against `__is_unbounded_array`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __is_unbounded_array(T); };
#else
template <class T> struct A { unsigned long w = __is_trivially_relocatable(T); };
#endif
