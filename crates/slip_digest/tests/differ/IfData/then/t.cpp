#include "side.h"
// §6 IfData `then`.
#ifdef B
void f(int x) { if (x) return; else ; }
#else
void f(int x) { if (x) ; else ; }
#endif
