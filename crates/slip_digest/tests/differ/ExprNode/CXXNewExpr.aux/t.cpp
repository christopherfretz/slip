#include "side.h"
// WP-035, digest-kinds §7 `ExprNode` `CXXNewExpr.aux`: NewInitStyle alone: `()` and none call the same user-provided default constructor.
struct S { S(); };
#ifdef B
S *v = new S;
#else
S *v = new S();
#endif
