#include "side.h"
// §5.6 Linkage `Internal` (§1.7 row `enclosing`): a function declared static has internal linkage.
#ifdef B
static int f() { return 1; }
#else
int f() { return 1; }
#endif
int u() { return f(); }
