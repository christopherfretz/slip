#include "side.h"
// WP-035, digest-kinds §7 `ExprNode` `CXXNewExpr.aux`: NewInitStyle.
#ifdef B
int *v = new int{1};
#else
int *v = new int(1);
#endif
