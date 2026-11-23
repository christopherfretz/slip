#include "side.h"
// §10.4 LambdaMangling `number`: a second closure of the same signature.
#ifdef B
int f() { return [] { return 1; }() + [] { return 1; }(); }
#else
int f() { return [] { return 1; }() + [](int) { return 1; }(0); }
#endif
