#include "side.h"
// §7 TypeTrait: `IsTriviallyCopyable`: against `__is_trivially_destructible`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __is_trivially_destructible(T); };
#else
template <class T> struct A { unsigned long w = __is_trivially_copyable(T); };
#endif
