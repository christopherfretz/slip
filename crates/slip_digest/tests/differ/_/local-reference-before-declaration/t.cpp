#include "side.h"
// §4.3: `local(n)` counts by declaration: f's type names the second class on side A, the first on side B.
#ifdef B
auto f() { struct T {}; struct S {}; return T{}; }
#else
auto f() { struct S {}; struct T {}; return T{}; }
#endif
