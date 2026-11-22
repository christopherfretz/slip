#include "side.h"
// §7.2 CastKind `ToVoid`.
int a = 1, b = 2;
#ifdef B
int v = (a, 1);
#else
int v = ((void)a, 1);
#endif
