#include "side.h"
// §9.4 SpecKind `ExplicitInstantiationDefinition` against an explicit
// instantiation declaration.
template <class T> struct X {};
#ifdef B
template struct X<int>;
#else
extern template struct X<int>;
#endif
