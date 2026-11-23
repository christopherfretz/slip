#include "side.h"
// digest §3.4, WP-039 item 1: _BitInt parameters of two widths: two keys.
void f(_BitInt(8));
#ifdef B
void f(_BitInt(9));
#endif
