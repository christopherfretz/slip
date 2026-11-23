#include "side.h"
// digest §3.4, WP-039 item 1: anonymous structs as members: two keys.
struct S { struct { int a; };
#ifdef B
struct { int b; };
#endif
};
