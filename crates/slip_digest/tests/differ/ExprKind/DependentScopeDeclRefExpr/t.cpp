#include "side.h"
// §7 ExprKind `DependentScopeDeclRefExpr`.
template<class T> T *p;
#ifdef B
template<class T> int v = T::x;
#else
template<class T> int v = p<T>->x;
#endif
