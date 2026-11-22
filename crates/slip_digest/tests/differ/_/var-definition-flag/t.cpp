#include "side.h"
// §5.3 DeclMisc `0-definition`, §2.3: a variable with a defining declaration against one without.
#ifdef B
extern int x; int x;
#else
extern int x;
#endif
