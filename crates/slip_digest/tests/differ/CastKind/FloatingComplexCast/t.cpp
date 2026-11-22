#include "side.h"
// §7.2 CastKind `FloatingComplexCast`.
_Complex int ci; _Complex long cl; _Complex double cd; _Complex float cf;
int a; double d;
#ifdef B
_Complex float v = cf;
#else
_Complex float v = cd;
#endif
