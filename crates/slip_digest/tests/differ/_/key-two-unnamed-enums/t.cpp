#include "side.h"
// digest §3.4, WP-039 item 1: two unnamed enumerations in one scope: two keys.
enum { E1 };
#ifdef B
enum { E2 };
#endif
