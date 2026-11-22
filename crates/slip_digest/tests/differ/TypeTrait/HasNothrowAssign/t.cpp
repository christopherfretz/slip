#include "side.h"
// §7 TypeTrait: `HasNothrowAssign`: against `__has_nothrow_constructor`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __has_nothrow_constructor(T); };
#else
template <class T> struct A { unsigned long w = __has_nothrow_assign(T); };
#endif
