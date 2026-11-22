#include "side.h"
// §7.2 UnaryOp `Real`.
_Complex double cd;
#ifdef B
double v = __imag__ cd;
#else
double v = __real__ cd;
#endif
