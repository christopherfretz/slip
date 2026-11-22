#include "side.h"
// §7.2 CastKind `IntegralToPointer`.
int a; int *p;
#ifdef B
int *v = (int*)p;
#else
int *v = (int*)a;
#endif
