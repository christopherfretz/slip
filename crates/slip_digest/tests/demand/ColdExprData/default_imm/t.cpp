#include "side.h"
// §7.4 ColdExprData `default_imm`.
// §11.4: the default argument's immediate invocations (expr_init.rs)
consteval int g() { return 1; }
int fd(int x = g());
int u = fd();
#ifdef B
int v = fd();
int w = fd();
#else
int v = 0;
#endif
