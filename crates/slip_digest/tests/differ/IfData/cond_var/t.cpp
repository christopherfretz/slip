#include "side.h"
// §6 IfData `cond_var`.
#ifdef B
void f(int x) { if (int y = x) ; }
#else
void f(int x) { if (x) ; }
#endif
