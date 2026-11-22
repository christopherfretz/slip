#include "side.h"
// §7.2 BinaryOp `Sub`.
int a = 1, b = 2;
#ifdef B
int v = a + b;
#else
int v = a - b;
#endif
