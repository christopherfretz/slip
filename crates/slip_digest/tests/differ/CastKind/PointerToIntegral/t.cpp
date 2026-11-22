#include "side.h"
// §7.2 CastKind `PointerToIntegral`.
int a; int *p;
#ifdef B
long v = (long)a;
#else
long v = (long)p;
#endif
