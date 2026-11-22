#include "side.h"
// §7 TypeTrait: `IsInvocable`: against `__is_nothrow_invocable`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __is_nothrow_invocable(T, int); };
#else
template <class T> struct A { unsigned long w = __is_invocable(T, int); };
#endif
