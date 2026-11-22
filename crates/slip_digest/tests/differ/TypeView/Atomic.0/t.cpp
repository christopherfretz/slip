#include "side.h"
// §4.3 `Atomic.0`.
#ifdef B
_Atomic(long) v;
#else
_Atomic(int) v;
#endif
