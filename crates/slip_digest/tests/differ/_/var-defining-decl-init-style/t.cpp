#include "side.h"
// §2.3, §5.4 VarSpec `0-init-style`: read from the defining declaration.
#ifdef B
extern int x; int x{1};
#else
extern int x; int x = 1;
#endif
