#include "side.h"
// digest §3.3, WP-039 item 1: in a class.
#ifdef B
struct C { typedef struct {} *T, *P; };
#else
struct C { typedef struct {} *P, *T; };
#endif
