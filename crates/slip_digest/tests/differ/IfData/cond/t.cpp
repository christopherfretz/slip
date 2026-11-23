#include "side.h"
// §6 IfData `cond`.
#ifdef B
void f(int x) { if (x == 2) ; }
#else
void f(int x) { if (x == 1) ; }
#endif
