#include "side.h"
// §2.3: a variable is read from its defining declaration, here the second.
#ifdef B
extern int x; int x = 1;
#else
extern int x; int x;
#endif
