#include "side.h"
// §6 IfData `els`.
#ifdef B
void f(int x) { if (x) ; else ; }
#else
void f(int x) { if (x) ; }
#endif
