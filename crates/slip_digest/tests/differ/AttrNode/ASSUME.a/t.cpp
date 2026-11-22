// std: c++23
#include "side.h"
// §10.1 AttrNode: `ASSUME.a`: the fact differs.
#ifdef B
void f(int x) { [[assume(x > 1)]]; }
#else
void f(int x) { [[assume(x > 0)]]; }
#endif
