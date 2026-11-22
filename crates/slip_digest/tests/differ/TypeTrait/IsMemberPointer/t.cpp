#include "side.h"
// §7 TypeTrait: `IsMemberPointer`: against `__is_nothrow_destructible`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __is_nothrow_destructible(T); };
#else
template <class T> struct A { unsigned long w = __is_member_pointer(T); };
#endif
