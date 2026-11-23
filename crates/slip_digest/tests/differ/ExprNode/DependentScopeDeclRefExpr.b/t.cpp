#include "side.h"
// §7 ExprNode `DependentScopeDeclRefExpr.b`: `T::x` against `T::y`.
#ifdef B
template<class T> int v = T::y;
#else
template<class T> int v = T::x;
#endif
