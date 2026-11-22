#include "side.h"
// §7.2 CastKind `LValueToRValue`.
int a = 1, b = 2;
#ifdef B
int v = 1;
#else
int v = a;
#endif
