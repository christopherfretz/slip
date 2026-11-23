#include "side.h"
// §6 RangeForData `range`.
#ifdef B
void f(int (&a)[2], int (&b)[2]) { for (int e : b) ; }
#else
void f(int (&a)[2], int (&b)[2]) { for (int e : a) ; }
#endif
