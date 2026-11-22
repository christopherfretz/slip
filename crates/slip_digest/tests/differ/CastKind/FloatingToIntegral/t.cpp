#include "side.h"
// §7.2 CastKind `FloatingToIntegral`.
int a; double d;
#ifdef B
int v = a;
#else
int v = d;
#endif
