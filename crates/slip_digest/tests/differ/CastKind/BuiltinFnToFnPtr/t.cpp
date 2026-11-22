#include "side.h"
// §7.2 CastKind `BuiltinFnToFnPtr`.
int a; int abs_(int);
#ifdef B
int v = abs_(a);
#else
int v = __builtin_abs(a);
#endif
