// std: c++26
#include "side.h"
// digest §4.3 back-references, `backref-destructuring-after`: the shared pattern is the same; a statement after the expansions differs.
struct P { int a; int b; };
int g(int);
#ifdef B
void f() { P p{1, 2}; template for (auto e : p) { template for (auto k : p) { g(e + k); } } g(1); }
#else
void f() { P p{1, 2}; template for (auto e : p) { template for (auto k : p) { g(e + k); } } g(2); }
#endif
