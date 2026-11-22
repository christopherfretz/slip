#include "side.h"
// §7.2 CastKind `IntegralToBoolean`.
int a; bool bb;
#ifdef B
bool v = bb;
#else
bool v = a;
#endif
