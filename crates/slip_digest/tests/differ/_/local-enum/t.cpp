#include "side.h"
// §4.3: a local enumeration and its enumerators.
#ifdef B
int f() { enum E { a, b = 2 }; return b; }
#else
int f() { enum E { a, b }; return b; }
#endif
