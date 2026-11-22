#include "side.h"
// WP-035, digest-kinds §7 `ExprKind` `CXXConstructExpr`.
struct S { S(int); S(long); };
S f();
#ifdef B
S v = f();
#else
S v(1);
#endif
