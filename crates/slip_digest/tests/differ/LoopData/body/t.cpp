#include "side.h"
// §6 LoopData `body`.
#ifdef B
void f(int x) { while (x) return; }
#else
void f(int x) { while (x) ; }
#endif
