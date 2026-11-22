#include "side.h"
// §7 ExprNode `CXXOperatorCallExpr.aux`.
struct O { int operator+(int) const; int operator-(int) const; };
O o;
#ifdef B
int v = o - 1;
#else
int v = o + 1;
#endif
