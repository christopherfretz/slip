#include "side.h"
// WP-035, digest-kinds §7 `ExprKind` `CXXInheritedCtorInitExpr`: the base construction of an inheriting constructor's implicit definition.
struct B1 { B1(int); B1(long); };
#ifdef B
struct D : B1 { D(int i) : B1(i) {} };
D d(1);
#else
struct D : B1 { using B1::B1; };
D d(1);
#endif
