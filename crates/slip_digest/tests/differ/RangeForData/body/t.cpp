#include "side.h"
// §6 RangeForData `body`.
#ifdef B
void f(int (&a)[2]) { for (int e : a) return; }
#else
void f(int (&a)[2]) { for (int e : a) ; }
#endif
