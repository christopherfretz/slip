#include "side.h"
// digest §3.3, WP-039 item 1: the second declarator does not name the class.
#ifdef B
typedef struct { int m; } *P, *U;
#else
typedef struct { int m; } *P, *T;
#endif
