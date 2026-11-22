#include "side.h"
// WP-035, digest-kinds §7 `ExprNode` `CXXInheritedCtorInitExpr.aux-kind`: a virtual base.
struct B1 { B1(int); B1(long); };
#ifdef B
struct D : virtual B1 { using B1::B1; };
D d(1);
#else
struct D : B1 { using B1::B1; };
D d(1);
#endif
