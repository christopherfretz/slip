#include "side.h"
// WP-035, digest-kinds §7 `ExprKind` `InitListExpr`.
#ifdef B
int v = 1;
#else
int v = {1};
#endif
