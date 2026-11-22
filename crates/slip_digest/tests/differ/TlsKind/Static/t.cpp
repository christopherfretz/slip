#include "side.h"
// §5.4 TlsKind `Static`: `__thread`.
#ifdef B
__thread int x;
#else
thread_local int x;
#endif
