#include "side.h"
// §7 ExprNode `CXXDependentScopeMemberExpr.b`: the member's name.
template<class T> T *p; template<class T> T *q;
#ifdef B
template<class T> int v = p<T>->n;
#else
template<class T> int v = p<T>->m;
#endif
