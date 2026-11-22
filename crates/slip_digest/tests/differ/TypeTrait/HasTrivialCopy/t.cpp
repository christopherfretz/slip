#include "side.h"
// §7 TypeTrait: `HasTrivialCopy`: against `__has_trivial_destructor`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __has_trivial_destructor(T); };
#else
template <class T> struct A { unsigned long w = __has_trivial_copy(T); };
#endif
