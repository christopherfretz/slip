#include "side.h"
// digest §3.3, WP-039 item 1: an array declarator.
#ifdef B
typedef struct { int m; } A2[2], *T;
#else
typedef struct { int m; } A[2], *T;
#endif
