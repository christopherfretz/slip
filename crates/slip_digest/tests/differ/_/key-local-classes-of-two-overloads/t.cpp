#include "side.h"
// digest §3.4, WP-039 item 1: local classes of two overloads: two keys.
void f(int) { struct L { void m() {} }; L().m(); }
#ifdef B
void f(long) { struct L { void m() {} }; L().m(); }
#endif
