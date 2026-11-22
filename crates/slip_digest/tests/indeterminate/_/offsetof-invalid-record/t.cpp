// errors: yes
#include "side.h"
// §7 ExprNode `OffsetOfExpr.aux`: the designator passes through an invalid class
// (`invalid_record`): Kiln has reported an error, and the unit is indeterminate.
struct S { int n; Undeclared u; int m; };
unsigned long v = __builtin_offsetof(S, m);
