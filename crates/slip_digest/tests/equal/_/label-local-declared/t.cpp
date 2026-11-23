#include "side.h"
// WP-034 (§6): a `__label__` label renamed.
#ifdef B
void f() { __label__ m; m: ; goto m; }
#else
void f() { __label__ l; l: ; goto l; }
#endif
