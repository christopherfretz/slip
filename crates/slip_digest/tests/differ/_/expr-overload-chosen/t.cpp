#include "side.h"
// digest §6 _ `expr-overload-chosen`.
int f(int); int f(long);
int a; long l;
#ifdef B
int v = f(l);
#else
int v = f(a);
#endif
