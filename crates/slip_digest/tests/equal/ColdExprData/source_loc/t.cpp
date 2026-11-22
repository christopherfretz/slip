#include "side.h"
// §7.4 ColdExprData: the values of `SourceLocValues` (expr_misc.rs)
#ifdef B

int v = __builtin_LINE();
#else
int v = __builtin_LINE();
#endif
