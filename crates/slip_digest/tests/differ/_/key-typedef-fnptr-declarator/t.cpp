#include "side.h"
// digest §3.3, WP-039 item 1: a function pointer declarator.
#ifdef B
struct { int m; } (*fp2)(), *q;
#else
struct { int m; } (*fp)(), *q;
#endif
