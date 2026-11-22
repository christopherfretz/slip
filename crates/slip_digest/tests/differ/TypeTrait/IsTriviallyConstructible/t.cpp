#include "side.h"
// §7 TypeTrait: `IsTriviallyConstructible`: against `__is_invocable`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __is_invocable(T, int); };
#else
template <class T> struct A { unsigned long w = __is_trivially_constructible(T, int); };
#endif
