#include "side.h"
// §7 ExprNode `bits-object-kind`.
struct BF { int b : 3; int o; };
BF bf;
#ifdef B
int v = bf.o;
#else
int v = bf.b;
#endif
