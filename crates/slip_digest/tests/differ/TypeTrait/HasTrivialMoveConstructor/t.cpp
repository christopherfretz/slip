#include "side.h"
// §7 TypeTrait: `HasTrivialMoveConstructor`: against `__has_unique_object_representations`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __has_unique_object_representations(T); };
#else
template <class T> struct A { unsigned long w = __has_trivial_move_constructor(T); };
#endif
