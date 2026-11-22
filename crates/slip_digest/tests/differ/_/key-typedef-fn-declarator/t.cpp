#include "side.h"
// digest §3.3, WP-039 item 1: a function declarator.
#ifdef B
typedef struct { int m; } F2(), *T;
#else
typedef struct { int m; } F(), *T;
#endif
