#include "side.h"
// §7 ExprNode `InitListExpr.bits-designated`: `.a.y = 3` (a list without braces that the designator opens) against `.a = {.y = 3}`.
struct A { int x; int y; };
struct S { A a; int z; };
#ifdef B
S s = {.a = {.y = 3}, .z = 4};
#else
S s = {.a.y = 3, .z = 4};
#endif
