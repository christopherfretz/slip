#include "side.h"
// §7 ExprNode `CXXOperatorCallExpr.b`.
struct O { int operator+(int) const; int operator-(int) const; };
O o;
#ifdef B
int v = o + 2;
#else
int v = o + 1;
#endif
