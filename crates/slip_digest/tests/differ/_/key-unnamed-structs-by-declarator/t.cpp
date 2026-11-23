#include "side.h"
// digest §3.4, WP-039 item 1: unnamed classes named by their declarators: two keys.
struct { int x; } a;
#ifdef B
struct { int x; } b;
#endif
