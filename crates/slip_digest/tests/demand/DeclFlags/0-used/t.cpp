#include "side.h"
// §5.3 DeclFlags/0-used: a use sets it; no unit changes.
int x; inline int g() { return 1; }
#ifdef B
int y = x + g();
#else

#endif
