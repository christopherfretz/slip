#include "side.h"
// §7 TypeTrait: `IsBoundedArray`: against `__is_class`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __is_class(T); };
#else
template <class T> struct A { unsigned long w = __is_bounded_array(T); };
#endif
