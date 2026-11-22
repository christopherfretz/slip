#include "side.h"
// §7 TypeTrait: `IsBitwiseCloneable`: against `__builtin_is_implicit_lifetime`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __builtin_is_implicit_lifetime(T); };
#else
template <class T> struct A { unsigned long w = __is_bitwise_cloneable(T); };
#endif
