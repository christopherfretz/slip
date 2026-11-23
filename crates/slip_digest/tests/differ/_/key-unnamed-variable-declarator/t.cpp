#include "side.h"
// digest §3.3, WP-039 item 1: an unnamed class named by its first declarator, a variable.
#ifdef B
struct { int m; } x, w;
#else
struct { int m; } v, w;
#endif
