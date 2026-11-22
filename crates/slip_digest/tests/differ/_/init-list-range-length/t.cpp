#include "side.h"
// WP-035, digest-kinds §7 `_` `init-list-range-length`: `InitListExpr.a` as runs: a range's length is its run's.
#ifdef B
int v[4] = {[0 ... 3] = 7};
#else
int v[4] = {[0 ... 2] = 7};
#endif
