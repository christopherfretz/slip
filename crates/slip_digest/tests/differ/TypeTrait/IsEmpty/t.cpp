#include "side.h"
// §7 TypeTrait: `IsEmpty`: against `__is_enum`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __is_enum(T); };
#else
template <class T> struct A { unsigned long w = __is_empty(T); };
#endif
