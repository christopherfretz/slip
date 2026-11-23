#include "side.h"
// digest §3.3, WP-039 item 1: an empty unnamed class named by its first declarator, swapped.
#ifdef B
typedef struct {} *T, *P;
#else
typedef struct {} *P, *T;
#endif
