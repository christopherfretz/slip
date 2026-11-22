#include "side.h"
// §7.2 CastKind `IntegralComplexToReal`.
_Complex int ci; _Complex long cl; _Complex double cd; _Complex float cf;
int a; double d;
#ifdef B
int v = (int)a;
#else
int v = (int)ci;
#endif
