#include "side.h"
// §7 TypeTrait: `IsReference`: against `__is_rvalue_reference`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __is_rvalue_reference(T); };
#else
template <class T> struct A { unsigned long w = __is_reference(T); };
#endif
