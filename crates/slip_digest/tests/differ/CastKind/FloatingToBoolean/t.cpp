#include "side.h"
// §7.2 CastKind `FloatingToBoolean`.
int a; double d;
#ifdef B
bool v = a;
#else
bool v = d;
#endif
