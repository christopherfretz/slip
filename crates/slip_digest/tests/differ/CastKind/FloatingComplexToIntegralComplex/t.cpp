#include "side.h"
// §7.2 CastKind `FloatingComplexToIntegralComplex`.
_Complex int ci; _Complex long cl; _Complex double cd; _Complex float cf;
int a; double d;
#ifdef B
_Complex int v = ci;
#else
_Complex int v = cd;
#endif
