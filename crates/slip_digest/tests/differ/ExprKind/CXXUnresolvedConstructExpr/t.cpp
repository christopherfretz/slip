#include "side.h"
// §7 ExprKind `CXXUnresolvedConstructExpr`.
#ifdef B
template<class T> T v = 0;
#else
template<class T> T v = T(0);
#endif
