// std: c++26
#include "side.h"
// digest §4.3 back-references, `backref-locals-renamed`: locals renamed around and inside nodes reached twice give the same digest.
int g(int);
#ifdef B
void f() { int k = 1; template for (int e : {1, 2, 3}) { g(e + k); } }
#else
void f() { int n = 1; template for (int x : {1, 2, 3}) { g(x + n); } }
#endif
