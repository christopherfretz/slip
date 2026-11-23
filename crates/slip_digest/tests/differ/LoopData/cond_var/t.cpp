#include "side.h"
// §6 LoopData `cond_var`.
#ifdef B
void f(int x) { while (int y = x + 1) ; }
#else
void f(int x) { while (int y = x) ; }
#endif
