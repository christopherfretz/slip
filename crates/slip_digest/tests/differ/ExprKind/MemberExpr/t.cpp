#include "side.h"
// §7 ExprKind `MemberExpr`.
int a = 1, b = 2, c = 3; long l; double d; float f; bool bb, bb2; char ch; int arr[2], arr2[2]; int *p; const int *cp; const char *ccp;
int fn(); int fn2(); int fn1(int);
struct S { int m; int m2; static const int k = 1; static int ks; int f(); int g(); int h(int); };
S s, s2; S *ps;
#ifdef B
int v = a;
#else
int v = s.m;
#endif
