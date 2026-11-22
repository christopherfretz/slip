#include "side.h"
// §2.3, WP-026: the defining declaration's completed type.
#ifdef B
extern int a[];
int a[4];
#else
extern int a[];
int a[3];
#endif
