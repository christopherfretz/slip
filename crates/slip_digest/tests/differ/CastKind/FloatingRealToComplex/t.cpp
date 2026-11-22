#include "side.h"
// §7.2 CastKind `FloatingRealToComplex`.
_Complex int ci; _Complex long cl; _Complex double cd; _Complex float cf;
int a; double d;
#ifdef B
_Complex double v = cd;
#else
_Complex double v = d;
#endif
