#include "side.h"
// §9.1 TemplLink `PartialSpecialization.0-index`: the partial specialisation's
// head (`PartialSpecInfo` `params`).
template <class T, class U = int> struct S {};
#ifdef B
template <class T, class U> struct S<T*, U> {};
#else
template <class T> struct S<T*> {};
#endif
