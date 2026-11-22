#include "side.h"
// §7 TypeTrait: `IsPod`: against `__is_pointer`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __is_pointer(T); };
#else
template <class T> struct A { unsigned long w = __is_pod(T); };
#endif
