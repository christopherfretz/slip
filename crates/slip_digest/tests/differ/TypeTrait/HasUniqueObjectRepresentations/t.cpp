#include "side.h"
// §7 TypeTrait: `HasUniqueObjectRepresentations`: against `__has_virtual_destructor`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __has_virtual_destructor(T); };
#else
template <class T> struct A { unsigned long w = __has_unique_object_representations(T); };
#endif
