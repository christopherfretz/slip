#include "side.h"
// §7 TypeTrait: `IsStructural`: against `__builtin_structured_binding_size`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __builtin_structured_binding_size(T); };
#else
template <class T> struct A { unsigned long w = __builtin_is_structural(T); };
#endif
