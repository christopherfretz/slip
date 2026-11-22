#include "side.h"
// §7 ExprNode `ConstantExpr.b`.
consteval int g(int x) { return x; }
consteval int g2(int x) { return x; }
constexpr int h(int x) { return x; }
#ifdef B
int v = g(2);
#else
int v = g(1);
#endif
