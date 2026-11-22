#include "side.h"
// WP-035, digest-kinds §7 `ExprNode` `CXXParenListInitExpr.a`.
struct P { int x, y; };
#ifdef B
P v(1, 3);
#else
P v(1, 2);
#endif
