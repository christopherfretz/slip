#include "side.h"
// digest §3.4, WP-039 item 1: parameters of pointer to array of two bounds: two keys.
void f(int (*)[3]);
#ifdef B
void f(int (*)[4]);
#endif
