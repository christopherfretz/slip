#include "side.h"
// WP-035, digest-kinds §7 `ExprNode` `CXXDeleteExpr.aux-sized`: the deallocation function takes the size.
struct C { int i; static void operator delete(void *, decltype(sizeof 0)); };
struct D { int i; static void operator delete(void *); };
C *pc;
D *pd;
#ifdef B
int v = (delete pd, 0);
#else
int v = (delete pc, 0);
#endif
