#include "side.h"
// §7 TypeTrait: `GeSynthesizesFromSpaceship`: against `__is_base_of`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __is_base_of(T, T); };
#else
template <class T> struct A { unsigned long w = __builtin_ge_synthesizes_from_spaceship(T, T); };
#endif
