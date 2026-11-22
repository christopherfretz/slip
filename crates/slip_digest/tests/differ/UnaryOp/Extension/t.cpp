#include "side.h"
// §7.2 UnaryOp `Extension`.
int a; int *p; bool bb;
#ifdef B
int v = a;
#else
int v = __extension__ a;
#endif
