#include "side.h"
// digest §3.4, WP-039 item 1: an inline namespace member and a member of the enclosing namespace: two keys.
inline namespace v1 { void f(int); }
#ifdef B
void f(long);
#endif
