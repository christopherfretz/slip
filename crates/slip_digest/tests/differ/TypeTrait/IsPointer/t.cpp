#include "side.h"
// §7 TypeTrait: `IsPointer`: against `__is_polymorphic`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __is_polymorphic(T); };
#else
template <class T> struct A { unsigned long w = __is_pointer(T); };
#endif
