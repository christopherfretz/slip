#include "side.h"
// §7.4 ColdExprData `designators`.
// written by `expr_init::designators`
struct P2 { int x, y; };
#ifdef B
P2 v = {.y = 1};
#else
P2 v = {.x = 1};
#endif
