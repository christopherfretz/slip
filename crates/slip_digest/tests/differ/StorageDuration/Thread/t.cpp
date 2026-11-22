#include "side.h"
// §7.2 StorageDuration `Thread`.
#ifdef B
const int &v = 1;
#else
thread_local const int &v = 1;
#endif
