#include "side.h"
// §5.3 DeclKind `UnresolvedUsingValue`: added.
#ifdef B
template <class T> struct D : T { using T::f; };
#else
template <class T> struct D : T { };
#endif
