#include "side.h"
// §7.2 CastKind `LValueBitCast`.
int a; float f;
#ifdef B
float &v = f;
#else
float &v = reinterpret_cast<float&>(a);
#endif
