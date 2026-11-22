#include "side.h"
// WP-035, digest-kinds §7 `ExprKind` `ArrayInitLoopExpr`: a decomposition of an array copies it element by element.
int arr[2] = {1, 2};
#ifdef B
auto &[p, q] = arr;
#else
auto [p, q] = arr;
#endif
