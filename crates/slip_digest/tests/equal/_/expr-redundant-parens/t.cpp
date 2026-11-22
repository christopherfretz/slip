#include "side.h"
// digest §6 _ `expr-redundant-parens`.
int a = 1, b = 2, c = 3; long l; double d; float f; bool bb, bb2; char ch; int arr[2], arr2[2]; int *p; const int *cp; const char *ccp;
int fn(); int fn2(); int fn1(int);
#ifdef B
int v = (a + b) * c;
#else
int v = ((a + b) * (c));
#endif
