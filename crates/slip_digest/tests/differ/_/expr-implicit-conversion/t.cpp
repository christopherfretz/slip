#include "side.h"
// digest §6 _ `expr-implicit-conversion`.
int f(long);
int a; long l;
#ifdef B
int v = f(l);
#else
int v = f(a);
#endif
