#include "side.h"
// §7 TypeTrait: `IsClass`: against `__is_complete_type`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __is_complete_type(T); };
#else
template <class T> struct A { unsigned long w = __is_class(T); };
#endif
