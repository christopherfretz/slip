#include "side.h"
// digest §3.4, WP-039 item 1: two unnamed bit-fields: two keys.
struct S { int a; int : 3;
#ifdef B
int : 4;
#endif
};
