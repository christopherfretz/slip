#include "side.h"
// digest §3.4, WP-039 item 1: unnamed classes named by a pointer typedef: two keys.
typedef struct { int a; } *P;
#ifdef B
typedef struct { int b; } *Q;
#endif
