#include "side.h"
// §7 TypeTrait: `IsScalar`: against `__is_scoped_enum`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __is_scoped_enum(T); };
#else
template <class T> struct A { unsigned long w = __is_scalar(T); };
#endif
