#include "side.h"
// §7.2 CastKind `BitCast`.
int *p; float *fp;
#ifdef B
float *v = fp;
#else
float *v = reinterpret_cast<float*>(p);
#endif
