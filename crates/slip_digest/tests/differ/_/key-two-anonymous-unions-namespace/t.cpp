#include "side.h"
// digest §3.4, WP-039 item 1: anonymous unions at namespace scope: two keys.
static union { int a; };
#ifdef B
static union { int b; };
#endif
