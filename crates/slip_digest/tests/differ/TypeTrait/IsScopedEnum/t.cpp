#include "side.h"
// §7 TypeTrait: `IsScopedEnum`: against `__is_signed`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __is_signed(T); };
#else
template <class T> struct A { unsigned long w = __is_scoped_enum(T); };
#endif
