#include "side.h"
// §7 TypeTrait: `IsTriviallyEqualityComparable`: against `__is_trivially_relocatable`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __is_trivially_relocatable(T); };
#else
template <class T> struct A { unsigned long w = __is_trivially_equality_comparable(T); };
#endif
