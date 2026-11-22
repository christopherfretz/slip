#include "side.h"
// §7.2 StorageDuration `Static`.
#ifdef B
thread_local const int &v = 1;
#else
const int &v = 1;
#endif
