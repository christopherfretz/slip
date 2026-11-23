#include "side.h"
// §6 LoopData `init`.
#ifdef B
void f(int x) { for (int i = 0; x;) ; }
#else
void f(int x) { for (; x;) ; }
#endif
