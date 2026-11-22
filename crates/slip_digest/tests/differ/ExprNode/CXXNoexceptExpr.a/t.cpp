#include "side.h"
// §7 ExprNode: `CXXNoexceptExpr.a`: the fact differs.
int a = 1, b = 2, c = 3; long l;
#ifdef B
bool v = noexcept(b);
#else
bool v = noexcept(a);
#endif
