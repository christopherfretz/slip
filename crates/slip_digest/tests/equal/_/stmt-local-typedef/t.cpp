#include "side.h"
// WP-034 (§6): a local typedef respelled.
#ifdef B
void f() { typedef signed int I; I x = 0; (void)x; }
#else
void f() { typedef int I; I x = 0; (void)x; }
#endif
