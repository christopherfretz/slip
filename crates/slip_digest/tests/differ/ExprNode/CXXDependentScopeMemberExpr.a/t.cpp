#include "side.h"
// §7 ExprNode `CXXDependentScopeMemberExpr.a`: the object.
template<class T> T *p; template<class T> T *q;
#ifdef B
template<class T> int v = q<T>->m;
#else
template<class T> int v = p<T>->m;
#endif
