#include "side.h"
// digest §3.3, WP-039 item 1: unnamed enumerations without a name: their ordinal.
#ifdef B
struct S { enum : long {}; };
#else
struct S { enum : int {}; enum : long {}; };
#endif
