#include "side.h"
// §7.2 UnaryOp `Imag`.
_Complex double cd;
#ifdef B
double v = __real__ cd;
#else
double v = __imag__ cd;
#endif
