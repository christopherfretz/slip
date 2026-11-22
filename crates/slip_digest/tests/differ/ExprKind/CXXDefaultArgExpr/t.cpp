#include "side.h"
// WP-035, digest-kinds §7 `ExprKind` `CXXDefaultArgExpr`.
int fd(int x = 1);
#ifdef B
int v = fd(1);
#else
int v = fd();
#endif
