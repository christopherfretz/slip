#include "side.h"
// §7 TypeTrait: `HasTrivialDefaultConstructor`: against `__has_trivial_copy`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __has_trivial_copy(T); };
#else
template <class T> struct A { unsigned long w = __has_trivial_constructor(T); };
#endif
