#include "side.h"
// digest §3.4, WP-039 item 1: two closure types at namespace scope: two keys.
auto l1 = [] { return 1; };
#ifdef B
auto l2 = [] { return 2; };
#endif
