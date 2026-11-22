#include "side.h"
// §7.1 ObjectKind `BitField`.
struct BF { int b : 3; int o; };
BF bf;
#ifdef B
int v = bf.o;
#else
int v = bf.b;
#endif
