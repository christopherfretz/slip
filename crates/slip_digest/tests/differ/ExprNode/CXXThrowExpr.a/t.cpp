#include "side.h"
// WP-035, digest-kinds §7 `ExprNode` `CXXThrowExpr.a`: a rethrow has no operand.
#ifdef B
int v = (throw, 0);
#else
int v = (throw 1, 0);
#endif
