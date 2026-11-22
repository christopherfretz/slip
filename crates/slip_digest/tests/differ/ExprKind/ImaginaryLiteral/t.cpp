#include "side.h"
// §7 ExprKind `ImaginaryLiteral`.
_Complex int ci;
#ifdef B
_Complex int v = ci;
#else
_Complex int v = 1i;
#endif
