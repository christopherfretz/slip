#include "side.h"
// §7 _ `dependent-member-template-keyword`: `CXXDependentScopeMemberExpr.aux-template-keyword` is `out`: `t->template m<N>(0)` against `t->m < N > (0)`, a comparison, differ through the other rows (expr_targs, the call).
template<class T> T *p; template<class T> T *q;
#ifdef B
template<class T, int N> int v = p<T>->m < N > (0);
#else
template<class T, int N> int v = p<T>->template m<N>(0);
#endif
