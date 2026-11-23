#include "side.h"
// §6 LoopData `inc`.
#ifdef B
void f(int x) { for (;; ++x) ; }
#else
void f(int x) { for (;;) ; }
#endif
