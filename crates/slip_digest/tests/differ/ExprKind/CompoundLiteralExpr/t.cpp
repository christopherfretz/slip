#include "side.h"
// WP-035, digest-kinds §7 `ExprKind` `CompoundLiteralExpr`.
struct P { int x, y; };
#ifdef B
P v = P{1, 2};
#else
P v = (P){1, 2};
#endif
