#include "side.h"
// §7 TypeTrait: `HasVirtualDestructor`: against `__is_abstract`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __is_abstract(T); };
#else
template <class T> struct A { unsigned long w = __has_virtual_destructor(T); };
#endif
