#include "side.h"
// §7.2 CastKind `LValueToRValueBitCast`.
int a; float f;
#ifdef B
float v = f;
#else
float v = __builtin_bit_cast(float, a);
#endif
