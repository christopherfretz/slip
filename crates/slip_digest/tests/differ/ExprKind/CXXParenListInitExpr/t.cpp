#include "side.h"
// WP-035, digest-kinds §7 `ExprKind` `CXXParenListInitExpr`.
struct P { int x, y; };
#ifdef B
P v{1, 2};
#else
P v(1, 2);
#endif
