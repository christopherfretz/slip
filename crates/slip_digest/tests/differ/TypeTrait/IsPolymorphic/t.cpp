#include "side.h"
// §7 TypeTrait: `IsPolymorphic`: against `__is_reference`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __is_reference(T); };
#else
template <class T> struct A { unsigned long w = __is_polymorphic(T); };
#endif
