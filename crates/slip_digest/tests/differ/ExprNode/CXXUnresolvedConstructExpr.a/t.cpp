#include "side.h"
// §7 ExprNode `CXXUnresolvedConstructExpr.a`: the arguments.
#ifdef B
template<class T> T v = T(1);
#else
template<class T> T v = T(0);
#endif
