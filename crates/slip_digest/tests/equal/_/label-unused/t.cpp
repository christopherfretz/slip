#include "side.h"
// WP-034 (§6): a label nothing jumps to, renamed.
#ifdef B
void f() { m: ; }
#else
void f() { l: ; }
#endif
