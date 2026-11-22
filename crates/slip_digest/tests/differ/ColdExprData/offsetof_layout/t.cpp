#include "side.h"
// §7.4 ColdExprData `offsetof_layout`.
// written by `expr_misc::offsetof_layout`
struct S { int m, n; };
#ifdef B
unsigned long v = __builtin_offsetof(S, n);
#else
unsigned long v = __builtin_offsetof(S, m);
#endif
