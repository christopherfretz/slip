#include "side.h"
// §7 TypeTrait: `IsUnion`: against `__is_unsigned`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __is_unsigned(T); };
#else
template <class T> struct A { unsigned long w = __is_union(T); };
#endif
