// std: c++26
#include "side.h"
// digest §4.3 back-references, `backref-expansion-inside`: an expansion statement's body is reached once per element; a call inside the shared body differs.
int g(int);
#ifdef B
void f() { template for (int e : {1, 2, 3}) { g(e + 1); } }
#else
void f() { template for (int e : {1, 2, 3}) { g(e + 2); } }
#endif
