#include "side.h"
// §7.2 CastKind `FloatingCast`.
float f; double d;
#ifdef B
float v = f;
#else
float v = d;
#endif
