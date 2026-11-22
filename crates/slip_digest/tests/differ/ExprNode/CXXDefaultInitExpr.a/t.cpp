#include "side.h"
// WP-035, digest-kinds §7 `ExprNode` `CXXDefaultInitExpr.a`.
struct Q { int a = 1; int b = 1; };
#ifdef B
Q v = {.a = 0, .b = 0};
#else
Q v = {.a = 0};
#endif
