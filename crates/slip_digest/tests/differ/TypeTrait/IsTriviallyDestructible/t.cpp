#include "side.h"
// §7 TypeTrait: `IsTriviallyDestructible`: against `__is_trivially_equality_comparable`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __is_trivially_equality_comparable(T); };
#else
template <class T> struct A { unsigned long w = __is_trivially_destructible(T); };
#endif
