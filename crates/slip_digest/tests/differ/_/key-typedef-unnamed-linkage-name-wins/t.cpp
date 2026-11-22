#include "side.h"
// digest §3.3, WP-039 item 1: the typedef name for linkage names the class, not the first declarator.
#ifdef B
typedef struct { int m; } *P, S2;
#else
typedef struct { int m; } *P, S;
#endif
