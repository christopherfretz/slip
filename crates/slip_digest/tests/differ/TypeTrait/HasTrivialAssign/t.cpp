#include "side.h"
// §7 TypeTrait: `HasTrivialAssign`: against `__has_trivial_constructor`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __has_trivial_constructor(T); };
#else
template <class T> struct A { unsigned long w = __has_trivial_assign(T); };
#endif
