#include "side.h"
// §7 ExprNode `MemberExpr.a`.
struct S { int m; int m2; static const int k = 1; static int ks; int f(); int g(); int h(int); };
S s, s2; S *ps;
#ifdef B
int v = s2.m;
#else
int v = s.m;
#endif
