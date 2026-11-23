#include "side.h"
// digest §3.4, WP-039 item 1: a static member against a namespace variable: two keys.
struct S { static int x; };
#ifdef B
int x;
#endif
