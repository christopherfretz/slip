#include "side.h"
// §9.4 SpecKind `ImplicitInstantiation`: an implicit instantiation is a derived
// unit (§9), an explicit instantiation definition a declared one.
template <class T> struct X {};
#ifdef B
template struct X<int>;
X<int> v;
#else
X<int> v;
#endif
