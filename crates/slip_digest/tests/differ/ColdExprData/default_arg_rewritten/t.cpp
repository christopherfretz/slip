#include "side.h"
// §7.4 ColdExprData `default_arg_rewritten`.
// the use of a default argument (expr_init.rs)
int fd(int x = __builtin_LINE());
#ifdef B

int v = fd();
#else
int v = fd();
#endif
