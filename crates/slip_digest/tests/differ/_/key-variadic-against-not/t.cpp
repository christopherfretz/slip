#include "side.h"
// digest §3.4, WP-039 item 1: a variadic function: two keys.
void f(int);
#ifdef B
void f(int, ...);
#endif
