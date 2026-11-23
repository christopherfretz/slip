#include "side.h"
// §7 ExprNode `CXXDependentScopeMemberExpr.aux-arrow`: `t.m` against `t->m`.
template<class T> T *p; template<class T> T *q;
#ifdef B
template<class T> int v = p<T>.m;
#else
template<class T> int v = p<T>->m;
#endif
