#include "side.h"
// WP-035, digest-kinds §7 `ExprKind` `DesignatedInitUpdateExpr`: a designator that overrides part of an earlier initialiser.
struct P { int x, y; };
struct Q2 { P p; int z; };
P mk();
P mk2();
#ifdef B
Q2 v = {.p = mk()};
#else
Q2 v = {.p = mk(), .p.y = 1};
#endif
