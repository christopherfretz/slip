#include "side.h"
// §6 RangeForData `loop_var`.
#ifdef B
void f(int (&a)[2]) { for (const int e : a) ; }
#else
void f(int (&a)[2]) { for (int e : a) ; }
#endif
