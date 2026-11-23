// std: c++23
#include "side.h"
// §10.1 AttrNode: `ASSUME.flags-ignored`: the fact differs.
int g();
#ifdef B
void f(int x) { [[assume(g() > 0)]]; }
#else
void f(int x) { [[assume(x > 0)]]; }
#endif
