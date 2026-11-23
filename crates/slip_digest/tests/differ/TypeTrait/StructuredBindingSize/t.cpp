#include "side.h"
// §7 TypeTrait: `StructuredBindingSize`: against `__has_nothrow_assign`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __has_nothrow_assign(T); };
#else
template <class T> struct A { unsigned long w = __builtin_structured_binding_size(T); };
#endif
