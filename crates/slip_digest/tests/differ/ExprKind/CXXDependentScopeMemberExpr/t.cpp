#include "side.h"
// §7 ExprKind `CXXDependentScopeMemberExpr`.
template<class T> T *p; template<class T> T *q;
#ifdef B
template<class T> int v = T::m;
#else
template<class T> int v = p<T>->m;
#endif
