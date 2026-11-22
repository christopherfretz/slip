#include "side.h"
// WP-035, digest-kinds §7 `ExprNode` `CXXDeleteExpr.b`: the deallocation function: the same expression finds another class's.
struct D1 { static void operator delete(void *); };
struct D2 { static void operator delete(void *); };
#ifdef B
struct C : D2 { int i; };
#else
struct C : D1 { int i; };
#endif
C *pc;
int v = (delete pc, 0);
