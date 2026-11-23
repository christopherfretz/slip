// std: c++26
#include "side.h"
// digest §4.3 back-references, `backref-destructuring-inside`: nested destructuring expansions share their pattern; a call inside the shared inner body differs.
struct P { int a; int b; };
int g(int);
#ifdef B
void f() { P p{1, 2}; template for (auto e : p) { template for (auto k : p) { g(e + k); } } }
#else
void f() { P p{1, 2}; template for (auto e : p) { template for (auto k : p) { g(e - k); } } }
#endif
