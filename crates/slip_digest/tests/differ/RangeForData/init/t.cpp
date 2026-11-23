// std: c++20
#include "side.h"
// §6 RangeForData `init`.
#ifdef B
void f(int (&a)[2]) { for (int k = 0; int e : a) ; }
#else
void f(int (&a)[2]) { for (int e : a) ; }
#endif
