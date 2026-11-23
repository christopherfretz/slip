// std: c++26
#include "side.h"
// §6 ExpansionData `init`.
#ifdef B
void f() { template for (int k = 0; int e : {1, 2}) ; }
#else
void f() { template for (int e : {1, 2}) ; }
#endif
