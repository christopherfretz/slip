#include "side.h"
// §7.2 CastKind `NullToPointer`.
int *p;
#ifdef B
int *v = p;
#else
int *v = 0;
#endif
