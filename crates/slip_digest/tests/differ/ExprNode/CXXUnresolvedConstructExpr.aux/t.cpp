#include "side.h"
// §7 ExprNode `CXXUnresolvedConstructExpr.aux`: `T(0)` against `T{0}`.
#ifdef B
template<class T> T v = T{0};
#else
template<class T> T v = T(0);
#endif
