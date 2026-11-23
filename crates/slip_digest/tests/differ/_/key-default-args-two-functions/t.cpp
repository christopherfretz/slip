#include "side.h"
// digest §3.4, WP-039 item 1: default arguments of two functions: two keys.
void f(int = 1);
#ifdef B
void f(long = 1);
#endif
