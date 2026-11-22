#include "side.h"
// WP-035, digest-kinds §7 `ExprKind` `CXXTemporaryObjectExpr`.
struct S2 { S2(int, int); S2(long, long); };
S2 f2();
#ifdef B
S2 v = f2();
#else
S2 v = S2(1, 2);
#endif
