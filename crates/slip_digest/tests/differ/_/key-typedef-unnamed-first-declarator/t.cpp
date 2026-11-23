#include "side.h"
// digest §3.3, WP-039 item 1: the first declarator names the class.
#ifdef B
typedef struct { int m; } *Q, *T;
#else
typedef struct { int m; } *P, *T;
#endif
