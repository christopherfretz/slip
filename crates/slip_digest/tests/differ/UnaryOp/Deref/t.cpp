#include "side.h"
// §7.2 UnaryOp `Deref`.
int a; int *p; bool bb;
#ifdef B
int v = p[0];
#else
int v = *p;
#endif
