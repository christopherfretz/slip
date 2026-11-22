#include "side.h"
// §2.3, WP-026: a variable's type is the completed one, whichever
//  declaration is read: here no declaration is a definition and the
//  first is read.
#ifdef B
extern int a[];
extern int a[4];
#else
extern int a[];
extern int a[3];
#endif
