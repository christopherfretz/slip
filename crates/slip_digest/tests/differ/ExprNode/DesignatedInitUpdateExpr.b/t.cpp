#include "side.h"
// WP-035, digest-kinds §7 `ExprNode` `DesignatedInitUpdateExpr.b`: the update.
struct P { int x, y; };
struct Q2 { P p; int z; };
P mk();
P mk2();
#ifdef B
Q2 v = {.p = mk(), .p.y = 2};
#else
Q2 v = {.p = mk(), .p.y = 1};
#endif
