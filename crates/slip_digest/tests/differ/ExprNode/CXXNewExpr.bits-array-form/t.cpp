#include "side.h"
// WP-035, digest-kinds §7 `ExprNode` `CXXNewExpr.bits-array-form`.
#ifdef B
int *v = new int;
#else
int *v = new int[1];
#endif
