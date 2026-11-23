#include "side.h"
// §7 TypeTrait: `IsNothrowConvertible`: against `__is_pointer_interconvertible_base_of`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __is_pointer_interconvertible_base_of(T, T); };
#else
template <class T> struct A { unsigned long w = __is_nothrow_convertible(T, T); };
#endif
