#include "side.h"
// §7 TypeTrait: `IsBaseOf`: against `__builtin_is_virtual_base_of`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __builtin_is_virtual_base_of(T, T); };
#else
template <class T> struct A { unsigned long w = __is_base_of(T, T); };
#endif
