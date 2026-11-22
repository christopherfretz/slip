#include "side.h"
// §5.4 VarData `spec`.
#ifdef B
thread_local int x;
#else
int x;
#endif
