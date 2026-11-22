#include "side.h"
// §7 TypeTrait: `IsStandardLayout`: against `__is_trivial`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __is_trivial(T); };
#else
template <class T> struct A { unsigned long w = __is_standard_layout(T); };
#endif
