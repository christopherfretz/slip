#include "side.h"
// WP-035, digest-kinds §7 `ExprKind` `CXXThrowExpr`.
#ifdef B
int v = (1, 0);
#else
int v = (throw 1, 0);
#endif
