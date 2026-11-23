#include "side.h"
// §7 TypeTrait: `IsNullptr`: against `__is_object`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __is_object(T); };
#else
template <class T> struct A { unsigned long w = __is_nullptr(T); };
#endif
