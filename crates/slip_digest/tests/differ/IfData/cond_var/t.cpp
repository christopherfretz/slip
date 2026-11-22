#include "side.h"
// §6 IfData `cond_var`.
#ifdef B
void f(int x) { if (int y = x + 1) ; }
#else
void f(int x) { if (int y = x) ; }
#endif
