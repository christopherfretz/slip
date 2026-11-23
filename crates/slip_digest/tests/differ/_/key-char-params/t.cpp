#include "side.h"
// digest §3.4, WP-039 item 1: the three char types: two keys.
void f(char); void f(signed char);
#ifdef B
void f(unsigned char);
#endif
