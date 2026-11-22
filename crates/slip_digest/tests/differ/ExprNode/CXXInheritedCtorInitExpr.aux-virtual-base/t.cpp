#include "side.h"
// WP-035, digest-kinds §7 `ExprNode` `CXXInheritedCtorInitExpr.aux-virtual-base`: the constructor is inherited from a virtual base.
struct B1 { B1(int); B1(long); };
#ifdef B
struct M : virtual B1 { using B1::B1; };
struct D : M { using M::M; };
D d(1);
#else
struct M : B1 { using B1::B1; };
struct D : M { using M::M; };
D d(1);
#endif
