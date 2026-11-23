#include "side.h"
// digest §3.4, WP-039 item 1: constructor overloads: two keys.
struct S { S(int);
#ifdef B
S(long);
#endif
};
