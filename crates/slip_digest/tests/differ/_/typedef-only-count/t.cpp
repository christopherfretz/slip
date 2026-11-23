#include "side.h"
// WP-034 (§6): the count of a declaration statement.
#ifdef B
void f() { typedef int I; typedef int J; }
#else
void f() { typedef int I; }
#endif
