#include "side.h"
// §5.4 ConstexprKind `Consteval`.
#ifdef B
consteval int f() { return 1; }
#else
int f() { return 1; }
#endif
