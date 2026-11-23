#include "side.h"
// digest §3.3, WP-039 item 1: an unnamed enumeration named by its first enumerator.
#ifdef B
enum { E3, E2 } ;
#else
enum { E1, E2 } ;
#endif
