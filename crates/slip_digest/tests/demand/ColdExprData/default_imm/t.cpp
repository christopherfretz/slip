#include "side.h"
// §7.4 ColdExprData `default_imm`: a use of the default argument records
// its immediate invocations (§11.4); the use is on side B only, under a
// new name, and changes nothing else.
consteval int g() { return 1; }
int fd(int x = g());
int u = fd();
#ifdef B
int w = fd();
#endif
