#include "side.h"
// §7.2 CastKind `IntegralToFloating`.
int a; double d;
#ifdef B
double v = d;
#else
double v = a;
#endif
