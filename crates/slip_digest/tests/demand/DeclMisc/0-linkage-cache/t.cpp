#include "side.h"
// §5.3 DeclMisc/0-linkage-cache: a use sets it; no unit changes.
int x; inline int g() { return 1; }
#ifdef B
int y() { return x + g(); }
#else

#endif
