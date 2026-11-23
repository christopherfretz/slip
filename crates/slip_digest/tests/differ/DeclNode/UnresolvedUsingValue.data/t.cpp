#include "side.h"
// §5.3 DeclNode `UnresolvedUsingValue.data`: the qualifier.
template <class T, class U> struct D : T, U {
#ifdef B
using U::f;
#else
using T::f;
#endif
};
