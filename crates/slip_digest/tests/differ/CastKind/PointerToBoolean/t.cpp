#include "side.h"
// §7.2 CastKind `PointerToBoolean`.
int a; int *p;
#ifdef B
bool v = a;
#else
bool v = p;
#endif
