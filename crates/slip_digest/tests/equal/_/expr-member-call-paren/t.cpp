#include "side.h"
// digest §6 _ `expr-member-call-paren`.
struct S { int m; int m2; static const int k = 1; static int ks; int f(); int g(); int h(int); };
S s, s2; S *ps;
#ifdef B
int v = (s).f();
#else
int v = s.f();
#endif
