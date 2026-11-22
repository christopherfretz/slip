#include "side.h"
// §7.2 UnaryOp `AddrOf`.
int a; int *p; bool bb;
#ifdef B
int *v = p;
#else
int *v = &a;
#endif
