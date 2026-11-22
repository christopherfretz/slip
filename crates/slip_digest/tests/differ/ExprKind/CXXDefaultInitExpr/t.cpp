#include "side.h"
// WP-035, digest-kinds §7 `ExprKind` `CXXDefaultInitExpr`: the member's default initialiser is used.
struct Q { int a = 1; };
#ifdef B
Q v = {1};
#else
Q v = {};
#endif
