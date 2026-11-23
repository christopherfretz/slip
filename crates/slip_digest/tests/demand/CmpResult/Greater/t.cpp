#include "side.h"
// §10.6 CmpResult `Greater` (demand, §11.7): a built-in `<=>` on pointers (`strong_ordering`): kiln_sema `class/compare/mod.rs:307`; every unit stays equal.
#include "compare.h"
int g[2];
#ifdef B
auto y() { return &g[0] <=> &g[1]; }
#endif
