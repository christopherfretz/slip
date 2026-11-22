#include "side.h"
// §7.2 UnaryOp `PostDec`.
int a; int *p; bool bb;
#ifdef B
int v = a++;
#else
int v = a--;
#endif
