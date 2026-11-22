#include "side.h"
// WP-035, digest-kinds §7 `ExprNode` `InitListExpr.b`: the array filler.
#ifdef B
int v[3] = {1, 0, 0};
#else
int v[3] = {1};
#endif
