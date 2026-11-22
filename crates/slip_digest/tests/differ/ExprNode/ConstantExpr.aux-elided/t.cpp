#include "side.h"
// §7 ExprNode `ConstantExpr.aux-elided`: an immediate invocation nested in
// another, in an initialiser that does not fold (kiln_sema
// consteval.rs:1351).
consteval int g(int x) { return x; }
int a;
#ifdef B
int v = g(1) + a;
#else
int v = g(g(1)) + a;
#endif
