#include "side.h"
// WP-035, digest-kinds §7 `ExprNode` `CXXInheritedCtorInitExpr.a`: the inherited constructor.
struct B1 { B1(int); B1(long); };
#ifdef B
struct D : B1 { using B1::B1; };
D d(1L);
#else
struct D : B1 { using B1::B1; };
D d(1);
#endif
