#include "side.h"
// §5.3 DeclKind `UnresolvedUsingTypename`: added.
#ifdef B
template <class T> struct D : T { using typename T::type; };
#else
template <class T> struct D : T { };
#endif
