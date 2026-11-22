#include "side.h"
// §5.4 TlsKind `Dynamic`: `thread_local`.
#ifdef B
thread_local int x;
#else
int x;
#endif
