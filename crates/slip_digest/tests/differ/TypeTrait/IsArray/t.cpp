#include "side.h"
// §7 TypeTrait: `IsArray`: against `__is_bounded_array`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __is_bounded_array(T); };
#else
template <class T> struct A { unsigned long w = __is_array(T); };
#endif
