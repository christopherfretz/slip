#include "side.h"
// WP-035, digest-kinds §7 `ExprNode` `ArrayInitLoopExpr.a`: the common expression: the array copied.
int arr[2] = {1, 2}, arr2[2] = {1, 2};
#ifdef B
auto [p, q] = arr2;
#else
auto [p, q] = arr;
#endif
