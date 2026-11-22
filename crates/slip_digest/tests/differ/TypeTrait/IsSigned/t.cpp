#include "side.h"
// §7 TypeTrait: `IsSigned`: against `__is_standard_layout`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __is_standard_layout(T); };
#else
template <class T> struct A { unsigned long w = __is_signed(T); };
#endif
