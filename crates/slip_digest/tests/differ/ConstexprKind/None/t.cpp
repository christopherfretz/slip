#include "side.h"
// §5.4 ConstexprKind `None`.
#ifdef B
int f() { return 1; }
#else
constexpr int f() { return 1; }
#endif
