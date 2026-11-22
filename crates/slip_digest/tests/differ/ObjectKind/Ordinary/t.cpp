#include "side.h"
// §7.1 ObjectKind `Ordinary`.
struct BF { int b : 3; int o; };
BF bf;
#ifdef B
int v = bf.b;
#else
int v = bf.o;
#endif
