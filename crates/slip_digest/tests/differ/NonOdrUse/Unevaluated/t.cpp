#include "side.h"
// §7.2 NonOdrUse `Unevaluated`.
int a, b;
// the operand of `sizeof` (expr_misc.rs)
#ifdef B
unsigned long v = sizeof(b);
#else
unsigned long v = sizeof(a);
#endif
