// std: c++26
#include "side.h"
// §6 ExpansionData `exprs`.
#ifdef B
void f() { template for (int e : {1, 2, 3}) ; }
#else
void f() { template for (int e : {1, 2}) ; }
#endif
