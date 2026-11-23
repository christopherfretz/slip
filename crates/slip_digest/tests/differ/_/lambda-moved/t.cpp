#include "side.h"
// §4.3: a lambda moved to another function of the same shape: both units change.
#ifdef B
int f() { return 0; }
int g() { return [] { return 1; }(); }
#else
int f() { return [] { return 1; }(); }
int g() { return 0; }
#endif
