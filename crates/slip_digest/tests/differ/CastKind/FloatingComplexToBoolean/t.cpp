#include "side.h"
// §7.2 CastKind `FloatingComplexToBoolean`.
_Complex int ci; _Complex long cl; _Complex double cd; _Complex float cf;
int a; double d;
#ifdef B
bool v = d;
#else
bool v = cd;
#endif
