#include "side.h"
// §7 TypeTrait: `GtSynthesizesFromSpaceship`: against `__builtin_ge_synthesizes_from_spaceship`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __builtin_ge_synthesizes_from_spaceship(T, T); };
#else
template <class T> struct A { unsigned long w = __builtin_gt_synthesizes_from_spaceship(T, T); };
#endif
