#include "side.h"
// §7 TypeTrait: `IsCompleteType`: against `__is_compound`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __is_compound(T); };
#else
template <class T> struct A { unsigned long w = __is_complete_type(T); };
#endif
