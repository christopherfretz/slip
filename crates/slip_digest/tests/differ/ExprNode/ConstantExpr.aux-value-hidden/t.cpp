#include "side.h"
// §7 ExprNode `ConstantExpr.aux-value-hidden`.
consteval int g(int x) { return x; }
consteval int g2(int x) { return x; }
constexpr int h(int x) { return x; }
#ifdef B
int v = g(1);
#else
constexpr int v = g(1);
#endif
