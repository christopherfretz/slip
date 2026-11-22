#include "side.h"
// §7 ExprNode `CXXReinterpretCastExpr.aux`.
int a = 1, b = 2, c = 3; long l; double d; float f; bool bb, bb2; char ch; int arr[2], arr2[2]; int *p; const int *cp; const char *ccp;
int fn(); int fn2(); int fn1(int);
#ifdef B
int *v = reinterpret_cast<int*>(l);
#else
long v = reinterpret_cast<long>(p);
#endif
