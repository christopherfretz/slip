#include "side.h"
// §7.2 StorageDuration `Automatic`.
// a temporary bound to a local reference
#ifdef B
int f() { static const int &r = 1; return r; }
#else
int f() { const int &r = 1; return r; }
#endif
