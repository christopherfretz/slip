#include "side.h"
// §9.1 TemplLinkKind `PartialSpecialization`: a partial specialisation against
// an explicit one.
template <class T> struct S {};
#ifdef B
template <class T> struct S<T*> {};
#else
template <> struct S<int*> {};
#endif
