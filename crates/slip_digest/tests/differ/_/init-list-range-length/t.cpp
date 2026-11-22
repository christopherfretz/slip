#include "side.h"
// WP-035, digest-kinds §7 `_` `init-list-range-length`: `InitListExpr.a` as runs: the same runs of the same values, of other lengths.
#ifdef B
int v[4] = {[0 ... 2] = 7, [3] = 8};
#else
int v[4] = {[0 ... 1] = 7, [2 ... 3] = 8};
#endif
