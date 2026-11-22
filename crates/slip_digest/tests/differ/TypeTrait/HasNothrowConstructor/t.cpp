#include "side.h"
// §7 TypeTrait: `HasNothrowConstructor`: against `__has_nothrow_copy`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __has_nothrow_copy(T); };
#else
template <class T> struct A { unsigned long w = __has_nothrow_constructor(T); };
#endif
