#include "side.h"
// §7 TypeTrait: `HasTrivialDestructor`: against `__has_trivial_move_assign`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __has_trivial_move_assign(T); };
#else
template <class T> struct A { unsigned long w = __has_trivial_destructor(T); };
#endif
