#include "side.h"
// §7 TypeTrait: `HasTrivialMoveAssign`: against `__has_trivial_move_constructor`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __has_trivial_move_constructor(T); };
#else
template <class T> struct A { unsigned long w = __has_trivial_move_assign(T); };
#endif
