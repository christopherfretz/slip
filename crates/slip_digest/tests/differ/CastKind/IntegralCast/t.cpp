#include "side.h"
// §7.2 CastKind `IntegralCast`.
int a; long l;
#ifdef B
long v = l;
#else
long v = a;
#endif
