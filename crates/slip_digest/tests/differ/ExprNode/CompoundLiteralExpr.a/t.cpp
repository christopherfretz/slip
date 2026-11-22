#include "side.h"
// WP-035, digest-kinds §7 `ExprNode` `CompoundLiteralExpr.a`.
struct P { int x, y; };
#ifdef B
P v = (P){1, 3};
#else
P v = (P){1, 2};
#endif
