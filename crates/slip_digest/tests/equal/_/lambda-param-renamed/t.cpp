#include "side.h"
// §4.3: a lambda's parameter renamed.
#ifdef B
int f() { return [](int y) { return y; }(1); }
#else
int f() { return [](int x) { return x; }(1); }
#endif
