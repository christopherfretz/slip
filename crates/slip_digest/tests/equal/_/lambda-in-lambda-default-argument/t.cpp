#include "side.h"
// digest §4.3: a lambda in the default argument of a lambda's parameter,
// in a body; the parameters renamed.
#ifdef B
int f() { return [](int b = [](int q) { return q; }(1)) { return b; }(); }
#else
int f() { return [](int a = [](int p) { return p; }(1)) { return a; }(); }
#endif
