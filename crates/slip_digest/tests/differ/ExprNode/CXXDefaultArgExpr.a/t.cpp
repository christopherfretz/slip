#include "side.h"
// WP-035, digest-kinds §7 `ExprNode` `CXXDefaultArgExpr.a`: the parameter by its position.
int fd(int x = 1, int y = 1);
#ifdef B
int v = fd(1);
#else
int v = fd();
#endif
