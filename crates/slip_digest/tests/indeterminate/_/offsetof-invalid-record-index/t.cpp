// errors: yes
#include "side.h"
// §7 ExprNode `OffsetOfExpr.aux`: as `_/offsetof-invalid-record`, with a
// non-constant index (the `offsetof_layout` steps are written too).
struct S { int n; Undeclared u; int z[4]; }; int a;
unsigned long v = __builtin_offsetof(S, z[a]);
