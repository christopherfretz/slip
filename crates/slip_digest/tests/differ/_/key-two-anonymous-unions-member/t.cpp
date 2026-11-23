#include "side.h"
// digest §3.4, WP-039 item 1: anonymous unions as members: two keys.
struct S { union { int a; };
#ifdef B
union { int b; };
#endif
};
