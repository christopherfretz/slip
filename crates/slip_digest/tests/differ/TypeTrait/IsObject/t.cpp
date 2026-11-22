#include "side.h"
// §7 TypeTrait: `IsObject`: against `__is_pod`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __is_pod(T); };
#else
template <class T> struct A { unsigned long w = __is_object(T); };
#endif
