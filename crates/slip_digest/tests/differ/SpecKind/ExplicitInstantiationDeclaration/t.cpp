#include "side.h"
// §9.4 SpecKind `ExplicitInstantiationDeclaration` against an explicit
// specialisation.
template <class T> struct X {};
#ifdef B
extern template struct X<int>;
#else
template <> struct X<int> {};
#endif
