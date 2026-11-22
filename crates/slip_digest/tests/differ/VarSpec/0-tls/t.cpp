#include "side.h"
// §5.4 VarSpec `0-tls`.
#ifdef B
thread_local int x;
#else
int x;
#endif
