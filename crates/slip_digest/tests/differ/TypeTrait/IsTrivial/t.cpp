#include "side.h"
// §7 TypeTrait: `IsTrivial`: against `__is_trivially_copyable`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __is_trivially_copyable(T); };
#else
template <class T> struct A { unsigned long w = __is_trivial(T); };
#endif
