#include "side.h"
// §7.2 UnaryOp `LNot`.
int a; int *p; bool bb;
#ifdef B
bool v = bb;
#else
bool v = !bb;
#endif
