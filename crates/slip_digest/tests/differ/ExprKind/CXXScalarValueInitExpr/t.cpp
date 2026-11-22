#include "side.h"
// WP-035, digest-kinds §7 `ExprKind` `CXXScalarValueInitExpr`.
#ifdef B
int v = 0;
#else
int v = int();
#endif
