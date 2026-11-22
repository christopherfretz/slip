#include "side.h"
// §7 TypeTrait: `LtSynthesizesFromSpaceship`: against `__builtin_le_synthesizes_from_spaceship`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __builtin_le_synthesizes_from_spaceship(T, T); };
#else
template <class T> struct A { unsigned long w = __builtin_lt_synthesizes_from_spaceship(T, T); };
#endif
