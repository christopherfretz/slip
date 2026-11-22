#include "side.h"
// §7 TypeTrait: `ReferenceConstructsFromTemporary`: against `__reference_converts_from_temporary`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __reference_converts_from_temporary(T, T); };
#else
template <class T> struct A { unsigned long w = __reference_constructs_from_temporary(T, T); };
#endif
