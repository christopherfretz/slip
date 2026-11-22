#include "side.h"
// §7.2 CastKind `FloatingComplexToReal`.
_Complex int ci; _Complex long cl; _Complex double cd; _Complex float cf;
int a; double d;
#ifdef B
double v = (double)d;
#else
double v = (double)cd;
#endif
