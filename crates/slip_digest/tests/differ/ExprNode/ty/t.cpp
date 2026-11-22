#include "side.h"
// §7 ExprNode `ty`.
int a = 1, b = 2;
#ifdef B
long v = (long)a;
#else
long v = (short)a;
#endif
