#include "side.h"
// §7.2 UnaryOp `PostInc`.
int a; int *p; bool bb;
#ifdef B
int v = a--;
#else
int v = a++;
#endif
