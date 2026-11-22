#include "side.h"
// WP-035, digest-kinds §7 `ExprKind` `ImplicitValueInitExpr`: the missing member is value-initialised.
struct P { int x, y; };
#ifdef B
P v = {1, 0};
#else
P v = {1};
#endif
