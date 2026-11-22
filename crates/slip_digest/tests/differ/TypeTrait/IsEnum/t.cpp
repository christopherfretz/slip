#include "side.h"
// §7 TypeTrait: `IsEnum`: against `__is_final`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __is_final(T); };
#else
template <class T> struct A { unsigned long w = __is_enum(T); };
#endif
