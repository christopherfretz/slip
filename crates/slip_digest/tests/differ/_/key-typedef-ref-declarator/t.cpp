#include "side.h"
// digest §3.3, WP-039 item 1: a reference declarator.
#ifdef B
typedef struct { int m; } &R2, *T;
#else
typedef struct { int m; } &R, *T;
#endif
