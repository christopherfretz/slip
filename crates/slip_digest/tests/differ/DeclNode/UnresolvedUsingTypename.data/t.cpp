#include "side.h"
// §5.3 DeclNode `UnresolvedUsingTypename.data`: the qualifier.
template <class T, class U> struct D : T, U {
#ifdef B
using typename U::type;
#else
using typename T::type;
#endif
};
