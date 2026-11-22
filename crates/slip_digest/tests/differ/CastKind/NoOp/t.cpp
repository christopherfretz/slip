#include "side.h"
// §7.2 CastKind `NoOp`.
int *p; const int *cp;
#ifdef B
const int *v = cp;
#else
const int *v = p;
#endif
