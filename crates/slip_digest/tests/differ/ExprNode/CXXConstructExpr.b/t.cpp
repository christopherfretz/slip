#include "side.h"
// WP-035, digest-kinds §7 `ExprNode` `CXXConstructExpr.b`.
struct S { S(int); S(long); };
S f();
#ifdef B
S v(2);
#else
S v(1);
#endif
