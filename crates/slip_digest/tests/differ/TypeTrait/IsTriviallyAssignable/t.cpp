#include "side.h"
// §7 TypeTrait: `IsTriviallyAssignable`: against `__reference_binds_to_temporary`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __reference_binds_to_temporary(T, T); };
#else
template <class T> struct A { unsigned long w = __is_trivially_assignable(T, T); };
#endif
