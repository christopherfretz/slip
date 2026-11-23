#include "side.h"
// WP-034 (§6): a typedef writes nothing (§2.4): a declaration statement of one unused typedef writes its tag and its count.
#ifdef B
void f() { typedef long I; }
#else
void f() { typedef int I; }
#endif
