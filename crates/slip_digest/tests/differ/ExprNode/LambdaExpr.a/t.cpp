#include "side.h"
// §7 ExprNode `LambdaExpr.a`: the closure the expression makes.
#ifdef B
int f(int a, int b) { return [] { return 2; }(); }
#else
int f(int a, int b) { return [] { return 1; }(); }
#endif
