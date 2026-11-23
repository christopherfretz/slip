#include "side.h"
// §7 ExprNode `DependentScopeDeclRefExpr.a`: the qualifier.
#ifdef B
template<class T, class U> int v = U::x;
#else
template<class T, class U> int v = T::x;
#endif
