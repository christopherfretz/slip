#include "side.h"
// WP-035, digest-kinds §7 `ExprNode` `CXXNewExpr.bits-global`.
#ifdef B
int *v = new int;
#else
int *v = ::new int;
#endif
