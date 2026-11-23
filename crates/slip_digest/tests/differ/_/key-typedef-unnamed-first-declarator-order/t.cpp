#include "side.h"
// digest §3.3, WP-039 item 1: an unnamed class named by its first declarator: the order of the declarators decides.
#ifdef B
typedef struct { int m; } *T, *P;
#else
typedef struct { int m; } *P, *T;
#endif
