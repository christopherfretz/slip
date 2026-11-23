#include "side.h"
// digest §3.4, WP-039 item 1: two unnamed enumerations with no enumerator: two keys.
enum : int {};
#ifdef B
enum : long {};
#endif
