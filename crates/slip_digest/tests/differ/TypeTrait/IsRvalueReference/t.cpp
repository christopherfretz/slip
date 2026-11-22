#include "side.h"
// §7 TypeTrait: `IsRvalueReference`: against `__is_scalar`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __is_scalar(T); };
#else
template <class T> struct A { unsigned long w = __is_rvalue_reference(T); };
#endif
