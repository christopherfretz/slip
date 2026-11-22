#include "side.h"
// WP-035, digest-kinds §7 `ExprNode` `CXXDeleteExpr.b`: the class's own deallocation function against the global one.
struct C { int i; static void operator delete(void *); };
struct D { int i; };
C *pc;
#ifdef B
int v = (::delete pc, 0);
#else
int v = (delete pc, 0);
#endif
