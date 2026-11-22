#include "side.h"
// §7 TypeTrait: `IsPointerInterconvertibleBaseOf`: against `__is_trivially_assignable`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __is_trivially_assignable(T, T); };
#else
template <class T> struct A { unsigned long w = __is_pointer_interconvertible_base_of(T, T); };
#endif
