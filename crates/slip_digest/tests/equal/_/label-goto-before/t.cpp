#include "side.h"
// WP-034 (§6): a `goto` before its label, the label renamed.
#ifdef B
void f() { goto m; m: ; }
#else
void f() { goto l; l: ; }
#endif
