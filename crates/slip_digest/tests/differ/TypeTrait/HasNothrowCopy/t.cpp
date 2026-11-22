#include "side.h"
// §7 TypeTrait: `HasNothrowCopy`: against `__has_nothrow_move_assign`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __has_nothrow_move_assign(T); };
#else
template <class T> struct A { unsigned long w = __has_nothrow_copy(T); };
#endif
