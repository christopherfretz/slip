// std: c++26
#include "side.h"
// §6 ExpansionData `decl`.
#ifdef B
void f() { template for (long e : {1, 2}) ; }
#else
void f() { template for (int e : {1, 2}) ; }
#endif
