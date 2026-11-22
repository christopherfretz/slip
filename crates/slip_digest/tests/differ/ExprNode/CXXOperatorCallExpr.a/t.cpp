#include "side.h"
// §7 ExprNode `CXXOperatorCallExpr.a`.
struct O { int operator+(int) const; };
int operator+(const O&, long);
O o;
#ifdef B
int v = o + 1L;
#else
int v = o + 1;
#endif
