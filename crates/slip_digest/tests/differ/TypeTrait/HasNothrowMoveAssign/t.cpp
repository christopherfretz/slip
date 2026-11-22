#include "side.h"
// §7 TypeTrait: `HasNothrowMoveAssign`: against `__has_trivial_assign`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __has_trivial_assign(T); };
#else
template <class T> struct A { unsigned long w = __has_nothrow_move_assign(T); };
#endif
