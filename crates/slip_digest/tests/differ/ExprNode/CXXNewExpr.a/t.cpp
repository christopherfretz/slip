#include "side.h"
// WP-035, digest-kinds §7 `ExprNode` `CXXNewExpr.a`: NewExtra.
#ifdef B
void *v = new long;
#else
void *v = new int;
#endif
