#include "side.h"
// §9.4 SpecKind `ExplicitSpecialization` against an explicit instantiation
// definition of the same (empty) class.
template <class T> struct X {};
#ifdef B
template <> struct X<int> {};
#else
template struct X<int>;
#endif
