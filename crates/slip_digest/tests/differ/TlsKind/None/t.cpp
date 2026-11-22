#include "side.h"
// §5.4 TlsKind `None`.
#ifdef B
int x;
#else
__thread int x;
#endif
