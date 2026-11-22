#include "side.h"
// WP-035, digest-kinds §7 `ExprNode` `ArrayInitLoopExpr.b`: the element initialiser.
int arr[2] = {1, 2};
long arl[2] = {1, 2};
#ifdef B
auto [p, q] = arl;
#else
auto [p, q] = arr;
#endif
