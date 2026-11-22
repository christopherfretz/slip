#include "side.h"
// WP-035, digest-kinds §7 `ExprNode` `InitListExpr.a`.
#ifdef B
int v[2] = {1, 3};
#else
int v[2] = {1, 2};
#endif
