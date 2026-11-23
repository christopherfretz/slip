#include "side.h"
// WP-035, digest-kinds §7 `ExprNode` `CXXConstructExpr.a`.
struct S { S(int); S(long); };
S f();
#ifdef B
S v(1L);
#else
S v(1);
#endif
