#include "side.h"
// §5.4 ConstexprKind `Constexpr`.
#ifdef B
constexpr int f() { return 1; }
#else
int f() { return 1; }
#endif
