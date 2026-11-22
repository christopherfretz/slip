#include "side.h"
// digest §3.4, WP-039 item 1: a hidden friend against a namespace-scope function of another signature: two keys.
struct S { friend void g(S, int) {} };
#ifdef B
void g(S, long);
#endif
