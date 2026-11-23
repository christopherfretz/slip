#include "side.h"
// §5.6 Linkage `External` (§1.7 row `enclosing`): a const variable declared extern has external linkage.
#ifdef B
extern const int v = 1;
#else
const int v = 1;
#endif
int u() { return v; }
