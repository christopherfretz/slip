#include "side.h"
// WP-035, digest-kinds §7 `ExprKind` `ArrayInitIndexExpr`: the index of an array copy's loop.
int arr[2] = {1, 2};
#ifdef B
auto &[p, q] = arr;
#else
auto [p, q] = arr;
#endif
