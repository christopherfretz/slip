#include "side.h"
// §7 TypeTrait: `IsConvertibleTo`: against `__is_layout_compatible`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __is_layout_compatible(T, T); };
#else
template <class T> struct A { unsigned long w = __is_convertible_to(T, T); };
#endif
