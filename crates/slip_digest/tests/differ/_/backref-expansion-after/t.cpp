// std: c++26
#include "side.h"
// digest §4.3 back-references, `backref-expansion-after`: the shared body is the same; a statement after the expansion differs.
int g(int);
#ifdef B
void f() { template for (int e : {1, 2, 3}) { g(e); } g(7); }
#else
void f() { template for (int e : {1, 2, 3}) { g(e); } g(8); }
#endif
