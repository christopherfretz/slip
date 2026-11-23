// std: c++26
#include "side.h"
// §6 ExpansionData `body`.
#ifdef B
void f() { template for (int e : {1, 2}) { (void)e; } }
#else
void f() { template for (int e : {1, 2}) ; }
#endif
